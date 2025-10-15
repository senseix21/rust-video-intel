use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use retail_surveillance::{
    api::{create_router, AppState},
    database::Database,
    pos_integration::{POSEventType, POSIntegration, RiskAnalyzer},
    video_clip::{FrameData, VideoClipManager, VideoClipRequest},
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::signal;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const DEFAULT_VIDEO_OUTPUT_DIR: &str = "./video_clips";
const DEFAULT_BUFFER_DURATION_SECS: i64 = 120;
const CORRELATION_WINDOW_SECS: i64 = 60;

struct Config {
    frame_width: u32,
    frame_height: u32,
    rtsp_latency_ms: u32,
    max_queue_size: usize,
    log_interval_frames: u64,
    enable_pos: bool,
    enable_video_clips: bool,
    api_port: u16,
    video_output_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_width: 640,
            frame_height: 640,
            rtsp_latency_ms: 100,
            max_queue_size: 10,
            log_interval_frames: 30,
            enable_pos: true,
            enable_video_clips: true,
            api_port: 3000,
            video_output_dir: PathBuf::from(DEFAULT_VIDEO_OUTPUT_DIR),
        }
    }
}

struct Metrics {
    frame_count: AtomicU64,
    dropped_frames: AtomicU64,
    pos_events: AtomicU64,
    alerts_triggered: AtomicU64,
    clips_generated: AtomicU64,
    start_time: Instant,
}

impl Metrics {
    fn new() -> Self {
        Self {
            frame_count: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            pos_events: AtomicU64::new(0),
            alerts_triggered: AtomicU64::new(0),
            clips_generated: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    fn get_stats(&self) -> (u64, u64, u64, u64, u64, f64) {
        let frames = self.frame_count.load(Ordering::Relaxed);
        let drops = self.dropped_frames.load(Ordering::Relaxed);
        let events = self.pos_events.load(Ordering::Relaxed);
        let alerts = self.alerts_triggered.load(Ordering::Relaxed);
        let clips = self.clips_generated.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let fps = if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 };

        (frames, drops, events, alerts, clips, fps)
    }
}

struct IntegratedPipeline {
    config: Config,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    pos_integration: Option<Arc<RwLock<POSIntegration>>>,
    risk_analyzer: Arc<RiskAnalyzer>,
    database: Arc<Database>,
    clip_manager: Arc<VideoClipManager>,
    clip_sender: Option<mpsc::Sender<VideoClipRequest>>,
}

impl IntegratedPipeline {
    async fn new(config: Config, camera_id: String, rtsp_url: Option<String>) -> Result<Self> {
        gst::init().context("Failed to initialize GStreamer")?;

        let metrics = Arc::new(Metrics::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://surveillance:secure_password@localhost:5432/retail_surveillance".to_string());

        let database = Arc::new(Database::new(&database_url).await?);

        let clip_manager = Arc::new(VideoClipManager::new());
        let clip_sender = if config.enable_video_clips {
            Some(clip_manager.add_camera(camera_id.clone(), config.video_output_dir.clone()))
        } else {
            None
        };

        let pos_integration = if config.enable_pos {
            let mqtt_host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".to_string());
            let mqtt_port = std::env::var("MQTT_PORT")
                .unwrap_or_else(|_| "1883".to_string())
                .parse()
                .unwrap_or(1883);

            let mut pos = POSIntegration::new(&mqtt_host, mqtt_port, database.clone());
            pos.connect().await?;
            pos.subscribe(&["pos/events/+/+".to_string()]).await?;
            Some(Arc::new(RwLock::new(pos)))
        } else {
            None
        };

        let risk_analyzer = Arc::new(RiskAnalyzer::new());

        let pipeline = if let Some(url) = rtsp_url {
            Self::create_rtsp_pipeline(&config, &url)?
        } else {
            Self::create_test_pipeline(&config)?
        };

        Ok(Self {
            config,
            metrics,
            shutdown,
            pipeline,
            pos_integration,
            risk_analyzer,
            database,
            clip_manager,
            clip_sender,
        })
    }

    fn create_rtsp_pipeline(config: &Config, rtsp_url: &str) -> Result<gst::Pipeline> {
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

        info!("Creating RTSP pipeline for video clips");

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

    async fn run(self, camera_id: String) -> Result<()> {
        let appsink = self.pipeline
            .by_name("sink")
            .and_then(|e| e.dynamic_cast::<gst_app::AppSink>().ok())
            .context("Failed to get appsink")?;

        let metrics = Arc::clone(&self.metrics);
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);
        let clip_sender = self.clip_sender.clone();
        let clip_manager = Arc::clone(&self.clip_manager);
        let database = Arc::clone(&self.database);

        let video_buffer = if config.enable_video_clips {
            let extractor = clip_manager.add_camera(camera_id.clone(), config.video_output_dir.clone());
            Some(retail_surveillance::video_clip::VideoBuffer::new(camera_id.clone(), DEFAULT_BUFFER_DURATION_SECS))
        } else {
            None
        };

        let video_buffer = Arc::new(video_buffer);
        let buffer_for_callback = Arc::clone(&video_buffer);

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
                                    let data = map.as_slice();

                                    if let Some(ref vb) = buffer_for_callback.as_ref() {
                                        let frame = FrameData {
                                            timestamp: Utc::now(),
                                            data: data.to_vec(),
                                            width: config.frame_width,
                                            height: config.frame_height,
                                        };
                                        vb.add_frame(frame);
                                    }

                                    metrics.frame_count.fetch_add(1, Ordering::Relaxed);

                                    let count = metrics.frame_count.load(Ordering::Relaxed);
                                    if count % config.log_interval_frames == 0 {
                                        let (frames, drops, events, alerts, clips, fps) = metrics.get_stats();
                                        info!(
                                            "📹 Frames: {} | FPS: {:.1} | POS Events: {} | Alerts: {} | Clips: {} | Drops: {}",
                                            frames, fps, events, alerts, clips, drops
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            metrics.dropped_frames.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        self.pipeline.set_state(gst::State::Playing)
            .context("Failed to start pipeline")?;

        info!("═══════════════════════════════════════");
        info!("Retail Surveillance System - Phase 5");
        info!("═══════════════════════════════════════");
        info!("✅ Video Pipeline: ACTIVE");
        if self.config.enable_pos {
            info!("✅ POS Integration: ENABLED");
        }
        if self.config.enable_video_clips {
            info!("✅ Video Clip Extraction: ENABLED");
        }
        info!("✅ REST API: Port {}", self.config.api_port);
        info!("═══════════════════════════════════════");

        if self.config.enable_pos {
            if let Some(pos) = &self.pos_integration {
                let pos_handle = self.spawn_pos_handler(pos.clone(), video_buffer.clone()).await;
            }
        }

        let api_state = AppState {
            db: self.database.clone(),
        };

        let api_router = create_router(api_state);
        let api_handle = tokio::spawn(async move {
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.api_port));
            info!("REST API listening on {}", addr);

            if let Err(e) = axum::serve(
                tokio::net::TcpListener::bind(addr).await.unwrap(),
                api_router
            ).await {
                error!("API server error: {}", e);
            }
        });

        let shutdown_signal = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            signal::ctrl_c().await.ok();
            info!("\nReceived shutdown signal");
            shutdown_signal.store(true, Ordering::Relaxed);
        });

        let bus = self.pipeline.bus().context("No bus")?;
        let shutdown_check = Arc::clone(&self.shutdown);

        loop {
            if shutdown_check.load(Ordering::Relaxed) {
                info!("Shutting down...");
                break;
            }

            if let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(100)) {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        info!("End of stream");
                        break;
                    }
                    MessageView::Error(err) => {
                        error!("Pipeline error: {}", err.error());
                        break;
                    }
                    _ => {}
                }
            }

            tokio::task::yield_now().await;
        }

        self.pipeline.set_state(gst::State::Null)?;

        let (frames, drops, events, alerts, clips, fps) = self.metrics.get_stats();
        info!("");
        info!("═══════════════════════════════════════");
        info!("Final Statistics:");
        info!("  Total frames: {}", frames);
        info!("  Dropped frames: {}", drops);
        info!("  Drop rate: {:.2}%", (drops as f64 / frames.max(1) as f64) * 100.0);
        info!("  Average FPS: {:.1}", fps);
        info!("  POS events: {}", events);
        info!("  Alerts triggered: {}", alerts);
        info!("  Video clips: {}", clips);
        info!("═══════════════════════════════════════");

        Ok(())
    }

    async fn spawn_pos_handler(
        &self,
        pos: Arc<RwLock<POSIntegration>>,
        video_buffer: Arc<Option<retail_surveillance::video_clip::VideoBuffer>>,
    ) -> tokio::task::JoinHandle<()> {
        let metrics = Arc::clone(&self.metrics);
        let risk_analyzer = Arc::clone(&self.risk_analyzer);
        let database = Arc::clone(&self.database);
        let clip_manager = Arc::clone(&self.clip_manager);
        let clip_sender = self.clip_sender.clone();

        tokio::spawn(async move {
            let mut pos_guard = pos.write().await;
            let mut receiver = pos_guard.get_receiver();
            drop(pos_guard);

            while let Some(event) = receiver.recv().await {
                metrics.pos_events.fetch_add(1, Ordering::Relaxed);

                let risk_score = risk_analyzer.calculate_risk_score(&event);

                if let Err(e) = database.insert_pos_event(&event).await {
                    error!("Failed to insert POS event: {}", e);
                }

                if risk_score >= 0.4 {
                    metrics.alerts_triggered.fetch_add(1, Ordering::Relaxed);

                    warn!("🚨 ALERT: Suspicious activity detected!");
                    warn!("     Type: {:?}", event.event_type);
                    warn!("     Order ID: {}", event.order_id);
                    warn!("     Staff: {}", event.staff_id);
                    warn!("     Risk Score: {:.2} / 1.00", risk_score);

                    let alert_id = Uuid::new_v4();
                    if let Err(e) = database.create_risk_alert(
                        event.id,
                        risk_score,
                        format!("{:?}", event.event_type),
                    ).await {
                        error!("Failed to create risk alert: {}", e);
                    }

                    if let Some(sender) = &clip_sender {
                        let request = VideoClipRequest {
                            id: Uuid::new_v4(),
                            timestamp: event.timestamp,
                            duration_before_secs: 30,
                            duration_after_secs: 30,
                            pos_event_id: Some(event.id),
                            alert_id: Some(alert_id),
                            camera_id: "camera_001".to_string(),
                            priority: retail_surveillance::video_clip::ClipPriority::High,
                        };

                        if let Err(e) = sender.send(request).await {
                            error!("Failed to request video clip: {}", e);
                        } else {
                            metrics.clips_generated.fetch_add(1, Ordering::Relaxed);
                            info!("📹 Video clip requested for alert {}", alert_id);
                        }
                    }
                }
            }
        })
    }
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

    info!("Retail Surveillance System - Phase 5: Video Clip Extraction");

    let args: Vec<String> = std::env::args().collect();

    let mut rtsp_url = None;
    let mut enable_pos = true;
    let mut enable_clips = true;
    let mut camera_id = "camera_001".to_string();

    for arg in &args[1..] {
        if arg.starts_with("rtsp://") {
            rtsp_url = Some(arg.clone());
        } else if arg == "--no-pos" {
            enable_pos = false;
        } else if arg == "--no-clips" {
            enable_clips = false;
        } else if arg.starts_with("--camera-id=") {
            camera_id = arg.strip_prefix("--camera-id=").unwrap().to_string();
        }
    }

    if rtsp_url.is_none() {
        info!("No RTSP URL provided, using test source");
        info!("Usage: cargo run --bin main_phase5 [rtsp://url] [--no-pos] [--no-clips] [--camera-id=ID]");
    }

    let mut config = Config::default();
    config.enable_pos = enable_pos;
    config.enable_video_clips = enable_clips;

    let pipeline = IntegratedPipeline::new(config, camera_id.clone(), rtsp_url).await?;
    pipeline.run(camera_id).await?;

    Ok(())
}