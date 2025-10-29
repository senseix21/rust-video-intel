//! Enhanced detection logging with explicit object identification and attributes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::bbox::Bbox;
use crate::coco_classes;
use crate::onnx_attributes::AttributeDetector;

/// Color information extracted from bounding box region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorInfo {
    pub dominant_color: String,
    pub rgb: (u8, u8, u8),
    pub color_name: String,
}

/// Extended attributes for detected objects
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectAttributes {
    pub color_info: Option<ColorInfo>,
    pub position: Position,
    pub size: Size,
    pub person_attrs: Option<PersonAttributesLog>,
    pub custom_metadata: HashMap<String, String>,
}

/// Person-specific attributes from neural network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonAttributesLog {
    pub gender: Option<String>,
    pub gender_confidence: Option<f32>,
    pub age_group: Option<String>,
    pub age_confidence: Option<f32>,
    pub upper_body_color: Option<String>,
    pub lower_body_color: Option<String>,
}

/// Position information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Position {
    pub x_center: f32,
    pub y_center: f32,
    pub area: f32,
}

/// Size information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
    pub relative_size: f32, // Percentage of frame
}

/// Enhanced detection log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionLog {
    pub frame_number: u64,
    pub timestamp_ms: u64,
    pub object_id: String,
    pub tracker_id: Option<i64>,
    pub class_name: String,
    pub confidence: f32,
    pub bbox: BBoxCoords,
    pub attributes: ObjectAttributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BBoxCoords {
    pub xmin: f32,
    pub ymin: f32,
    pub xmax: f32,
    pub ymax: f32,
}

impl ColorInfo {
    /// Create color info from RGB values
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            dominant_color: format!("rgb({}, {}, {})", r, g, b),
            rgb: (r, g, b),
            color_name: Self::rgb_to_color_name(r, g, b),
        }
    }

    /// Convert RGB to human-readable color name
    fn rgb_to_color_name(r: u8, g: u8, b: u8) -> String {
        // Simple color classification
        let (r, g, b) = (r as f32, g as f32, b as f32);
        
        // Calculate brightness
        let brightness = (r + g + b) / 3.0;
        
        if brightness < 50.0 {
            return "black".to_string();
        }
        if brightness > 200.0 {
            return "white".to_string();
        }
        
        // Determine dominant color
        let max_val = r.max(g).max(b);
        let min_val = r.min(g).min(b);
        let diff = max_val - min_val;
        
        if diff < 30.0 {
            if brightness < 128.0 {
                return "gray".to_string();
            } else {
                return "light_gray".to_string();
            }
        }
        
        if r == max_val {
            if g > b * 1.5 {
                "orange".to_string()
            } else if g > b {
                "yellow".to_string()
            } else {
                "red".to_string()
            }
        } else if g == max_val {
            if r > b * 1.2 {
                "yellow".to_string()
            } else {
                "green".to_string()
            }
        } else {
            if r > g * 1.2 {
                "purple".to_string()
            } else {
                "blue".to_string()
            }
        }
    }
}

impl DetectionLog {
    /// Create a detection log from a bbox with ONNX-based attribute detection
    pub fn from_bbox_with_detector(
        frame_number: u64,
        timestamp_ms: u64,
        bbox: &Bbox,
        class_idx: usize,
        frame_width: f32,
        frame_height: f32,
        image: &image::DynamicImage,
        attr_detector: &mut AttributeDetector,
    ) -> Self {
        let class_name = coco_classes::NAMES
            .get(class_idx)
            .unwrap_or(&"unknown")
            .to_string();
        
        let width = bbox.xmax - bbox.xmin;
        let height = bbox.ymax - bbox.ymin;
        let area = width * height;
        let frame_area = frame_width * frame_height;
        
        let object_id = format!(
            "{}_{}",
            class_name,
            bbox.tracker_id.map_or_else(
                || format!("untracked_{}", frame_number),
                |id| id.to_string()
            )
        );
        
        // Use ONNX model for color detection
        let color_info = attr_detector
            .classify_color(image, (bbox.xmin, bbox.ymin, bbox.xmax, bbox.ymax))
            .ok()
            .map(|color_class| ColorInfo {
                dominant_color: format!(
                    "rgb({}, {}, {})",
                    color_class.rgb_estimate.0,
                    color_class.rgb_estimate.1,
                    color_class.rgb_estimate.2
                ),
                rgb: color_class.rgb_estimate,
                color_name: color_class.color_name,
            });
        
        // Extract person-specific attributes if this is a person
        let person_attrs = if class_name == "person" {
            attr_detector
                .extract_person_attributes(image, (bbox.xmin, bbox.ymin, bbox.xmax, bbox.ymax))
                .ok()
                .map(|attrs| PersonAttributesLog {
                    gender: attrs.gender.as_ref().map(|(g, _)| g.clone()),
                    gender_confidence: attrs.gender.as_ref().map(|(_, c)| *c),
                    age_group: attrs.age_group.as_ref().map(|(a, _)| a.clone()),
                    age_confidence: attrs.age_group.as_ref().map(|(_, c)| *c),
                    upper_body_color: attrs.upper_color,
                    lower_body_color: attrs.lower_color,
                })
        } else {
            None
        };
        
        let attributes = ObjectAttributes {
            color_info,
            position: Position {
                x_center: (bbox.xmin + bbox.xmax) / 2.0,
                y_center: (bbox.ymin + bbox.ymax) / 2.0,
                area,
            },
            size: Size {
                width,
                height,
                relative_size: (area / frame_area) * 100.0,
            },
            person_attrs,
            custom_metadata: HashMap::new(),
        };
        
        Self {
            frame_number,
            timestamp_ms,
            object_id,
            tracker_id: bbox.tracker_id,
            class_name,
            confidence: bbox.detector_confidence,
            bbox: BBoxCoords {
                xmin: bbox.xmin,
                ymin: bbox.ymin,
                xmax: bbox.xmax,
                ymax: bbox.ymax,
            },
            attributes,
        }
    }

    /// Legacy method for backward compatibility (uses simple color extraction)
    pub fn from_bbox(
        frame_number: u64,
        timestamp_ms: u64,
        bbox: &Bbox,
        class_idx: usize,
        frame_width: f32,
        frame_height: f32,
        dominant_color_rgb: Option<(u8, u8, u8)>,
    ) -> Self {
        let class_name = coco_classes::NAMES
            .get(class_idx)
            .unwrap_or(&"unknown")
            .to_string();
        
        let width = bbox.xmax - bbox.xmin;
        let height = bbox.ymax - bbox.ymin;
        let area = width * height;
        let frame_area = frame_width * frame_height;
        
        let object_id = format!(
            "{}_{}",
            class_name,
            bbox.tracker_id.map_or_else(
                || format!("untracked_{}", frame_number),
                |id| id.to_string()
            )
        );
        
        let attributes = ObjectAttributes {
            color_info: dominant_color_rgb.map(|(r, g, b)| ColorInfo::from_rgb(r, g, b)),
            position: Position {
                x_center: (bbox.xmin + bbox.xmax) / 2.0,
                y_center: (bbox.ymin + bbox.ymax) / 2.0,
                area,
            },
            size: Size {
                width,
                height,
                relative_size: (area / frame_area) * 100.0,
            },
            person_attrs: None,
            custom_metadata: HashMap::new(),
        };
        
        Self {
            frame_number,
            timestamp_ms,
            object_id,
            tracker_id: bbox.tracker_id,
            class_name,
            confidence: bbox.detector_confidence,
            bbox: BBoxCoords {
                xmin: bbox.xmin,
                ymin: bbox.ymin,
                xmax: bbox.xmax,
                ymax: bbox.ymax,
            },
            attributes,
        }
    }

    /// Create a detection log with pre-computed attributes
    pub fn from_bbox_with_attributes(
        frame_number: u64,
        timestamp_ms: u64,
        bbox: &Bbox,
        class_idx: usize,
        _frame_width: f32,
        _frame_height: f32,
        attributes: ObjectAttributes,
    ) -> Self {
        let class_name = coco_classes::NAMES
            .get(class_idx)
            .unwrap_or(&"unknown")
            .to_string();
        
        let object_id = format!(
            "{}_{}",
            class_name,
            bbox.tracker_id.map_or_else(
                || format!("untracked_{}", frame_number),
                |id| id.to_string()
            )
        );
        
        Self {
            frame_number,
            timestamp_ms,
            object_id,
            tracker_id: bbox.tracker_id,
            class_name,
            confidence: bbox.detector_confidence,
            bbox: BBoxCoords {
                xmin: bbox.xmin,
                ymin: bbox.ymin,
                xmax: bbox.xmax,
                ymax: bbox.ymax,
            },
            attributes,
        }
    }
}

/// Logger for managing detection logs
pub struct DetectionLogger {
    logs: Vec<DetectionLog>,
    person_counter: HashMap<i64, usize>, // Maps tracker_id to person number
    next_person_number: usize,
}

impl DetectionLogger {
    pub fn new() -> Self {
        Self {
            logs: Vec::new(),
            person_counter: HashMap::new(),
            next_person_number: 1,
        }
    }
    
    /// Add a detection log entry
    pub fn log_detection(&mut self, detection: DetectionLog) {
        self.logs.push(detection);
    }
    
    /// Get or assign a person number for a tracker ID
    pub fn get_person_number(&mut self, tracker_id: i64) -> usize {
        if let Some(&number) = self.person_counter.get(&tracker_id) {
            number
        } else {
            let number = self.next_person_number;
            self.person_counter.insert(tracker_id, number);
            self.next_person_number += 1;
            number
        }
    }
    
    /// Get all logs
    pub fn get_logs(&self) -> &[DetectionLog] {
        &self.logs
    }
    
    /// Print formatted detection log
    pub fn print_detection(&mut self, detection: &DetectionLog) {
        if detection.class_name == "person" {
            if let Some(tracker_id) = detection.tracker_id {
                let person_num = self.get_person_number(tracker_id);
                print!("Person{}: ", person_num);
            } else {
                print!("Person (untracked): ");
            }
        } else {
            print!("{}: ", detection.class_name);
        }
        
        // Build attribute string
        let mut attrs = vec![
            format!("confidence={:.2}", detection.confidence),
            format!("pos=({:.0},{:.0})", 
                detection.attributes.position.x_center,
                detection.attributes.position.y_center),
            format!("size=({:.0}x{:.0}, {:.1}% of frame)",
                detection.attributes.size.width,
                detection.attributes.size.height,
                detection.attributes.size.relative_size),
        ];
        
        // Add person-specific attributes (skip gender/age for now)
        if let Some(ref person_attrs) = detection.attributes.person_attrs {
            // TODO: Uncomment when proper age/gender models are loaded
            // if let Some(ref gender) = person_attrs.gender {
            //     if let Some(conf) = person_attrs.gender_confidence {
            //         if conf > 0.1 {  // Only show if we have real predictions
            //             attrs.push(format!("gender={}({:.2})", gender, conf));
            //         }
            //     }
            // }
            // if let Some(ref age) = person_attrs.age_group {
            //     if let Some(conf) = person_attrs.age_confidence {
            //         if conf > 0.1 {  // Only show if we have real predictions
            //             attrs.push(format!("age={}({:.2})", age, conf));
            //         }
            //     }
            // }
            if let Some(ref upper) = person_attrs.upper_body_color {
                attrs.push(format!("upper={}", upper));
            }
            if let Some(ref lower) = person_attrs.lower_body_color {
                attrs.push(format!("lower={}", lower));
            }
        } else if let Some(ref color) = detection.attributes.color_info {
            // For non-person objects, show overall color
            attrs.push(format!("color={}", color.color_name));
        }
        
        if let Some(tid) = detection.tracker_id {
            attrs.push(format!("tracker_id={}", tid));
        }
        
        println!("{}", attrs.join(", "));
    }
    
    /// Print frame summary
    pub fn print_frame_summary(&mut self, frame_number: u64, detections: &[DetectionLog]) {
        if detections.is_empty() {
            return;
        }
        
        println!("\n--- Frame {} ---", frame_number);
        
        // Group by class
        let mut by_class: HashMap<String, Vec<&DetectionLog>> = HashMap::new();
        for det in detections {
            by_class.entry(det.class_name.clone())
                .or_insert_with(Vec::new)
                .push(det);
        }
        
        // Print grouped detections
        for (class_name, dets) in by_class.iter() {
            if class_name == "person" {
                println!("  People: {}", dets.len());
                for det in dets {
                    print!("    ");
                    self.print_detection(det);
                }
            } else {
                println!("  {}: {}", class_name, dets.len());
                for det in dets {
                    print!("    ");
                    self.print_detection(det);
                }
            }
        }
        println!("----------------\n");
    }
    
    /// Export logs to JSON
    pub fn export_json(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &self.logs)?;
        Ok(())
    }
    
    /// Clear logs
    pub fn clear(&mut self) {
        self.logs.clear();
    }
}

impl Default for DetectionLogger {
    fn default() -> Self {
        Self::new()
    }
}
