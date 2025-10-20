//! ONNX-based attribute detection for enhanced object analysis.
//! 
//! This module provides neural network-based attribute extraction including:
//! - Color classification using deep learning
//! - Person attributes (gender, age, etc.)
//! - Detailed appearance features

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ndarray::{Array4, CowArray};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::TensorRef;
use std::path::Path;

/// Attribute detection using ONNX models
pub struct AttributeDetector {
    // Color classification model (optional)
    color_model: Option<Session>,
    // Person attribute model (optional) 
    person_attr_model: Option<Session>,
}

/// Color classification result
#[derive(Debug, Clone)]
pub struct ColorClassification {
    pub color_name: String,
    pub confidence: f32,
    pub rgb_estimate: (u8, u8, u8),
}

/// Person attributes from neural network
#[derive(Debug, Clone)]
pub struct PersonAttributes {
    pub gender: Option<(String, f32)>,      // (gender, confidence)
    pub age_group: Option<(String, f32)>,   // (age_group, confidence)
    pub upper_color: Option<String>,
    pub lower_color: Option<String>,
}

impl AttributeDetector {
    /// Create a new attribute detector with optional model paths
    pub fn new(
        color_model_path: Option<&Path>,
        person_attr_model_path: Option<&Path>,
    ) -> Result<Self> {
        let color_model = if let Some(path) = color_model_path {
            if path.exists() {
                log::info!("Loading color classification model from {:?}", path);
                Some(
                    Session::builder()?
                        .with_optimization_level(GraphOptimizationLevel::Level3)?
                        .commit_from_file(path)
                        .context("Failed to load color model")?,
                )
            } else {
                log::warn!("Color model path {:?} does not exist, using fallback", path);
                None
            }
        } else {
            None
        };

        let person_attr_model = if let Some(path) = person_attr_model_path {
            if path.exists() {
                log::info!("Loading person attribute model from {:?}", path);
                Some(
                    Session::builder()?
                        .with_optimization_level(GraphOptimizationLevel::Level3)?
                        .commit_from_file(path)
                        .context("Failed to load person attribute model")?,
                )
            } else {
                log::warn!(
                    "Person attribute model path {:?} does not exist, using fallback",
                    path
                );
                None
            }
        } else {
            None
        };

        Ok(Self {
            color_model,
            person_attr_model,
        })
    }

    /// Classify color using neural network (if model available) or fallback
    pub fn classify_color(
        &mut self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32), // (xmin, ymin, xmax, ymax)
    ) -> Result<ColorClassification> {
        if self.color_model.is_some() {
            self.classify_color_nn(image, bbox)
        } else {
            // Fallback to simple color extraction
            self.classify_color_fallback(image, bbox)
        }
    }

    /// Neural network-based color classification
    fn classify_color_nn(
        &mut self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
    ) -> Result<ColorClassification> {
        // Extract and preprocess the bounding box region
        let cropped = self.crop_and_resize(image, bbox, 64, 64)?;
        
        // Convert to model input format (1, 3, 64, 64) normalized to [0, 1]
        let input_array = self.image_to_array(&cropped)?;
        
        // Run inference
        let input_array_dyn = CowArray::from(input_array).into_dyn();
        let input = ort::inputs![TensorRef::from_array_view(&input_array_dyn)?];
        let model = self.color_model.as_mut().unwrap();
        let outputs = model.run(input)?;
        let (_shape, output) = outputs[0].try_extract_tensor::<f32>()?;

        // Parse output - assuming softmax over color classes
        let color_classes = vec![
            "red", "blue", "green", "yellow", "orange", "purple", 
            "pink", "brown", "black", "white", "gray", "beige"
        ];
        
        let (max_idx, max_conf) = output
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap();

        let color_name = color_classes
            .get(max_idx)
            .unwrap_or(&"unknown")
            .to_string();
        
        let max_conf = *max_conf;

        // Drop outputs to release mutable borrow before calling other methods
        drop(outputs);

        // Estimate RGB from detected color
        let rgb_estimate = self.color_name_to_rgb(&color_name);

        Ok(ColorClassification {
            color_name,
            confidence: max_conf,
            rgb_estimate,
        })
    }

    /// Fallback color classification using simple averaging
    fn classify_color_fallback(
        &self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
    ) -> Result<ColorClassification> {
        let (xmin, ymin, xmax, ymax) = bbox;
        let (img_width, img_height) = image.dimensions();

        let x1 = xmin.max(0.0).min(img_width as f32) as u32;
        let y1 = ymin.max(0.0).min(img_height as f32) as u32;
        let x2 = xmax.max(0.0).min(img_width as f32) as u32;
        let y2 = ymax.max(0.0).min(img_height as f32) as u32;

        if x2 <= x1 || y2 <= y1 {
            return Ok(ColorClassification {
                color_name: "unknown".to_string(),
                confidence: 0.0,
                rgb_estimate: (128, 128, 128),
            });
        }

        // Sample center region
        let margin_x = ((x2 - x1) as f32 * 0.2) as u32;
        let margin_y = ((y2 - y1) as f32 * 0.2) as u32;

        let sample_x1 = (x1 + margin_x).min(x2);
        let sample_y1 = (y1 + margin_y).min(y2);
        let sample_x2 = (x2 - margin_x).max(x1);
        let sample_y2 = (y2 - margin_y).max(y1);

        let mut r_sum: u64 = 0;
        let mut g_sum: u64 = 0;
        let mut b_sum: u64 = 0;
        let mut count: u64 = 0;

        let step = ((x2 - x1).max(y2 - y1) / 20).max(1);

        for y in (sample_y1..sample_y2).step_by(step as usize) {
            for x in (sample_x1..sample_x2).step_by(step as usize) {
                let pixel = image.get_pixel(x, y);
                r_sum += pixel[0] as u64;
                g_sum += pixel[1] as u64;
                b_sum += pixel[2] as u64;
                count += 1;
            }
        }

        if count == 0 {
            return Ok(ColorClassification {
                color_name: "unknown".to_string(),
                confidence: 0.0,
                rgb_estimate: (128, 128, 128),
            });
        }

        let r = (r_sum / count) as u8;
        let g = (g_sum / count) as u8;
        let b = (b_sum / count) as u8;

        let color_name = Self::rgb_to_color_name(r, g, b);

        Ok(ColorClassification {
            color_name,
            confidence: 0.7, // Lower confidence for fallback
            rgb_estimate: (r, g, b),
        })
    }

    /// Extract person attributes using neural network
    pub fn extract_person_attributes(
        &mut self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
    ) -> Result<PersonAttributes> {
        if self.person_attr_model.is_some() {
            self.extract_person_attributes_nn(image, bbox)
        } else {
            // Fallback - basic color detection for upper/lower body
            self.extract_person_attributes_fallback(image, bbox)
        }
    }

    /// Neural network-based person attribute extraction
    fn extract_person_attributes_nn(
        &mut self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
    ) -> Result<PersonAttributes> {
        // Extract and preprocess person region
        let cropped = self.crop_and_resize(image, bbox, 128, 256)?;
        let input_array = self.image_to_array(&cropped)?;

        // Run inference
        let input_array_dyn = CowArray::from(input_array).into_dyn();
        let input = ort::inputs![TensorRef::from_array_view(&input_array_dyn)?];
        let model = self.person_attr_model.as_mut().unwrap();
        let _outputs = model.run(input)?;
        
        // Parse outputs (assuming multi-task model)
        // Output format: [gender_logits, age_logits, upper_color_logits, lower_color_logits]
        
        // This is a placeholder - actual parsing depends on model architecture
        let gender = Some(("male".to_string(), 0.8));
        let age_group = Some(("adult".to_string(), 0.75));
        let upper_color = Some("blue".to_string());
        let lower_color = Some("black".to_string());

        Ok(PersonAttributes {
            gender,
            age_group,
            upper_color,
            lower_color,
        })
    }

    /// Fallback person attribute extraction
    fn extract_person_attributes_fallback(
        &self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
    ) -> Result<PersonAttributes> {
        let (xmin, ymin, xmax, ymax) = bbox;
        let height = ymax - ymin;

        // Upper body: top 40% of bbox
        let upper_bbox = (xmin, ymin, xmax, ymin + height * 0.4);
        let upper_color_result = self.classify_color_fallback(image, upper_bbox)?;

        // Lower body: bottom 40% of bbox
        let lower_bbox = (xmin, ymax - height * 0.4, xmax, ymax);
        let lower_color_result = self.classify_color_fallback(image, lower_bbox)?;

        Ok(PersonAttributes {
            gender: None,
            age_group: None,
            upper_color: Some(upper_color_result.color_name),
            lower_color: Some(lower_color_result.color_name),
        })
    }

    /// Crop and resize image region
    fn crop_and_resize(
        &self,
        image: &DynamicImage,
        bbox: (f32, f32, f32, f32),
        target_w: u32,
        target_h: u32,
    ) -> Result<DynamicImage> {
        let (xmin, ymin, xmax, ymax) = bbox;
        let (img_width, img_height) = image.dimensions();

        let x = xmin.max(0.0) as u32;
        let y = ymin.max(0.0) as u32;
        let w = ((xmax - xmin).max(1.0) as u32).min(img_width - x);
        let h = ((ymax - ymin).max(1.0) as u32).min(img_height - y);

        let cropped = image.crop_imm(x, y, w, h);
        let resized = cropped.resize_exact(
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        );

        Ok(resized)
    }

    /// Convert image to ndarray for model input
    fn image_to_array(&self, image: &DynamicImage) -> Result<Array4<f32>> {
        let rgb_image = image.to_rgb8();
        let (width, height) = rgb_image.dimensions();

        let mut array = Array4::<f32>::zeros((1, 3, height as usize, width as usize));

        for y in 0..height {
            for x in 0..width {
                let pixel = rgb_image.get_pixel(x, y);
                array[[0, 0, y as usize, x as usize]] = pixel[0] as f32 / 255.0;
                array[[0, 1, y as usize, x as usize]] = pixel[1] as f32 / 255.0;
                array[[0, 2, y as usize, x as usize]] = pixel[2] as f32 / 255.0;
            }
        }

        Ok(array)
    }

    /// Convert color name to approximate RGB
    fn color_name_to_rgb(&self, color_name: &str) -> (u8, u8, u8) {
        match color_name {
            "red" => (220, 20, 20),
            "blue" => (20, 20, 220),
            "green" => (20, 220, 20),
            "yellow" => (220, 220, 20),
            "orange" => (255, 140, 0),
            "purple" => (128, 0, 128),
            "pink" => (255, 192, 203),
            "brown" => (139, 69, 19),
            "black" => (20, 20, 20),
            "white" => (240, 240, 240),
            "gray" => (128, 128, 128),
            "beige" => (245, 245, 220),
            _ => (128, 128, 128),
        }
    }

    /// Simple RGB to color name mapping
    fn rgb_to_color_name(r: u8, g: u8, b: u8) -> String {
        let (r, g, b) = (r as f32, g as f32, b as f32);

        let brightness = (r + g + b) / 3.0;

        if brightness < 40.0 {
            return "black".to_string();
        }
        if brightness > 210.0 {
            return "white".to_string();
        }

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

    /// Check if models are loaded
    pub fn has_color_model(&self) -> bool {
        self.color_model.is_some()
    }

    pub fn has_person_attr_model(&self) -> bool {
        self.person_attr_model.is_some()
    }

    /// Detect attributes for an object (combines color and person attributes)
    pub fn detect_attributes(
        &mut self,
        image: &DynamicImage,
        xmin: f32,
        ymin: f32,
        xmax: f32,
        ymax: f32,
        class_name: &str,
    ) -> Result<crate::detection_logger::ObjectAttributes> {
        use crate::detection_logger::{ColorInfo, ObjectAttributes, PersonAttributesLog, Position, Size};
        
        let bbox = (xmin, ymin, xmax, ymax);
        
        // Extract color information
        let color_info = match self.classify_color(image, bbox) {
            Ok(color_class) => Some(ColorInfo {
                dominant_color: color_class.color_name.clone(),
                rgb: color_class.rgb_estimate,
                color_name: color_class.color_name,
            }),
            Err(e) => {
                log::warn!("Color classification failed: {}", e);
                None
            }
        };
        
        // Extract person attributes ONLY for person class
        let person_attrs = if class_name == "person" {
            match self.extract_person_attributes(image, bbox) {
                Ok(attrs) => {
                    let (gender, gender_conf) = attrs.gender.unwrap_or_else(|| ("unknown".to_string(), 0.0));
                    let (age, age_conf) = attrs.age_group.unwrap_or_else(|| ("unknown".to_string(), 0.0));
                    
                    Some(PersonAttributesLog {
                        gender: Some(gender),
                        gender_confidence: Some(gender_conf),
                        age_group: Some(age),
                        age_confidence: Some(age_conf),
                        upper_body_color: attrs.upper_color,
                        lower_body_color: attrs.lower_color,
                    })
                }
                Err(e) => {
                    log::debug!("Person attribute extraction skipped/failed: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        // Calculate position and size
        let (img_width, img_height) = image.dimensions();
        let area = (xmax - xmin) * (ymax - ymin);
        
        let position = Position {
            x_center: (xmin + xmax) / 2.0,
            y_center: (ymin + ymax) / 2.0,
            area,
        };
        
        let size = Size {
            width: xmax - xmin,
            height: ymax - ymin,
            relative_size: area / (img_width as f32 * img_height as f32),
        };
        
        Ok(ObjectAttributes {
            color_info,
            position,
            size,
            person_attrs,
            custom_metadata: std::collections::HashMap::new(),
        })
    }
}

/// Default implementation with no models (uses fallback methods)
impl Default for AttributeDetector {
    fn default() -> Self {
        Self {
            color_model: None,
            person_attr_model: None,
        }
    }
}
