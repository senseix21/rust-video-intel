use anyhow::{Context, Result};
use inference_common::detection_logger::DetectionLog;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

const ZONES_FILE: &str = "zones.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoiZone {
    pub id: String,
    pub name: String,
    pub bbox: RoiBBox,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoiBBox {
    pub xmin: f32, // Normalized 0.0-1.0
    pub ymin: f32,
    pub xmax: f32,
    pub ymax: f32,
}

impl RoiZone {
    pub fn new(name: String) -> Self {
        let id = format!("zone_{}", Uuid::new_v4().to_string()[..8].to_string());
        Self {
            id,
            name,
            bbox: RoiBBox {
                xmin: 0.25,
                ymin: 0.25,
                xmax: 0.75,
                ymax: 0.75,
            },
            enabled: true,
        }
    }

    pub fn new_with_bbox(name: String, bbox: RoiBBox) -> Self {
        let id = format!("zone_{}", Uuid::new_v4().to_string()[..8].to_string());
        Self {
            id,
            name,
            bbox,
            enabled: true,
        }
    }

    /// Check if a detection is inside this zone using center-point method
    pub fn contains_detection(&self, det: &DetectionLog, frame_w: u32, frame_h: u32) -> bool {
        if !self.enabled {
            return false;
        }

        // Calculate detection center in normalized coordinates
        let det_center_x = ((det.bbox.xmin + det.bbox.xmax) / 2.0) / frame_w as f32;
        let det_center_y = ((det.bbox.ymin + det.bbox.ymax) / 2.0) / frame_h as f32;

        // Check if center is inside zone bbox
        det_center_x >= self.bbox.xmin
            && det_center_x <= self.bbox.xmax
            && det_center_y >= self.bbox.ymin
            && det_center_y <= self.bbox.ymax
    }

    /// Validate and clamp bbox coordinates
    pub fn validate_and_clamp(&mut self) {
        self.bbox.xmin = self.bbox.xmin.clamp(0.0, 1.0);
        self.bbox.ymin = self.bbox.ymin.clamp(0.0, 1.0);
        self.bbox.xmax = self.bbox.xmax.clamp(0.0, 1.0);
        self.bbox.ymax = self.bbox.ymax.clamp(0.0, 1.0);

        // Ensure min < max
        if self.bbox.xmin > self.bbox.xmax {
            std::mem::swap(&mut self.bbox.xmin, &mut self.bbox.xmax);
        }
        if self.bbox.ymin > self.bbox.ymax {
            std::mem::swap(&mut self.bbox.ymin, &mut self.bbox.ymax);
        }
    }
}

impl RoiBBox {
    pub fn new(xmin: f32, ymin: f32, xmax: f32, ymax: f32) -> Self {
        Self {
            xmin: xmin.clamp(0.0, 1.0),
            ymin: ymin.clamp(0.0, 1.0),
            xmax: xmax.clamp(0.0, 1.0),
            ymax: ymax.clamp(0.0, 1.0),
        }
    }

    pub fn area(&self) -> f32 {
        (self.xmax - self.xmin) * (self.ymax - self.ymin)
    }
}

/// Save zones to JSON file
pub fn save_zones(zones: &[RoiZone]) -> Result<()> {
    let json = serde_json::to_string_pretty(zones)
        .context("Failed to serialize zones to JSON")?;
    fs::write(ZONES_FILE, json).context("Failed to write zones.json")?;
    Ok(())
}

/// Load zones from JSON file
pub fn load_zones() -> Result<Vec<RoiZone>> {
    if !Path::new(ZONES_FILE).exists() {
        return Ok(Vec::new());
    }

    let json = fs::read_to_string(ZONES_FILE).context("Failed to read zones.json")?;
    let zones: Vec<RoiZone> =
        serde_json::from_str(&json).context("Failed to parse zones.json")?;
    Ok(zones)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_detection(center_x: f32, center_y: f32, _frame_w: u32, _frame_h: u32) -> DetectionLog {
        let half_width = 10.0;
        let half_height = 10.0;
        
        DetectionLog {
            frame_number: 0,
            timestamp_ms: 0,
            object_id: "test_0".to_string(),
            tracker_id: Some(0),
            class_name: "person".to_string(),
            confidence: 0.9,
            bbox: inference_common::detection_logger::BBoxCoords {
                xmin: center_x - half_width,
                ymin: center_y - half_height,
                xmax: center_x + half_width,
                ymax: center_y + half_height,
            },
            attributes: inference_common::detection_logger::ObjectAttributes::default(),
        }
    }

    #[test]
    fn test_zone_creation() {
        let zone = RoiZone::new("Test Zone".to_string());
        assert_eq!(zone.name, "Test Zone");
        assert!(zone.enabled);
        assert!(zone.id.starts_with("zone_"));
    }

    #[test]
    fn test_bbox_area() {
        let bbox = RoiBBox::new(0.0, 0.0, 0.5, 0.5);
        assert_eq!(bbox.area(), 0.25);
        
        let bbox2 = RoiBBox::new(0.25, 0.25, 0.75, 0.75);
        assert_eq!(bbox2.area(), 0.25);
    }

    #[test]
    fn test_contains_detection_center_point() {
        let zone = RoiZone::new_with_bbox(
            "Test".to_string(),
            RoiBBox::new(0.25, 0.25, 0.75, 0.75),
        );

        let frame_w = 1000;
        let frame_h = 1000;

        // Detection at center of frame (should be inside zone)
        let det_inside = create_test_detection(500.0, 500.0, frame_w, frame_h);
        assert!(zone.contains_detection(&det_inside, frame_w, frame_h));

        // Detection at top-left corner (should be outside zone)
        let det_outside = create_test_detection(100.0, 100.0, frame_w, frame_h);
        assert!(!zone.contains_detection(&det_outside, frame_w, frame_h));

        // Detection at bottom-right corner (should be outside zone)
        let det_outside2 = create_test_detection(900.0, 900.0, frame_w, frame_h);
        assert!(!zone.contains_detection(&det_outside2, frame_w, frame_h));
    }

    #[test]
    fn test_zone_boundary_cases() {
        let zone = RoiZone::new_with_bbox(
            "Boundary Test".to_string(),
            RoiBBox::new(0.0, 0.0, 0.5, 0.5),
        );

        let frame_w = 100;
        let frame_h = 100;

        // Detection exactly at zone edge (25, 25) - should be inside
        let det_edge = create_test_detection(25.0, 25.0, frame_w, frame_h);
        assert!(zone.contains_detection(&det_edge, frame_w, frame_h));

        // Detection just inside zone (24, 24)
        let det_just_inside = create_test_detection(24.0, 24.0, frame_w, frame_h);
        assert!(zone.contains_detection(&det_just_inside, frame_w, frame_h));

        // Detection just outside zone (51, 51)
        let det_just_outside = create_test_detection(51.0, 51.0, frame_w, frame_h);
        assert!(!zone.contains_detection(&det_just_outside, frame_w, frame_h));
    }

    #[test]
    fn test_disabled_zone() {
        let mut zone = RoiZone::new_with_bbox(
            "Disabled".to_string(),
            RoiBBox::new(0.0, 0.0, 1.0, 1.0),
        );
        zone.enabled = false;

        let frame_w = 100;
        let frame_h = 100;

        // Even though detection is inside bbox, disabled zone returns false
        let det = create_test_detection(50.0, 50.0, frame_w, frame_h);
        assert!(!zone.contains_detection(&det, frame_w, frame_h));
    }

    #[test]
    fn test_validate_and_clamp() {
        let mut zone = RoiZone::new_with_bbox(
            "Invalid".to_string(),
            RoiBBox {
                xmin: -0.5,
                ymin: 1.5,
                xmax: 2.0,
                ymax: 0.8,
            },
        );

        zone.validate_and_clamp();

        assert_eq!(zone.bbox.xmin, 0.0);
        assert_eq!(zone.bbox.xmax, 1.0);
        assert_eq!(zone.bbox.ymin, 0.8); // Swapped
        assert_eq!(zone.bbox.ymax, 1.0); // Clamped and swapped
    }

    #[test]
    fn test_save_load_roundtrip() {
        let test_file = "test_zones.json";
        
        // Create test zones
        let zones = vec![
            RoiZone::new_with_bbox(
                "Zone 1".to_string(),
                RoiBBox::new(0.1, 0.1, 0.4, 0.4),
            ),
            RoiZone::new_with_bbox(
                "Zone 2".to_string(),
                RoiBBox::new(0.6, 0.6, 0.9, 0.9),
            ),
        ];

        // Save
        let json = serde_json::to_string_pretty(&zones).unwrap();
        fs::write(test_file, json).unwrap();

        // Load
        let loaded_json = fs::read_to_string(test_file).unwrap();
        let loaded_zones: Vec<RoiZone> = serde_json::from_str(&loaded_json).unwrap();

        // Verify
        assert_eq!(loaded_zones.len(), 2);
        assert_eq!(loaded_zones[0].name, "Zone 1");
        assert_eq!(loaded_zones[1].name, "Zone 2");
        assert_eq!(loaded_zones[0].bbox.xmin, 0.1);

        // Cleanup
        fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_load_nonexistent_file() {
        // Should return empty vec, not error
        let result = load_zones();
        assert!(result.is_ok());
    }

    #[test]
    fn test_bbox_clamping() {
        let bbox = RoiBBox::new(-1.0, -1.0, 2.0, 2.0);
        assert_eq!(bbox.xmin, 0.0);
        assert_eq!(bbox.ymin, 0.0);
        assert_eq!(bbox.xmax, 1.0);
        assert_eq!(bbox.ymax, 1.0);
    }
}
