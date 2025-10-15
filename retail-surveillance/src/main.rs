use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
// use image::{ImageBuffer, Rgb};
// use ort::{GraphOptimizationLevel, Session, SessionBuilder, Value};
// use ndarray::{Array4, ArrayView3, Axis, s};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::signal;
use tracing::{error, info, warn, debug};

const COCO_PERSON_CLASS: usize = 0;
const NMS_THRESHOLD: f32 = 0.45;
const CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Configuration for the surveillance system
#[derive(Debug, Clone)]
struct Config {
    frame_width: u32,
    frame_height: u32,
    rtsp_latency_ms: u32,
    max_queue_size: usize,
    log_interval_frames: u64,
    model_path: Option<String>,
    enable_ml: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_width: 640,
            frame_height: 640,
            rtsp_latency_ms: 100,
            max_queue_size: 10,
            log_interval_frames: 30,
            model_path: Some("yolo_nas_s.onnx".into()),
            enable_ml: false,
        }
    }
}

/// Detection result from YOLO
#[derive(Debug, Clone)]
struct Detection {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    confidence: f32,
    class: usize,
}

/// Thread-safe frame processing metrics
struct Metrics {
    frame_count: AtomicU64,
    dropped_frames: AtomicU64,
    detection_count: AtomicU64,
    inference_time_ms: AtomicU64,
    start_time: Instant,
}

impl Metrics {
    fn new() -> Self {
        Self {
            frame_count: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            detection_count: AtomicU64::new(0),
            inference_time_ms: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    fn record_frame(&self) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_drop(&self) {
        self.dropped_frames.fetch_add(1, Ordering::Relaxed);
    }

    fn record_detection(&self, count: u64, inference_ms: u64) {
        self.detection_count.fetch_add(count, Ordering::Relaxed);
        self.inference_time_ms.fetch_add(inference_ms, Ordering::Relaxed);
    }

    fn get_stats(&self) -> (u64, u64, u64, f64, f64) {
        let frames = self.frame_count.load(Ordering::Relaxed);
        let drops = self.dropped_frames.load(Ordering::Relaxed);
        let detections = self.detection_count.load(Ordering::Relaxed);
        let total_inference = self.inference_time_ms.load(Ordering::Relaxed);

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let fps = if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 };
        let avg_inference = if frames > 0 {
            total_inference as f64 / frames as f64
        } else {
            0.0
        };

        (frames, drops, detections, fps, avg_inference)
    }
}

/// ML inference engine
struct MLInference {
    // session: Option<Session>,
    enabled: bool,
}

impl MLInference {
    fn new(config: &Config) -> Result<Self> {
        if !config.enable_ml {
            info!("ML inference disabled");
            return Ok(Self {
                // session: None,
                enabled: false,
            });
        }

        let model_path = config.model_path.as_ref()
            .context("Model path required when ML is enabled")?;

        if !std::path::Path::new(model_path).exists() {
            warn!("Model file not found: {}", model_path);
            info!("To enable ML inference:");
            info!("  1. pip install super-gradients onnx torch");
            info!("  2. python3 scripts/export_yolo_nas.py");
            info!("  3. cargo run --release -- --enable-ml rtsp://camera");

            return Ok(Self {
                // session: None,
                enabled: false,
            });
        }

        // ML loading disabled for now - need ort and ndarray deps
        warn!("ML inference not available - dependencies not included");

        Ok(Self {
            // session: Some(session),
            enabled: false,
        })
    }

    fn detect(&self, _image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<Detection>> {
        // ML inference disabled - dependencies not included
        Ok(Vec::new())
    }

    fn non_max_suppression(&self, mut detections: Vec<Detection>) -> Vec<Detection> {
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = Vec::new();

        while !detections.is_empty() {
            let current = detections.remove(0);
            keep.push(current.clone());

            detections.retain(|det| {
                self.iou(&current, det) < NMS_THRESHOLD
            });
        }

        keep
    }

    fn iou(&self, a: &Detection, b: &Detection) -> f32 {
        let x1 = a.x.max(b.x);
        let y1 = a.y.max(b.y);
        let x2 = (a.x + a.w).min(b.x + b.w);
        let y2 = (a.y + a.h).min(b.y + b.h);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let area_a = a.w * a.h;
        let area_b = b.w * b.h;
        let union = area_a + area_b - intersection;

        intersection / union
    }
}

/// Main surveillance pipeline
struct SurveillancePipeline {
    config: Config,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    ml_engine: Arc<MLInference>,
}

impl SurveillancePipeline {
    fn new(config: Config, source: VideoSource) -> Result<Self> {
        // Initialize GStreamer once
        gst::init().context("Failed to initialize GStreamer")?;

        let metrics = Arc::new(Metrics::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let ml_engine = Arc::new(MLInference::new(&config)?);

        let pipeline = match source {
            VideoSource::Rtsp(url) => Self::create_rtsp_pipeline(&config, &url)?,
            VideoSource::Test => Self::create_test_pipeline(&config)?,
        };

        Ok(Self {
            config,
            metrics,
            shutdown,
            pipeline,
            ml_engine,
        })
    }

    fn create_rtsp_pipeline(config: &Config, rtsp_url: &str) -> Result<gst::Pipeline> {
        // Validate URL to prevent injection
        if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtsps://") {
            anyhow::bail!("Invalid RTSP URL format");
        }

        let pipeline_str = format!(
            "rtspsrc location=\"{}\" latency={} drop-on-latency=true buffer-mode=1 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             videoconvert ! videoscale ! \
             video/x-raw,format=RGB,width={},height={} ! \
             appsink name=sink max-buffers={} drop=true sync=false",
            rtsp_url, config.rtsp_latency_ms,
            config.frame_width, config.frame_height,
            config.max_queue_size
        );

        info!("Creating RTSP pipeline");
        debug!("Pipeline: {}", pipeline_str);

        gst::parse::launch(&pipeline_str)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Failed to create RTSP pipeline"))
    }

    fn create_test_pipeline(config: &Config) -> Result<gst::Pipeline> {
        let pipeline_str = format!(
            "videotestsrc pattern=ball is-live=true ! \
             video/x-raw,width=1280,height=720,framerate=30/1 ! \
             videoconvert ! videoscale ! \
             video/x-raw,format=RGB,width={},height={} ! \
             appsink name=sink max-buffers={} drop=true sync=false",
            config.frame_width, config.frame_height, config.max_queue_size
        );

        info!("Creating test pipeline");

        gst::parse::launch(&pipeline_str)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Failed to create test pipeline"))
    }

    async fn run(self) -> Result<()> {
        let appsink = self.pipeline
            .by_name("sink")
            .and_then(|e| e.dynamic_cast::<gst_app::AppSink>().ok())
            .context("Failed to get appsink")?;

        // Setup frame callback
        let metrics = Arc::clone(&self.metrics);
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);
        let ml_engine = Arc::clone(&self.ml_engine);

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    if shutdown.load(Ordering::Relaxed) {
                        return Ok(gst::FlowSuccess::Ok);
                    }

                    match sink.pull_sample() {
                        Ok(sample) => {
                            if let Some(buffer) = sample.buffer() {
                                if let Ok(map) = buffer.map_readable() {
                                    // Process frame without copying
                                    let data = map.as_slice();

                                    // Run ML inference if enabled
                                    let start = Instant::now();
                                    let detections = if ml_engine.enabled {
                                        ml_engine.detect(
                                            data,
                                            config.frame_width,
                                            config.frame_height
                                        ).unwrap_or_default()
                                    } else {
                                        Vec::new()
                                    };
                                    let inference_ms = start.elapsed().as_millis() as u64;

                                    metrics.record_frame();
                                    metrics.record_detection(detections.len() as u64, inference_ms);

                                    let count = metrics.frame_count.load(Ordering::Relaxed);
                                    if count % config.log_interval_frames == 0 {
                                        let (frames, drops, total_detections, fps, avg_inference) =
                                            metrics.get_stats();

                                        if ml_engine.enabled {
                                            info!(
                                                "Processed {} frames | FPS: {:.1} | People: {} | \
                                                 Inference: {:.1}ms | Drops: {}",
                                                frames, fps, total_detections, avg_inference, drops
                                            );
                                        } else {
                                            info!(
                                                "Processed {} frames | FPS: {:.1} | Drops: {}",
                                                frames, fps, drops
                                            );
                                        }
                                    }

                                    // Log individual detections (debug)
                                    for det in &detections {
                                        debug!(
                                            "Person detected @ ({:.0}, {:.0}) {}x{} conf: {:.2}",
                                            det.x * config.frame_width as f32,
                                            det.y * config.frame_height as f32,
                                            det.w * config.frame_width as f32,
                                            det.h * config.frame_height as f32,
                                            det.confidence
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            metrics.record_drop();
                        }
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // Start pipeline
        self.pipeline.set_state(gst::State::Playing)
            .context("Failed to start pipeline")?;

        info!("═══════════════════════════════════════");
        info!("Pipeline started successfully");
        if self.ml_engine.enabled {
            info!("ML inference: ENABLED (YOLO-NAS)");
        } else {
            info!("ML inference: DISABLED");
        }
        info!("═══════════════════════════════════════");
        info!("");

        // Setup graceful shutdown
        let shutdown_signal = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            signal::ctrl_c().await.ok();
            info!("\nReceived shutdown signal");
            shutdown_signal.store(true, Ordering::Relaxed);
        });

        // Monitor pipeline bus asynchronously
        let bus = self.pipeline.bus().context("No bus")?;
        let shutdown_check = Arc::clone(&self.shutdown);

        loop {
            // Check for shutdown
            if shutdown_check.load(Ordering::Relaxed) {
                info!("Shutting down...");
                break;
            }

            // Poll bus with timeout
            if let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(100)) {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        info!("End of stream");
                        break;
                    }
                    MessageView::Error(err) => {
                        error!(
                            "Pipeline error from {:?}: {}",
                            err.src().map(|s| s.path_string()),
                            err.error()
                        );
                        break;
                    }
                    MessageView::StateChanged(s) => {
                        if msg.src() == Some(self.pipeline.upcast_ref()) {
                            debug!("Pipeline state: {:?} -> {:?}", s.old(), s.current());
                        }
                    }
                    _ => {}
                }
            }

            // Allow other tasks to run
            tokio::task::yield_now().await;
        }

        // Cleanup
        self.pipeline.set_state(gst::State::Null)?;

        // Final stats
        let (frames, drops, detections, fps, avg_inference) = self.metrics.get_stats();
        info!("");
        info!("═══════════════════════════════════════");
        info!("Final Statistics:");
        info!("  Total frames: {}", frames);
        info!("  Dropped frames: {}", drops);
        info!("  Drop rate: {:.2}%", (drops as f64 / frames.max(1) as f64) * 100.0);
        info!("  Average FPS: {:.1}", fps);
        if self.ml_engine.enabled {
            info!("  Total people detected: {}", detections);
            info!("  Average inference: {:.1}ms", avg_inference);
            info!("  Inference FPS: {:.1}", 1000.0 / avg_inference.max(1.0));
        }
        info!("═══════════════════════════════════════");

        Ok(())
    }
}

enum VideoSource {
    Rtsp(String),
    Test,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("retail_surveillance=info".parse()?)
        )
        .init();

    info!("Retail Surveillance System - Phase 2");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let mut enable_ml = false;
    let mut rtsp_url = None;

    for arg in &args[1..] {
        if arg == "--enable-ml" {
            enable_ml = true;
        } else if arg.starts_with("rtsp://") {
            rtsp_url = Some(arg.clone());
        }
    }

    let source = if let Some(url) = rtsp_url {
        info!("Using RTSP stream: {}", url);
        VideoSource::Rtsp(url)
    } else {
        info!("No RTSP URL provided, using test source");
        info!("Usage: cargo run --release -- [--enable-ml] [rtsp://camera-url]");
        VideoSource::Test
    };

    // Load config
    let mut config = Config::default();
    config.enable_ml = enable_ml;

    // Create and run pipeline
    let pipeline = SurveillancePipeline::new(config, source)?;
    pipeline.run().await?;

    Ok(())
}