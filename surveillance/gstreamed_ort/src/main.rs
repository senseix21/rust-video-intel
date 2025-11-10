mod inference;
mod process_image;
mod process_video;
mod tui;

use std::path::PathBuf;

use clap::Parser;
use ort::execution_providers::CPUExecutionProvider;
use ort::execution_providers::CUDAExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::builder::SessionBuilder;
use tracing_subscriber::prelude::*;

#[derive(Debug, Parser)]
pub struct Args {
    /// Path to input image (.jpeg/.png) or video file (.mp4/.mkv).
    /// Use "webcam" or specify device path like "/dev/video0" for webcam input.
    input: PathBuf,
    /// Whether to attempt to use `cuda` hw acceleration.
    /// This may silently fail and fallback to cpu acceleration presently.
    #[arg(long, action, default_value = "false")]
    cuda: bool,
    /// Yolov8 onnx model file to use.
    #[arg(long, short, default_value = "_models/yolov8s.onnx")]
    model: String,
    /// Whether to live playback the inference results.
    #[arg(long, action, default_value = "false")]
    live: bool,
    /// Webcam device (e.g., /dev/video0). Use with input "webcam".
    #[arg(long, default_value = "/dev/video0")]
    device: String,
    /// Enable interactive TUI dashboard
    #[arg(long, action, default_value = "false")]
    tui: bool,
    /// Confidence threshold for detections (0.0-1.0). Higher = fewer false positives
    #[arg(long, default_value = "0.7")]
    conf_threshold: f32,
    /// NMS IoU threshold for removing duplicate detections (0.0-1.0)
    #[arg(long, default_value = "0.45")]
    nms_threshold: f32,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging - suppress if TUI is active
    if !args.tui {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn,gstreamed_ort=info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    } else {
        // For TUI mode, completely disable all logging output
        // This prevents any log output from interfering with the TUI
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("off"))
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::sink))
            .init();
        
        // Also disable log crate output
        log::set_max_level(log::LevelFilter::Off);
    }

    // Load model into ort.
    let (ep, ep_name) = if args.cuda {
        (CUDAExecutionProvider::default().build(), "cuda")
    } else {
        (CPUExecutionProvider::default().build(), "cpu")
    };
    // TODO test trt exec provider, but requires a rebuild of onnxruntime with trt enabled
    // TODO warmup with synthetic image of the same dims?

    ort::init().with_execution_providers([ep]).commit()?;

    let session = SessionBuilder::new()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        // .with_intra_threads(1)?
        .commit_from_file(&args.model)?;
    log::debug!("{session:?}");

    log::info!(
        "Prepared ort {ep_name} session with model: {:?}",
        args.model
    );
    
    log::info!(
        "Detection thresholds: confidence={:.2}, nms={:.2}",
        args.conf_threshold,
        args.nms_threshold
    );

    // Check if input is "webcam" or a device path
    let input_str = args.input.to_string_lossy();
    if input_str == "webcam" || input_str.starts_with("/dev/video") {
        let device = if input_str == "webcam" {
            &args.device
        } else {
            input_str.as_ref()
        };
        if args.tui {
            tui::process_webcam_with_tui(device, args.live, session, args.conf_threshold, args.nms_threshold)?;
        } else {
            process_video::process_webcam(device, args.live, session, args.conf_threshold, args.nms_threshold)?;
        }
    } else {
        match args.input.extension().and_then(|os_str| os_str.to_str()) {
            Some("mp4" | "mkv") => {
                if args.tui {
                    tui::process_video_with_tui(&args.input, args.live, session, args.conf_threshold, args.nms_threshold)?;
                } else {
                    process_video::process_video(&args.input, args.live, session, args.conf_threshold, args.nms_threshold)?;
                }
            }
            Some("jpeg" | "jpg" | "png") => process_image::process_image(&args.input, session, args.conf_threshold, args.nms_threshold)?,
            Some(unk) => log::error!("Unhandled file extension: {unk}"),
            None => log::error!(
                "Input path does not have valid file extension: {:?}",
                args.input
            ),
        }
    }

    Ok(())
}
