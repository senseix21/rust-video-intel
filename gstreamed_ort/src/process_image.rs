use std::path::Path;

use image::GenericImageView;
use inference_common::{frame_meta::FrameMeta, frame_times::FrameTimes};
use inference_common::detection_logger::{DetectionLog, DetectionLogger};
use inference_common::color_extractor;
use ort::session::Session;

use crate::inference;

/// Performs inference on a single image file.
pub fn process_image(path: &Path, mut session: Session) -> anyhow::Result<()> {
    let mut frame_times = FrameTimes::default();

    // Read image.
    let og_image = image::open(path)?;
    let (img_width, img_height) = og_image.dimensions();

    // Process image.
    let (img, bboxes) =
        inference::infer_on_image(&mut session, None, og_image.clone(), &mut frame_times)?;
    
    // Enhanced logging with color extraction
    let mut detection_logger = DetectionLogger::new();
    let mut frame_detections = Vec::new();
    
    println!("\nDetections in {:?}:", path);
    for (class_idx, class_bboxes) in bboxes.iter().enumerate() {
        for bbox in class_bboxes {
            // Extract dominant color for the detected object
            let dominant_color = color_extractor::extract_dominant_color(
                &og_image,
                bbox.xmin,
                bbox.ymin,
                bbox.xmax,
                bbox.ymax,
            );
            
            let detection = DetectionLog::from_bbox(
                0,
                0,
                bbox,
                class_idx,
                img_width as f32,
                img_height as f32,
                dominant_color,
            );
            
            frame_detections.push(detection.clone());
            detection_logger.log_detection(detection);
        }
    }
    
    // Print enhanced detection summary
    detection_logger.print_frame_summary(0, &frame_detections);
    
    // NB! For a single image, ort times will be misleading,
    // as the first time it's used, it does all kinds of lazy init.
    log::debug!("{frame_times:?}");
    
    // Save output: image & bboxes.
    let img_output_path = path.with_extension("out.jpg");
    img.save(img_output_path)?;
    
    let bbox_output_path = path.with_extension("out.json");
    let frame_meta = FrameMeta {
        pts: 0,
        dts: 0,
        bboxes_by_class: bboxes,
    };
    serde_json::to_writer(std::fs::File::create(bbox_output_path)?, &frame_meta)?;
    
    // Save detection logs
    let detections_path = path.with_extension("detections.json");
    detection_logger.export_json(&detections_path)?;
    println!("Detection logs saved to: {:?}", detections_path);

    Ok(())
}
