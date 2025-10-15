use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::signal;
use tracing::{error, info, warn, debug};

/// Configuration for the surveillance system
#[derive(Debug, Clone)]
struct Config {
    frame_width: u32,
    frame_height: u32,
    rtsp_latency_ms: u32,
    max_queue_size: usize,
    log_interval_frames: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_width: 640,
            frame_height: 640,
            rtsp_latency_ms: 100,
            max_queue_size: 10,
            log_interval_frames: 30,
        }
    }
}

/// Thread-safe frame processing metrics using lock-free atomics
struct Metrics {
    frame_count: AtomicU64,
    dropped_frames: AtomicU64,
    start_time: Instant,
}

impl Metrics {
    fn new() -> Self {
        Self {
            frame_count: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    fn record_frame(&self) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_drop(&self) {
        self.dropped_frames.fetch_add(1, Ordering::Relaxed);
    }

    fn get_stats(&self) -> (u64, u64, f64) {
        let frames = self.frame_count.load(Ordering::Relaxed);
        let drops = self.dropped_frames.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let fps = if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 };
        (frames, drops, fps)
    }
}

/// Main surveillance pipeline
struct SurveillancePipeline {
    config: Config,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
}

impl SurveillancePipeline {
    fn new(config: Config, source: VideoSource) -> Result<Self> {
        // Initialize GStreamer once
        gst::init().context("Failed to initialize GStreamer")?;

        let metrics = Arc::new(Metrics::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let pipeline = match source {
            VideoSource::Rtsp(url) => Self::create_rtsp_pipeline(&config, &url)?,
            VideoSource::Test => Self::create_test_pipeline(&config)?,
        };

        Ok(Self {
            config,
            metrics,
            shutdown,
            pipeline,
        })
    }

    fn create_rtsp_pipeline(config: &Config, rtsp_url: &str) -> Result<gst::Pipeline> {
        // Validate URL to prevent injection
        if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtsps://") {
            anyhow::bail!("Invalid RTSP URL format: must start with rtsp:// or rtsps://");
        }

        // Escape quotes in URL for safety
        let safe_url = rtsp_url.replace('"', "");

        let pipeline_str = format!(
            "rtspsrc location=\"{}\" latency={} drop-on-latency=true buffer-mode=1 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             videoconvert ! videoscale ! \
             video/x-raw,format=RGB,width={},height={} ! \
             appsink name=sink max-buffers={} drop=true sync=false",
            safe_url, config.rtsp_latency_ms,
            config.frame_width, config.frame_height,
            config.max_queue_size
        );

        info!("Creating RTSP pipeline for: {}", rtsp_url);
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
            .context("Failed to get appsink from pipeline")?;

        // Setup frame callback with zero-copy processing
        let metrics = Arc::clone(&self.metrics);
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);

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
                                    // Process frame without copying (zero-copy)
                                    let _data = map.as_slice();
                                    let _size = _data.len();

                                    metrics.record_frame();

                                    let count = metrics.frame_count.load(Ordering::Relaxed);
                                    if count % config.log_interval_frames == 0 {
                                        let (frames, drops, fps) = metrics.get_stats();
                                        info!(
                                            "Processed {} frames | Real FPS: {:.1} | Dropped: {} ({:.1}%)",
                                            frames,
                                            fps,
                                            drops,
                                            (drops as f64 / frames as f64) * 100.0
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
        info!("Press Ctrl+C to stop gracefully");
        info!("═══════════════════════════════════════");
        info!("");

        // Setup graceful shutdown handler
        let shutdown_signal = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            signal::ctrl_c().await.ok();
            info!("\nReceived shutdown signal");
            shutdown_signal.store(true, Ordering::Relaxed);
        });

        // Monitor pipeline bus asynchronously (non-blocking)
        let bus = self.pipeline.bus().context("Pipeline has no bus")?;
        let shutdown_check = Arc::clone(&self.shutdown);

        loop {
            // Check for shutdown
            if shutdown_check.load(Ordering::Relaxed) {
                info!("Shutting down gracefully...");
                break;
            }

            // Poll bus with timeout (non-blocking)
            if let Some(msg) = bus.timed_pop(Duration::from_millis(100).into()) {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        info!("End of stream");
                        break;
                    }
                    MessageView::Error(err) => {
                        error!(
                            "Pipeline error from {:?}: {} (debug: {:?})",
                            err.src().map(|s| s.path_string()),
                            err.error(),
                            err.debug()
                        );
                        break;
                    }
                    MessageView::StateChanged(s) => {
                        if msg.src() == Some(self.pipeline.upcast_ref()) {
                            debug!("Pipeline state: {:?} -> {:?}", s.old(), s.current());
                        }
                    }
                    MessageView::Warning(w) => {
                        warn!(
                            "Warning from {:?}: {} (debug: {:?})",
                            w.src().map(|s| s.path_string()),
                            w.error(),
                            w.debug()
                        );
                    }
                    _ => {}
                }
            }

            // Yield to other tasks for cooperative multitasking
            tokio::task::yield_now().await;
        }

        // Cleanup pipeline
        self.pipeline.set_state(gst::State::Null)?;

        // Final statistics
        let (frames, drops, fps) = self.metrics.get_stats();
        info!("");
        info!("═══════════════════════════════════════");
        info!("Final Statistics:");
        info!("  Total frames: {}", frames);
        info!("  Dropped frames: {}", drops);
        info!("  Drop rate: {:.2}%", (drops as f64 / frames.max(1) as f64) * 100.0);
        info!("  Average FPS: {:.1}", fps);
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
    // Initialize tracing with environment filter
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("retail_surveillance=info".parse()?)
        )
        .init();

    info!("Retail Surveillance System - Improved Design");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let rtsp_url = args.get(1).filter(|s| s.starts_with("rtsp://"));

    let source = if let Some(url) = rtsp_url {
        info!("Using RTSP stream: {}", url);
        VideoSource::Rtsp(url.clone())
    } else {
        info!("No RTSP URL provided, using test source");
        info!("Usage: cargo run --release -- rtsp://camera-url");
        VideoSource::Test
    };

    // Load configuration (could be extended to read from file/env)
    let config = Config::default();

    // Create and run surveillance pipeline
    let pipeline = SurveillancePipeline::new(config, source)?;
    pipeline.run().await?;

    Ok(())
}