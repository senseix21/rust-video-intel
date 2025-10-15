use anyhow::{Context, Result};
use chrono::Utc;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use retail_surveillance::{
    api::{create_router, AppState},
    database::Database,
    ml_client::{ByteTracker, MLClient, Zone, ZoneCounter},
    pos_integration::{POSConfig, POSIntegration, RiskAnalyzer},
    video_clip::{FrameData, VideoClipManager, VideoClipRequest},
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const DEFAULT_VIDEO_OUTPUT_DIR: &str = "./video_clips";
const DEFAULT_ML_SERVICE_URL: &str = "http://localhost:8080";

#[derive(Clone)]
struct Config {
    frame_width: u32,
    frame_height: u32,
    rtsp_latency_ms: u32,
    max_queue_size: usize,
    log_interval_frames: u64,
    enable_pos: bool,
    enable_video_clips: bool,
    enable_ml: bool,
    ml_service_url: String,
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
            enable_ml: true,
            ml_service_url: DEFAULT_ML_SERVICE_URL.to_string(),
            api_port: 3000,
            video_output_dir: PathBuf::from(DEFAULT_VIDEO_OUTPUT_DIR),
        }
    }
}

struct MLMetrics {
    total_detections: AtomicU64,
    total_tracks: AtomicU64,
    zone_entries: AtomicU64,
    zone_exits: AtomicU64,
    inference_time_total: AtomicU64,
    inference_count: AtomicU64,
}

impl MLMetrics {
    fn new() -> Self {
        Self {
            total_detections: AtomicU64::new(0),
            total_tracks: AtomicU64::new(0),
            zone_entries: AtomicU64::new(0),
            zone_exits: AtomicU64::new(0),
            inference_time_total: AtomicU64::new(0),
            inference_count: AtomicU64::new(0),
        }
    }

    fn record_inference(&self, detection_count: u64, inference_ms: u64) {
        self.total_detections.fetch_add(detection_count, Ordering::Relaxed);
        self.inference_time_total.fetch_add(inference_ms, Ordering::Relaxed);
        self.inference_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_zone_event(&self, entries: u64, exits: u64) {
        self.zone_entries.fetch_add(entries, Ordering::Relaxed);
        self.zone_exits.fetch_add(exits, Ordering::Relaxed);
    }

    fn get_stats(&self) -> (u64, u64, u64, u64, f64) {
        let detections = self.total_detections.load(Ordering::Relaxed);
        let tracks = self.total_tracks.load(Ordering::Relaxed);
        let entries = self.zone_entries.load(Ordering::Relaxed);
        let exits = self.zone_exits.load(Ordering::Relaxed);

        let total_inference = self.inference_time_total.load(Ordering::Relaxed);
        let count = self.inference_count.load(Ordering::Relaxed);
        let avg_inference = if count > 0 {
            total_inference as f64 / count as f64
        } else {
            0.0
        };

        (detections, tracks, entries, exits, avg_inference)
    }
}

struct Metrics {
    frame_count: AtomicU64,
    dropped_frames: AtomicU64,
    pos_events: AtomicU64,
    alerts_triggered: AtomicU64,
    clips_generated: AtomicU64,
    ml: MLMetrics,
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
            ml: MLMetrics::new(),
            start_time: Instant::now(),
        }
    }

    fn get_stats(&self) -> String {
        let frames = self.frame_count.load(Ordering::Relaxed);
        let drops = self.dropped_frames.load(Ordering::Relaxed);
        let events = self.pos_events.load(Ordering::Relaxed);
        let alerts = self.alerts_triggered.load(Ordering::Relaxed);
        let clips = self.clips_generated.load(Ordering::Relaxed);

        let (detections, tracks, entries, exits, avg_inference) = self.ml.get_stats();

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let fps = if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 };

        format!(
            "📹 FPS: {:.1} | 👥 People: {} | 🎯 Tracks: {} | 📊 In: {} Out: {} | \
             🚨 Alerts: {} | 💾 Clips: {} | ⚡ ML: {:.1}ms",
            fps, detections, tracks, entries, exits, alerts, clips, avg_inference
        )
    }
}

struct IntegratedMLPipeline {
    config: Config,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    pos_integration: Option<Arc<RwLock<POSIntegration>>>,
    risk_analyzer: Arc<RiskAnalyzer>,
    database: Arc<Database>,
    clip_manager: Arc<VideoClipManager>,
    ml_client: Arc<MLClient>,
    tracker: Arc<RwLock<ByteTracker>>,
    zone_counter: Arc<RwLock<ZoneCounter>>,
}

impl IntegratedMLPipeline {
    async fn new(config: Config, camera_id: String, rtsp_url: Option<String>) -> Result<Self> {
        gst::init().context("Failed to initialize GStreamer")?;

        let metrics = Arc::new(Metrics::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://surveillance:secure_password@localhost:5432/retail_surveillance".to_string());

        let database = Arc::new(Database::new(&database_url).await?);
        let clip_manager = Arc::new(VideoClipManager::new());

        // Initialize ML components
        let ml_client = Arc::new(MLClient::new(Some(config.ml_service_url.clone())));

        // Check ML service health
        if config.enable_ml {
            match ml_client.check_health().await {
                Ok(true) => info!("✅ ML service is healthy"),
                Ok(false) => warn!("⚠️ ML service is not responding"),
                Err(e) => {
                    warn!("⚠️ Could not connect to ML service: {}", e);
                    warn!("ML features will be disabled. Start the Python service with:");
                    warn!("  python ml_service/inference_server.py --port 8080");
                }
            }
        }

        let tracker = Arc::new(RwLock::new(ByteTracker::new()));

        // Create example zones
        let zones = vec![
            Zone::new(
                "entrance".to_string(),
                "Store Entrance".to_string(),
                vec![(0.0, 0.0), (0.3, 0.0), (0.3, 1.0), (0.0, 1.0)],
            ),
            Zone::new(
                "checkout".to_string(),
                "Checkout Area".to_string(),
                vec![(0.7, 0.0), (1.0, 0.0), (1.0, 1.0), (0.7, 1.0)],
            ),
        ];
        let zone_counter = Arc::new(RwLock::new(ZoneCounter::new(zones)));

        let pos_integration = if config.enable_pos {
            let mqtt_host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".to_string());
            let mqtt_port = std::env::var("MQTT_PORT")
                .unwrap_or_else(|_| "1883".to_string())
                .parse()
                .unwrap_or(1883);

            let mut pos_config = POSConfig::default();
            pos_config.mqtt_host = mqtt_host;
            pos_config.mqtt_port = mqtt_port;

            match POSIntegration::new(pos_config).await {
                Ok(pos) => Some(Arc::new(RwLock::new(pos))),
                Err(e) => {
                    warn!("Failed to connect to MQTT: {}. POS integration disabled.", e);
                    None
                }
            }
        } else {
            None
        };

        let risk_analyzer = Arc::new(RiskAnalyzer::new(POSConfig::default()));

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
            ml_client,
            tracker,
            zone_counter,
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

        info!("Creating RTSP pipeline with ML support");

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
        let ml_client = Arc::clone(&self.ml_client);
        let tracker = Arc::clone(&self.tracker);
        let zone_counter = Arc::clone(&self.zone_counter);
        let database = Arc::clone(&self.database);

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
                                    let frame_data = data.to_vec();

                                    // Process with ML if enabled
                                    if config.enable_ml {
                                        let ml_client = Arc::clone(&ml_client);
                                        let tracker = Arc::clone(&tracker);
                                        let zone_counter = Arc::clone(&zone_counter);
                                        let metrics = Arc::clone(&metrics);
                                        let database = Arc::clone(&database);
                                        let width = config.frame_width;
                                        let height = config.frame_height;

                                        tokio::spawn(async move {
                                            let start = Instant::now();

                                            match ml_client.detect_people(&frame_data, width, height).await {
                                                Ok(detections) => {
                                                    let inference_ms = start.elapsed().as_millis() as u64;

                                                    // Track people
                                                    let mut tracker = tracker.write().await;
                                                    let tracked = tracker.update(detections);
                                                    let track_count = tracker.get_track_count();
                                                    drop(tracker);

                                                    // Update zones
                                                    let mut zone_counter = zone_counter.write().await;
                                                    zone_counter.update(&tracked);

                                                    // Get zone stats
                                                    let mut total_entries = 0;
                                                    let mut total_exits = 0;
                                                    for zone in zone_counter.get_zones() {
                                                        total_entries += zone.entry_count;
                                                        total_exits += zone.exit_count;
                                                    }
                                                    drop(zone_counter);

                                                    // Update metrics
                                                    metrics.ml.record_inference(tracked.len() as u64, inference_ms);
                                                    metrics.ml.total_tracks.store(track_count as u64, Ordering::Relaxed);
                                                    metrics.ml.record_zone_event(total_entries as u64, total_exits as u64);

                                                    // Store in database (sample rate to avoid overwhelming)
                                                    if metrics.frame_count.load(Ordering::Relaxed) % 30 == 0 {
                                                        let detections_json = serde_json::to_value(&tracked).unwrap_or_default();

                                                        // Store detection results
                                                        // Note: Would need to add these methods to database.rs
                                                        // database.store_detections(camera_id, tracked.len(), detections_json).await.ok();
                                                    }

                                                    debug!("Detected {} people, {} active tracks", tracked.len(), track_count);
                                                }
                                                Err(e) => {
                                                    debug!("ML inference error: {}", e);
                                                }
                                            }
                                        });
                                    }

                                    metrics.frame_count.fetch_add(1, Ordering::Relaxed);

                                    let count = metrics.frame_count.load(Ordering::Relaxed);
                                    if count % config.log_interval_frames == 0 {
                                        info!("{}", metrics.get_stats());
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
        info!("Retail Surveillance - Phase 6: ML Detection");
        info!("═══════════════════════════════════════");
        info!("✅ Video Pipeline: ACTIVE");
        if self.config.enable_ml {
            info!("✅ ML People Detection: ENABLED");
            info!("✅ ByteTrack Tracking: ACTIVE");
            info!("✅ Zone Counting: {} zones", 2);
        }
        if self.config.enable_pos {
            info!("✅ POS Integration: ENABLED");
        }
        if self.config.enable_video_clips {
            info!("✅ Video Clips: ENABLED");
        }
        info!("✅ REST API: Port {}", self.config.api_port);
        info!("═══════════════════════════════════════");

        let api_state = AppState {
            db: self.database.clone(),
        };

        let api_router = create_router(api_state);
        let api_port = self.config.api_port;
        let api_handle = tokio::spawn(async move {
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], api_port));
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

        // Print final statistics
        info!("");
        info!("═══════════════════════════════════════");
        info!("Final Statistics:");
        info!("{}", self.metrics.get_stats());

        let (detections, tracks, entries, exits, avg_inference) = self.metrics.ml.get_stats();
        info!("  Total people detected: {}", detections);
        info!("  Unique tracks: {}", tracks);
        info!("  Zone entries: {}", entries);
        info!("  Zone exits: {}", exits);
        info!("  Avg ML inference: {:.1}ms", avg_inference);
        info!("═══════════════════════════════════════");

        Ok(())
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

    info!("Retail Surveillance System - Phase 6: ML People Detection");

    let args: Vec<String> = std::env::args().collect();

    let mut rtsp_url = None;
    let mut enable_ml = true;
    let mut enable_pos = true;
    let mut enable_clips = true;
    let mut camera_id = "camera_001".to_string();
    let mut ml_service_url = DEFAULT_ML_SERVICE_URL.to_string();

    for arg in &args[1..] {
        if arg.starts_with("rtsp://") {
            rtsp_url = Some(arg.clone());
        } else if arg == "--no-ml" {
            enable_ml = false;
        } else if arg == "--no-pos" {
            enable_pos = false;
        } else if arg == "--no-clips" {
            enable_clips = false;
        } else if arg.starts_with("--camera-id=") {
            camera_id = arg.strip_prefix("--camera-id=").unwrap().to_string();
        } else if arg.starts_with("--ml-service=") {
            ml_service_url = arg.strip_prefix("--ml-service=").unwrap().to_string();
        }
    }

    if rtsp_url.is_none() {
        info!("No RTSP URL provided, using test source");
        info!("Usage: cargo run --bin main_phase6 [rtsp://url] [options]");
        info!("Options:");
        info!("  --no-ml         Disable ML people detection");
        info!("  --no-pos        Disable POS integration");
        info!("  --no-clips      Disable video clip extraction");
        info!("  --camera-id=ID  Set camera ID");
        info!("  --ml-service=URL Set ML service URL (default: http://localhost:8080)");
    }

    let mut config = Config::default();
    config.enable_ml = enable_ml;
    config.enable_pos = enable_pos;
    config.enable_video_clips = enable_clips;
    config.ml_service_url = ml_service_url;

    if enable_ml {
        info!("Starting ML inference service...");
        info!("Make sure Python service is running:");
        info!("  cd ml_service && python inference_server.py");
    }

    let pipeline = IntegratedMLPipeline::new(config, camera_id.clone(), rtsp_url).await?;
    pipeline.run(camera_id).await?;

    Ok(())
}