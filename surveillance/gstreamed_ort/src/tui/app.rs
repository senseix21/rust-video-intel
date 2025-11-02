use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use inference_common::detection_logger::DetectionLog;
use inference_common::frame_times::FrameTimes;

use super::roi::{RoiZone, load_zones, save_zones};

const MAX_HISTORY: usize = 1000;
const PERF_HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone)]
pub enum TuiMessage {
    VideoInfo {
        filename: String,
        width: u32,
        height: u32,
        total_frames: Option<u64>,
    },
    FrameProcessed {
        frame_num: u64,
        timestamp_ms: u64,
        detections: Vec<DetectionLog>,
        performance: FrameTimes,
    },
    Error(String),
    Finished,
}

#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub inference_ms: f64,
    pub preprocess_ms: f64,
    pub postprocess_ms: f64,
    pub total_ms: f64,
}

impl From<&FrameTimes> for PerformanceStats {
    fn from(ft: &FrameTimes) -> Self {
        let preprocess_ms = (ft.frame_to_buffer.as_secs_f64() 
                           + ft.buffer_resize.as_secs_f64()
                           + ft.buffer_to_tensor.as_secs_f64()) * 1000.0;
        let inference_ms = ft.forward_pass.as_secs_f64() * 1000.0;
        let postprocess_ms = (ft.bbox_extraction.as_secs_f64()
                            + ft.nms.as_secs_f64()
                            + ft.tracking.as_secs_f64()
                            + ft.annotation.as_secs_f64()) * 1000.0;
        
        Self {
            inference_ms,
            preprocess_ms,
            postprocess_ms,
            total_ms: preprocess_ms + inference_ms + postprocess_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TuiMode {
    Monitor,
    ZoneList,
    ZoneEdit,
}

pub struct App {
    // Video info
    pub filename: String,
    pub width: u32,
    pub height: u32,
    pub total_frames: Option<u64>,
    
    // Current state
    pub frame_num: u64,
    pub timestamp_ms: u64,
    pub fps: f32,
    pub is_paused: bool,
    pub is_finished: bool,
    should_quit: bool,
    
    // Detection data
    pub current_detections: Vec<DetectionLog>,
    pub class_counts: HashMap<String, usize>,
    pub total_detections: usize,
    
    // Living beings tracking
    pub living_beings: HashMap<String, LivingBeingStats>,
    pub total_living_seen: usize,
    
    // Performance metrics
    pub current_perf: PerformanceStats,
    pub perf_history: VecDeque<PerformanceStats>,
    pub avg_fps: f32,
    
    // UI state
    pub selected_index: usize,
    pub scroll_offset: usize,
    
    // ROI zones
    pub tui_mode: TuiMode,
    pub zones: Vec<RoiZone>,
    pub selected_zone_idx: usize,
    pub zone_draft: Option<RoiZone>,
    
    // Timing
    last_frame_time: Instant,
    frame_count_for_fps: u32,
    fps_calc_start: Instant,
}

#[derive(Debug, Clone)]
pub struct LivingBeingStats {
    pub class_name: String,
    pub first_seen_frame: u64,
    pub last_seen_frame: u64,
    pub total_count: usize,
    pub unique_ids: std::collections::HashSet<i64>,
}

impl App {
    pub fn new() -> Self {
        let zones = load_zones().unwrap_or_else(|e| {
            eprintln!("Failed to load zones: {}", e);
            Vec::new()
        });

        Self {
            filename: String::from("Loading..."),
            width: 0,
            height: 0,
            total_frames: None,
            frame_num: 0,
            timestamp_ms: 0,
            fps: 0.0,
            is_paused: false,
            is_finished: false,
            should_quit: false,
            current_detections: Vec::new(),
            class_counts: HashMap::new(),
            total_detections: 0,
            living_beings: HashMap::new(),
            total_living_seen: 0,
            current_perf: PerformanceStats {
                inference_ms: 0.0,
                preprocess_ms: 0.0,
                postprocess_ms: 0.0,
                total_ms: 0.0,
            },
            perf_history: VecDeque::with_capacity(PERF_HISTORY_SIZE),
            avg_fps: 0.0,
            selected_index: 0,
            scroll_offset: 0,
            tui_mode: TuiMode::Monitor,
            zones,
            selected_zone_idx: 0,
            zone_draft: None,
            last_frame_time: Instant::now(),
            frame_count_for_fps: 0,
            fps_calc_start: Instant::now(),
        }
    }
    
    fn is_living_being(class_name: &str) -> bool {
        matches!(class_name, 
            "person" | "cat" | "dog" | "horse" | "sheep" | "cow" | 
            "elephant" | "bear" | "zebra" | "giraffe" | "bird"
        )
    }
    
    pub fn update(&mut self, msg: TuiMessage) {
        match msg {
            TuiMessage::VideoInfo { filename, width, height, total_frames } => {
                self.filename = filename;
                self.width = width;
                self.height = height;
                self.total_frames = total_frames;
            }
            TuiMessage::FrameProcessed { frame_num, timestamp_ms, detections, performance } => {
                self.frame_num = frame_num;
                self.timestamp_ms = timestamp_ms;
                self.current_detections = detections.clone();
                
                // Update class counts
                self.class_counts.clear();
                for det in &detections {
                    *self.class_counts.entry(det.class_name.clone()).or_insert(0) += 1;
                }
                self.total_detections += detections.len();
                
                // Track living beings
                for det in &detections {
                    if Self::is_living_being(&det.class_name) {
                        let entry = self.living_beings
                            .entry(det.class_name.clone())
                            .or_insert_with(|| LivingBeingStats {
                                class_name: det.class_name.clone(),
                                first_seen_frame: frame_num,
                                last_seen_frame: frame_num,
                                total_count: 0,
                                unique_ids: std::collections::HashSet::new(),
                            });
                        
                        entry.last_seen_frame = frame_num;
                        entry.total_count += 1;
                        
                        if let Some(tracker_id) = det.tracker_id {
                            entry.unique_ids.insert(tracker_id);
                        }
                    }
                }
                
                // Update total living seen count
                self.total_living_seen = self.living_beings.values()
                    .map(|stats| stats.unique_ids.len().max(1))
                    .sum();
                
                // Update performance stats
                let perf = PerformanceStats::from(&performance);
                self.current_perf = perf.clone();
                self.perf_history.push_back(perf);
                if self.perf_history.len() > PERF_HISTORY_SIZE {
                    self.perf_history.pop_front();
                }
                
                // Calculate FPS
                self.frame_count_for_fps += 1;
                let elapsed = self.fps_calc_start.elapsed().as_secs_f32();
                if elapsed >= 1.0 {
                    self.fps = self.frame_count_for_fps as f32 / elapsed;
                    self.avg_fps = self.fps;
                    self.frame_count_for_fps = 0;
                    self.fps_calc_start = Instant::now();
                }
                
                self.last_frame_time = Instant::now();
            }
            TuiMessage::Error(err) => {
                log::error!("TUI received error: {}", err);
            }
            TuiMessage::Finished => {
                self.is_finished = true;
            }
        }
    }
    
    pub fn quit(&mut self) {
        self.should_quit = true;
    }
    
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
    
    pub fn toggle_pause(&mut self) {
        self.is_paused = !self.is_paused;
    }
    
    pub fn scroll_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }
    
    pub fn scroll_down(&mut self) {
        if self.selected_index < self.current_detections.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }
    
    pub fn page_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(10);
    }
    
    pub fn page_down(&mut self) {
        let max = self.current_detections.len().saturating_sub(1);
        self.selected_index = (self.selected_index + 10).min(max);
    }
    
    pub fn scroll_home(&mut self) {
        self.selected_index = 0;
    }
    
    pub fn scroll_end(&mut self) {
        self.selected_index = self.current_detections.len().saturating_sub(1);
    }
    
    pub fn select_current(&mut self) {
        // For future use - could open detail view
    }
    
    pub fn mark_finished(&mut self) {
        self.is_finished = true;
    }
    
    pub fn get_selected_detection(&self) -> Option<&DetectionLog> {
        self.current_detections.get(self.selected_index)
    }
    
    pub fn progress_percentage(&self) -> f32 {
        if let Some(total) = self.total_frames {
            if total > 0 {
                return (self.frame_num as f32 / total as f32) * 100.0;
            }
        }
        0.0
    }
    
    /// Check if a bounding box overlaps with a target region
    pub fn bbox_overlaps(bbox: &DetectionLog, target_xmin: f32, target_ymin: f32, target_xmax: f32, target_ymax: f32) -> bool {
        !(bbox.bbox.xmax < target_xmin 
          || bbox.bbox.xmin > target_xmax
          || bbox.bbox.ymax < target_ymin
          || bbox.bbox.ymin > target_ymax)
    }
    
    /// Check if a bounding box is completely contained within a target region
    pub fn bbox_contained_in(bbox: &DetectionLog, target_xmin: f32, target_ymin: f32, target_xmax: f32, target_ymax: f32) -> bool {
        bbox.bbox.xmin >= target_xmin
            && bbox.bbox.ymin >= target_ymin
            && bbox.bbox.xmax <= target_xmax
            && bbox.bbox.ymax <= target_ymax
    }
    
    /// Check if a bounding box center point is within a target region
    pub fn bbox_center_in(bbox: &DetectionLog, target_xmin: f32, target_ymin: f32, target_xmax: f32, target_ymax: f32) -> bool {
        let center_x = bbox.attributes.position.x_center;
        let center_y = bbox.attributes.position.y_center;
        center_x >= target_xmin && center_x <= target_xmax
            && center_y >= target_ymin && center_y <= target_ymax
    }
    
    /// Filter detections that overlap with a target BBox
    pub fn filter_detections_overlapping(
        detections: &[DetectionLog],
        target_xmin: f32,
        target_ymin: f32,
        target_xmax: f32,
        target_ymax: f32,
    ) -> Vec<&DetectionLog> {
        detections
            .iter()
            .filter(|det| Self::bbox_overlaps(det, target_xmin, target_ymin, target_xmax, target_ymax))
            .collect()
    }
    
    /// Filter detections completely contained within a target BBox
    pub fn filter_detections_contained(
        detections: &[DetectionLog],
        target_xmin: f32,
        target_ymin: f32,
        target_xmax: f32,
        target_ymax: f32,
    ) -> Vec<&DetectionLog> {
        detections
            .iter()
            .filter(|det| Self::bbox_contained_in(det, target_xmin, target_ymin, target_xmax, target_ymax))
            .collect()
    }
    
    /// Filter detections with center point in a target BBox
    pub fn filter_detections_center_in(
        detections: &[DetectionLog],
        target_xmin: f32,
        target_ymin: f32,
        target_xmax: f32,
        target_ymax: f32,
    ) -> Vec<&DetectionLog> {
        detections
            .iter()
            .filter(|det| Self::bbox_center_in(det, target_xmin, target_ymin, target_xmax, target_ymax))
            .collect()
    }
    
    /// Find first detection overlapping with target BBox
    pub fn find_detection_at(
        detections: &[DetectionLog],
        target_xmin: f32,
        target_ymin: f32,
        target_xmax: f32,
        target_ymax: f32,
    ) -> Option<&DetectionLog> {
        detections
            .iter()
            .find(|det| Self::bbox_overlaps(det, target_xmin, target_ymin, target_xmax, target_ymax))
    }

    // ===== ROI Zone Methods =====
    
    /// Get all detections inside a specific zone
    pub fn get_zone_detections(&self, zone: &RoiZone) -> Vec<&DetectionLog> {
        self.current_detections
            .iter()
            .filter(|det| zone.contains_detection(det, self.width, self.height))
            .collect()
    }
    
    /// Create a new zone
    pub fn create_zone(&mut self, name: String) -> usize {
        let zone = RoiZone::new(name);
        self.zones.push(zone);
        self.zones.len() - 1
    }
    
    /// Delete zone at index
    pub fn delete_zone(&mut self, idx: usize) {
        if idx < self.zones.len() {
            self.zones.remove(idx);
            if self.selected_zone_idx >= self.zones.len() && self.selected_zone_idx > 0 {
                self.selected_zone_idx = self.zones.len() - 1;
            }
            let _ = save_zones(&self.zones);
        }
    }
    
    /// Toggle zone enabled/disabled
    pub fn toggle_zone(&mut self, idx: usize) {
        if let Some(zone) = self.zones.get_mut(idx) {
            zone.enabled = !zone.enabled;
            let _ = save_zones(&self.zones);
        }
    }
    
    /// Save zones to disk
    pub fn save_zones(&self) -> anyhow::Result<()> {
        save_zones(&self.zones)
    }
    
    /// Count total detections in all enabled zones
    pub fn count_zone_detections(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for zone in &self.zones {
            if zone.enabled {
                let count = self.get_zone_detections(zone).len();
                counts.insert(zone.id.clone(), count);
            }
        }
        counts
    }
}
