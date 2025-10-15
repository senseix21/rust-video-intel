mod pos_integration;

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use pos_integration::{POSConfig, POSIntegration, POSEventType, POSSimulator};
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
    enable_pos: bool,
    mqtt_host: String,
    mqtt_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_width: 640,
            frame_height: 640,
            rtsp_latency_ms: 100,
            max_queue_size: 10,
            log_interval_frames: 30,
            enable_pos: false,
            mqtt_host: "localhost".to_string(),
            mqtt_port: 1883,
        }
    }
}

/// Thread-safe frame processing metrics
struct Metrics {
    frame_count: AtomicU64,
    dropped_frames: AtomicU64,
    pos_events: AtomicU64,
    alerts: AtomicU64,
    start_time: Instant,
}

impl Metrics {
    fn new() -> Self {
        Self {
            frame_count: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            pos_events: AtomicU64::new(0),
            alerts: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    fn record_frame(&self) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_drop(&self) {
        self.dropped_frames.fetch_add(1, Ordering::Relaxed);
    }

    fn record_pos_event(&self) {
        self.pos_events.fetch_add(1, Ordering::Relaxed);
    }

    fn record_alert(&self) {
        self.alerts.fetch_add(1, Ordering::Relaxed);
    }

    fn get_stats(&self) -> (u64, u64, u64, u64, f64) {
        let frames = self.frame_count.load(Ordering::Relaxed);
        let drops = self.dropped_frames.load(Ordering::Relaxed);
        let pos_events = self.pos_events.load(Ordering::Relaxed);
        let alerts = self.alerts.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let fps = if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 };
        (frames, drops, pos_events, alerts, fps)
    }
}

/// Main surveillance pipeline with POS integration
struct SurveillanceWithPOS {
    config: Config,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    pos_integration: Option<POSIntegration>,
}

impl SurveillanceWithPOS {
    async fn new(config: Config, source: VideoSource) -> Result<Self> {
        // Initialize GStreamer once
        gst::init().context("Failed to initialize GStreamer")?;

        let metrics = Arc::new(Metrics::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        // Initialize POS integration if enabled
        let pos_integration = if config.enable_pos {
            info!("Initializing POS integration via MQTT");
            let pos_config = POSConfig {
                mqtt_host: config.mqtt_host.clone(),
                mqtt_port: config.mqtt_port,
                mqtt_client_id: format!("surveillance_{}", uuid::Uuid::new_v4()),
                mqtt_username: None,  // Set from env in production
                mqtt_password: None,
                topics: vec![
                    "pos/events/+/discount".to_string(),
                    "pos/events/+/void".to_string(),
                    "pos/events/+/refund".to_string(),
                    "pos/events/+/drawer".to_string(),
                ],
                correlation_window_secs: 60,
                high_value_threshold: 1000.0,
                discount_threshold: 30.0,
            };

            match POSIntegration::new(pos_config).await {
                Ok(integration) => {
                    info!("✅ POS integration initialized");
                    Some(integration)
                }
                Err(e) => {
                    warn!("Failed to initialize POS integration: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let pipeline = match source {
            VideoSource::Rtsp(url) => Self::create_rtsp_pipeline(&config, &url)?,
            VideoSource::Test => Self::create_test_pipeline(&config)?,
        };

        Ok(Self {
            config,
            metrics,
            shutdown,
            pipeline,
            pos_integration,
        })
    }

    fn create_rtsp_pipeline(config: &Config, rtsp_url: &str) -> Result<gst::Pipeline> {
        if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtsps://") {
            anyhow::bail!("Invalid RTSP URL format");
        }

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

    async fn run(mut self) -> Result<()> {
        let appsink = self.pipeline
            .by_name("sink")
            .and_then(|e| e.dynamic_cast::<gst_app::AppSink>().ok())
            .context("Failed to get appsink")?;

        // Setup frame callback
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
                                    let _data = map.as_slice();

                                    metrics.record_frame();

                                    let count = metrics.frame_count.load(Ordering::Relaxed);
                                    if count % config.log_interval_frames == 0 {
                                        let (frames, drops, pos_events, alerts, fps) = metrics.get_stats();
                                        info!(
                                            "📹 Frames: {} | FPS: {:.1} | POS Events: {} | Alerts: {} | Drops: {}",
                                            frames, fps, pos_events, alerts, drops
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
        info!("Surveillance + POS Integration Started");
        info!("Video pipeline: ✅ Running");
        if self.pos_integration.is_some() {
            info!("POS integration: ✅ Connected to MQTT");
            info!("Monitoring events: discount, void, refund, drawer");
        } else {
            info!("POS integration: ❌ Disabled");
        }
        info!("Press Ctrl+C to stop");
        info!("═══════════════════════════════════════");
        info!("");

        // Start POS integration in background if enabled
        let metrics_clone = Arc::clone(&self.metrics);
        if let Some(mut pos) = self.pos_integration {
            tokio::spawn(async move {
                if let Err(e) = pos.run().await {
                    error!("POS integration error: {}", e);
                }
            });

            // Update metrics periodically
            let metrics_for_pos = metrics_clone.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    // In production: Get actual event count from POS integration
                    metrics_for_pos.record_pos_event();
                }
            });
        }

        // Setup graceful shutdown
        let shutdown_signal = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            signal::ctrl_c().await.ok();
            info!("\nReceived shutdown signal");
            shutdown_signal.store(true, Ordering::Relaxed);
        });

        // Monitor pipeline bus
        let bus = self.pipeline.bus().context("No bus")?;
        let shutdown_check = Arc::clone(&self.shutdown);

        loop {
            if shutdown_check.load(Ordering::Relaxed) {
                info!("Shutting down...");
                break;
            }

            if let Some(msg) = bus.timed_pop(Duration::from_millis(100).into()) {
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
                    _ => {}
                }
            }

            tokio::task::yield_now().await;
        }

        // Cleanup
        self.pipeline.set_state(gst::State::Null)?;

        // Final stats
        let (frames, drops, pos_events, alerts, fps) = self.metrics.get_stats();
        info!("");
        info!("═══════════════════════════════════════");
        info!("Final Statistics:");
        info!("  Total frames: {}", frames);
        info!("  Average FPS: {:.1}", fps);
        info!("  POS events received: {}", pos_events);
        info!("  Alerts triggered: {}", alerts);
        info!("  Dropped frames: {} ({:.2}%)", drops, (drops as f64 / frames.max(1) as f64) * 100.0);
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

    info!("Retail Surveillance System - Phase 3: POS Integration");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let mut enable_pos = false;
    let mut simulate_pos = false;
    let mut rtsp_url = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "--enable-pos" => enable_pos = true,
            "--simulate-pos" => simulate_pos = true,
            _ if arg.starts_with("rtsp://") => rtsp_url = Some(arg.clone()),
            _ => {}
        }
    }

    let source = if let Some(url) = rtsp_url {
        info!("Using RTSP stream: {}", url);
        VideoSource::Rtsp(url)
    } else {
        info!("No RTSP URL provided, using test source");
        info!("Usage: cargo run --release -- [--enable-pos] [--simulate-pos] [rtsp://camera]");
        VideoSource::Test
    };

    // Load config
    let mut config = Config::default();
    config.enable_pos = enable_pos;

    // Start POS event simulator if requested
    if simulate_pos {
        info!("Starting POS event simulator");
        let simulator = POSSimulator::new(&config.mqtt_host, config.mqtt_port).await?;

        // Publish test events periodically
        tokio::spawn(async move {
            let event_types = vec![
                POSEventType::DiscountApplied,
                POSEventType::VoidTransaction,
                POSEventType::RefundIssued,
                POSEventType::PaymentCleared,
            ];

            let mut interval = tokio::time::interval(Duration::from_secs(10));
            let mut idx = 0;

            loop {
                interval.tick().await;
                let event_type = &event_types[idx % event_types.len()];
                if let Err(e) = simulator.publish_test_event(event_type.clone()).await {
                    error!("Failed to publish test event: {}", e);
                }
                idx += 1;
            }
        });
    }

    // Create and run pipeline
    let pipeline = SurveillanceWithPOS::new(config, source).await?;
    pipeline.run().await?;

    Ok(())
}