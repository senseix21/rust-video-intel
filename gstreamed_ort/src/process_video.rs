use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gstreamed_common::{discovery, pipeline::build_pipeline};
use gstreamer::{self as gst};
use gstreamer::{prelude::*, MessageView};
use image::{DynamicImage, RgbImage};
use inference_common::frame_meta::FrameMeta;
use inference_common::frame_times::{AggregatedTimes, FrameTimes};
use inference_common::img_dimensions::ImgDimensions;
use inference_common::tracker::similari::prelude::Sort;
use inference_common::video_meta::VideoMeta;
use inference_common::detection_logger::{DetectionLog, DetectionLogger};
use inference_common::onnx_attributes::AttributeDetector;
use ort::session::Session;

use crate::inference;

pub fn process_buffer(
    frame_dims: ImgDimensions,
    session: &mut Session,
    // TODO make tracking optional
    tracker: &Mutex<Sort>,
    agg_times: &mut AggregatedTimes,
    video_meta: &mut VideoMeta,
    detection_logger: &mut DetectionLogger,
    buffer: &mut gst::Buffer,
    attr_detector: &mut AttributeDetector,
) {
    let mut frame_times = FrameTimes::default();

    let start = Instant::now();
    // read buffer into an image
    let image = {
        let readable = buffer.map_readable().unwrap();
        let readable_vec = readable.to_vec();

        // buffer size is: width x height x 3
        let image = RgbImage::from_vec(
            frame_dims.width as u32,
            frame_dims.height as u32,
            readable_vec,
        )
        .unwrap();
        DynamicImage::ImageRgb8(image)
    };
    frame_times.frame_to_buffer = start.elapsed();

    // process it using some model + draw overlays on the output image
    let mut tracker = tracker.lock().unwrap();
    let (processed, bboxes) =
        inference::infer_on_image(session, Some(&mut *tracker), image.clone(), &mut frame_times).unwrap();
    
    // Enhanced logging with color extraction
    let frame_num = video_meta.frames.len() as u64;
    let timestamp_ms = buffer.pts().unwrap_or_default().mseconds();
    let mut frame_detections = Vec::new();
    
    for (class_idx, class_bboxes) in bboxes.iter().enumerate() {
        for bbox in class_bboxes {
            // Get class name for this detection
            let class_name = inference_common::coco_classes::NAMES
                .get(class_idx)
                .unwrap_or(&"unknown");
            
            // Extract attributes using ONNX model
            let attributes = attr_detector.detect_attributes(
                &image,
                bbox.xmin,
                bbox.ymin,
                bbox.xmax,
                bbox.ymax,
                class_name,
            ).unwrap_or_default();
            
            let detection = DetectionLog::from_bbox_with_attributes(
                frame_num,
                timestamp_ms,
                bbox,
                class_idx,
                frame_dims.width,
                frame_dims.height,
                attributes,
            );
            
            frame_detections.push(detection.clone());
            detection_logger.log_detection(detection);
        }
    }
    
    // Print frame summary with enhanced formatting
    detection_logger.print_frame_summary(frame_num, &frame_detections);
    
    let frame_meta = FrameMeta {
        pts: buffer.pts().unwrap_or_default().into(),
        dts: buffer.dts().unwrap_or_default().into(),
        bboxes_by_class: bboxes,
    };
    video_meta.push(frame_meta);

    // overwrite the buffer with our overlaid processed image
    let start = Instant::now();
    let buffer_mut = buffer.get_mut().unwrap();
    let mut writable = buffer_mut.map_writable().unwrap();
    let mut dst = writable.as_mut_slice();
    dst.write_all(processed.to_rgb8().as_raw()).unwrap();
    frame_times.buffer_to_frame = start.elapsed();

    log::debug!("{frame_times:?}");
    agg_times.push(frame_times);
}

/// Performs inference on a video file, using a gstreamer pipeline + ort.
pub fn process_video(input: &Path, live_playback: bool, session: Session) -> anyhow::Result<()> {
    gst::init()?;

    let agg_times = Arc::new(Mutex::new(AggregatedTimes::default()));

    // First, find out resolution of input file.
    log::info!("Discovering media properties of {input:?}");
    let file_info = discovery::discover(input)?;
    log::info!("{file_info:?}");
    let frame_dims = ImgDimensions::new(file_info.width as f32, file_info.height as f32);

    let output_path = input.with_extension("out.mkv");

    // Configure tracker, we use similari library, which provides iou/sort trackers.
    let tracker = inference_common::tracker::sort_tracker();
    
    // Create attribute detector
    let attr_detector = Arc::new(Mutex::new(
        AttributeDetector::new(None, None).expect("Failed to initialize attribute detector")
    ));
    
    // Create detection logger
    let detection_logger = Arc::new(Mutex::new(DetectionLogger::new()));

    // Build gst pipeline, which performs inference using the loaded model.
    let scoped_agg = Arc::clone(&agg_times);
    let video_meta = Arc::new(Mutex::new(VideoMeta::new(
        input.to_path_buf(),
        Some(output_path.clone()),
        frame_dims.width as u32,
        frame_dims.height as u32,
    )));
    let scoped_meta = Arc::clone(&video_meta);
    let scoped_logger = Arc::clone(&detection_logger);
    let scoped_attr = Arc::clone(&attr_detector);
    // FIXME can we do it without Mutex? it's not gonna be contested much, tho...
    let session = Arc::new(Mutex::new(session));
    let pipeline = build_pipeline(
        input.to_str().unwrap(),
        output_path.to_str().unwrap(),
        live_playback,
        move |buf| {
            let mut agg_times = scoped_agg.lock().unwrap();
            let mut video_meta = scoped_meta.lock().unwrap();
            let mut session = session.lock().unwrap();
            let mut logger = scoped_logger.lock().unwrap();
            let mut attr_detector = scoped_attr.lock().unwrap();
            process_buffer(
                frame_dims,
                &mut session,
                &tracker,
                &mut agg_times,
                &mut video_meta,
                &mut logger,
                buf,
                &mut attr_detector,
            );
        },
    )?;
    log::info!("Starting gst pipeline");

    // Make it play and listen to events to know when it's done.
    pipeline.set_state(gst::State::Playing).unwrap();

    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            MessageView::Error(err) => {
                pipeline.debug_to_dot_file(gst::DebugGraphDetails::all(), "pipeline.error");
                let name = err.src().map(|e| e.name().to_string());
                log::error!("Error from element {name:?}: {}", err.error());
                break;
            }
            MessageView::Eos(..) => {
                log::info!("Pipeline reached end of stream.");
                break;
            }
            _ => (),
        }
    }

    let video_meta = video_meta.lock().unwrap();
    let output_json_path = input.with_extension("json");
    log::info!(
        "Writing output json file, {} frames: {output_json_path:?}",
        video_meta.frames.len()
    );
    serde_json::to_writer(std::fs::File::create(&output_json_path)?, &*video_meta)?;
    
    // Export detection logs
    let detection_logger = detection_logger.lock().unwrap();
    let detections_path = input.with_extension("detections.json");
    log::info!("Writing detection logs: {detections_path:?}");
    detection_logger.export_json(&detections_path)?;

    pipeline.set_state(gst::State::Null).unwrap();

    // Print perf stats, ignoring first (outlier) frame.
    let agg = agg_times.lock().unwrap();
    let avg = agg.avg(true);
    log::info!("Average frame times: {avg:?}");

    let min = agg.min(true);
    log::info!("Min frame times: {min:?}");

    let max = agg.max(true);
    log::info!("Max frame times: {max:?}");

    Ok(())
}

/// Performs inference on webcam stream
pub fn process_webcam(device: &str, live_playback: bool, session: Session) -> anyhow::Result<()> {
    gst::init()?;

    let agg_times = Arc::new(Mutex::new(AggregatedTimes::default()));
    
    // For webcam, we'll detect dimensions from the first buffer
    // Start with a default that will be updated
    let frame_dims = Arc::new(Mutex::new(ImgDimensions::new(640.0, 480.0)));
    let dims_detected = Arc::new(Mutex::new(false));
    
    log::info!("Starting webcam inference from device: {device}");
    
    let tracker = inference_common::tracker::sort_tracker();
    let detection_logger = Arc::new(Mutex::new(DetectionLogger::new()));
    let attr_detector = Arc::new(Mutex::new(
        AttributeDetector::new(None, None).expect("Failed to initialize attribute detector")
    ));
    let scoped_agg = Arc::clone(&agg_times);
    let scoped_dims = Arc::clone(&frame_dims);
    let scoped_detected = Arc::clone(&dims_detected);
    let scoped_logger = Arc::clone(&detection_logger);
    let scoped_attr = Arc::clone(&attr_detector);
    let session = Arc::new(Mutex::new(session));
    let frame_count = Arc::new(Mutex::new(0u64));
    
    let pipeline = gstreamed_common::pipeline::build_webcam_pipeline(
        device,
        live_playback,
        move |buf| {
            // Detect dimensions from buffer size if not yet detected
            let dims = {
                let detected = scoped_detected.lock().unwrap();
                if !*detected {
                    drop(detected);
                    let readable = buf.map_readable().unwrap();
                    let buffer_size = readable.len();
                    drop(readable);
                    
                    // RGB format: buffer_size = width * height * 3
                    // Common webcam resolutions to try
                    let common_resolutions = [
                        (640, 480),
                        (1280, 720),
                        (1920, 1080),
                        (800, 600),
                        (320, 240),
                    ];
                    
                    for (w, h) in common_resolutions {
                        if w * h * 3 == buffer_size {
                            let mut dims_lock = scoped_dims.lock().unwrap();
                            *dims_lock = ImgDimensions::new(w as f32, h as f32);
                            log::info!("Detected webcam resolution: {}x{}", w, h);
                            let mut detected_lock = scoped_detected.lock().unwrap();
                            *detected_lock = true;
                            break;
                        }
                    }
                }
                *scoped_dims.lock().unwrap()
            };
            
            let mut frame_times = FrameTimes::default();
            let start = Instant::now();
            
            // Read buffer into an image
            let image = {
                let readable = buf.map_readable().unwrap();
                let readable_vec = readable.to_vec();
                
                let image = RgbImage::from_vec(
                    dims.width as u32,
                    dims.height as u32,
                    readable_vec,
                );
                
                if let Some(img) = image {
                    DynamicImage::ImageRgb8(img)
                } else {
                    log::error!("Failed to create image from buffer with dims {}x{}", dims.width, dims.height);
                    return;
                }
            };
            frame_times.frame_to_buffer = start.elapsed();
            
            // Process with inference
            let mut session = session.lock().unwrap();
            let mut tracker = tracker.lock().unwrap();
            let (processed, bboxes) = match inference::infer_on_image(&mut *session, Some(&mut *tracker), image.clone(), &mut frame_times) {
                Ok(result) => result,
                Err(e) => {
                    log::error!("Inference error: {}", e);
                    return;
                }
            };
            
            // Enhanced logging with color extraction
            let mut frame_num = frame_count.lock().unwrap();
            *frame_num += 1;
            let timestamp_ms = buf.pts().unwrap_or_default().mseconds();
            let mut frame_detections = Vec::new();
            
            for (class_idx, class_bboxes) in bboxes.iter().enumerate() {
                for bbox in class_bboxes {
                    // Get class name for this detection
                    let class_name = inference_common::coco_classes::NAMES
                        .get(class_idx)
                        .unwrap_or(&"unknown");
                    
                    // Extract attributes using ONNX model
                    let mut attr_detector = scoped_attr.lock().unwrap();
                    let attributes = attr_detector.detect_attributes(
                        &image,
                        bbox.xmin,
                        bbox.ymin,
                        bbox.xmax,
                        bbox.ymax,
                        class_name,
                    ).unwrap_or_default();
                    
                    let detection = DetectionLog::from_bbox_with_attributes(
                        *frame_num,
                        timestamp_ms,
                        bbox,
                        class_idx,
                        dims.width,
                        dims.height,
                        attributes,
                    );
                    
                    frame_detections.push(detection.clone());
                }
            }
            
            // Print frame summary with enhanced formatting
            if !frame_detections.is_empty() {
                let mut logger = scoped_logger.lock().unwrap();
                for detection in &frame_detections {
                    logger.log_detection(detection.clone());
                }
                logger.print_frame_summary(*frame_num, &frame_detections);
            }
            
            // Overwrite the buffer with processed image
            let start = Instant::now();
            if let Some(buffer_mut) = buf.get_mut() {
                if let Ok(mut writable) = buffer_mut.map_writable() {
                    let mut dst = writable.as_mut_slice();
                    let processed_raw = processed.to_rgb8();
                    let src = processed_raw.as_raw();
                    
                    // Only write if sizes match
                    if dst.len() == src.len() {
                        if let Err(e) = dst.write_all(src) {
                            log::error!("Failed to write processed frame: {}", e);
                        }
                    } else {
                        log::warn!("Buffer size mismatch: dst={}, src={}", dst.len(), src.len());
                    }
                } else {
                    log::error!("Failed to map buffer as writable");
                }
            } else {
                log::error!("Failed to get mutable buffer");
            }
            frame_times.buffer_to_frame = start.elapsed();
            
            log::debug!("{frame_times:?}");
            let mut agg = scoped_agg.lock().unwrap();
            agg.push(frame_times);
        },
    )?;
    
    log::info!("Starting webcam pipeline");
    pipeline.set_state(gst::State::Playing).unwrap();
    
    let bus = pipeline.bus().unwrap();
    println!("Webcam inference running. Press Ctrl+C to stop.");
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            MessageView::Error(err) => {
                pipeline.debug_to_dot_file(gst::DebugGraphDetails::all(), "pipeline.error");
                let name = err.src().map(|e| e.name().to_string());
                log::error!("Error from element {name:?}: {}", err.error());
                break;
            }
            MessageView::Eos(..) => {
                log::info!("Pipeline reached end of stream.");
                break;
            }
            _ => (),
        }
    }
    
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Print perf stats
    let agg = agg_times.lock().unwrap();
    let avg = agg.avg(true);
    log::info!("Average frame times: {avg:?}");
    
    let min = agg.min(true);
    log::info!("Min frame times: {min:?}");
    
    let max = agg.max(true);
    log::info!("Max frame times: {max:?}");
    
    Ok(())
}
