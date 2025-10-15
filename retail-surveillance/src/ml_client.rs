use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const DEFAULT_ML_SERVICE_URL: &str = "http://localhost:8080";
const REQUEST_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub x: f32,      // Normalized [0, 1]
    pub y: f32,      // Normalized [0, 1]
    pub width: f32,  // Normalized [0, 1]
    pub height: f32, // Normalized [0, 1]
    pub confidence: f32,
    pub track_id: Option<u32>,
}

impl Detection {
    pub fn to_pixel_coords(&self, img_width: u32, img_height: u32) -> (i32, i32, i32, i32) {
        let x = (self.x * img_width as f32) as i32;
        let y = (self.y * img_height as f32) as i32;
        let w = (self.width * img_width as f32) as i32;
        let h = (self.height * img_height as f32) as i32;
        (x, y, w, h)
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DetectionResponse {
    detections: Vec<Detection>,
    count: usize,
    image_size: [usize; 2],
}

pub struct MLClient {
    client: reqwest::Client,
    service_url: String,
    enabled: bool,
}

impl MLClient {
    pub fn new(service_url: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let url = service_url.unwrap_or_else(|| DEFAULT_ML_SERVICE_URL.to_string());

        Self {
            client,
            service_url: url,
            enabled: true,
        }
    }

    pub async fn check_health(&self) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        let url = format!("{}/health", self.service_url);
        let response = self.client.get(&url).send().await?;

        Ok(response.status().is_success())
    }

    pub async fn detect_people(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<Detection>> {
        if !self.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/detect", self.service_url);

        let response = self
            .client
            .post(&url)
            .query(&[
                ("width", width.to_string()),
                ("height", height.to_string()),
                ("channels", "3".to_string()),
            ])
            .body(image_data.to_vec())
            .header("Content-Type", "application/octet-stream")
            .send()
            .await
            .context("Failed to send request to ML service")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("ML service returned error {}: {}", status, error_text);
        }

        let detection_response: DetectionResponse = response
            .json()
            .await
            .context("Failed to parse ML service response")?;

        debug!(
            "Detected {} people in {}x{} image",
            detection_response.count,
            detection_response.image_size[0],
            detection_response.image_size[1]
        );

        Ok(detection_response.detections)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            info!("ML inference enabled");
        } else {
            info!("ML inference disabled");
        }
    }
}

// ByteTrack implementation for people tracking
pub struct ByteTracker {
    tracks: Vec<Track>,
    next_id: u32,
    max_age: u32,
    min_hits: u32,
    iou_threshold: f32,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: u32,
    pub bbox: Detection,
    pub hits: u32,
    pub age: u32,
    pub state: TrackState,
    pub velocity: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackState {
    Tentative,
    Confirmed,
    Lost,
}

impl ByteTracker {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            next_id: 1,
            max_age: 30,
            min_hits: 3,
            iou_threshold: 0.3,
        }
    }

    pub fn update(&mut self, detections: Vec<Detection>) -> Vec<Detection> {
        // Age existing tracks
        for track in &mut self.tracks {
            track.age += 1;
        }

        // Get indices of confirmed and tentative tracks
        let confirmed_indices: Vec<usize> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TrackState::Confirmed)
            .map(|(i, _)| i)
            .collect();

        let tentative_indices: Vec<usize> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TrackState::Tentative)
            .map(|(i, _)| i)
            .collect();

        // Match detections to confirmed tracks
        let (matched_confirmed, unmatched_dets) =
            self.match_detections_by_indices(&detections, &confirmed_indices);

        // Match remaining detections to tentative tracks
        let (matched_tentative, unmatched_dets) =
            self.match_detections_by_indices(&unmatched_dets, &tentative_indices);

        // Update matched tracks
        for (det_idx, track_idx) in matched_confirmed.iter().chain(matched_tentative.iter()) {
            if *track_idx < self.tracks.len() {
                self.tracks[*track_idx].bbox = if *det_idx < detections.len() {
                    detections[*det_idx].clone()
                } else {
                    unmatched_dets[det_idx - detections.len()].clone()
                };
                self.tracks[*track_idx].age = 0;
            }
        }

        // Create new tracks for unmatched detections
        for det in unmatched_dets {
            let track = Track {
                id: self.next_id,
                bbox: det,
                hits: 1,
                age: 0,
                state: TrackState::Tentative,
                velocity: (0.0, 0.0),
            };
            self.tracks.push(track);
            self.next_id += 1;
        }

        // Update track states
        for track in &mut self.tracks {
            if track.age == 0 {
                track.hits += 1;

                // Update velocity
                let (cx, cy) = track.bbox.center();
                track.velocity = (
                    cx - (track.bbox.x + track.bbox.width / 2.0),
                    cy - (track.bbox.y + track.bbox.height / 2.0),
                );

                if track.state == TrackState::Tentative && track.hits >= self.min_hits {
                    track.state = TrackState::Confirmed;
                }
            } else if track.age > self.max_age {
                track.state = TrackState::Lost;
            }
        }

        // Remove lost tracks
        self.tracks.retain(|t| t.state != TrackState::Lost);

        // Return detections with track IDs
        self.tracks
            .iter()
            .filter(|t| t.state == TrackState::Confirmed && t.age == 0)
            .map(|t| {
                let mut det = t.bbox.clone();
                det.track_id = Some(t.id);
                det
            })
            .collect()
    }

    fn match_detections_by_indices(
        &self,
        detections: &[Detection],
        track_indices: &[usize],
    ) -> (Vec<(usize, usize)>, Vec<Detection>) {
        let mut matched = Vec::new();
        let mut unmatched_dets = Vec::new();

        if detections.is_empty() || track_indices.is_empty() {
            return (matched, detections.to_vec());
        }

        // Calculate IOU matrix
        let mut iou_matrix = vec![vec![0.0; track_indices.len()]; detections.len()];
        for (i, det) in detections.iter().enumerate() {
            for (j, &track_idx) in track_indices.iter().enumerate() {
                if track_idx < self.tracks.len() {
                    iou_matrix[i][j] = self.calculate_iou(det, &self.tracks[track_idx].bbox);
                }
            }
        }

        // Hungarian algorithm (simplified greedy matching)
        let mut matched_tracks = vec![false; track_indices.len()];
        let mut matched_dets = vec![false; detections.len()];

        // First pass: match high IOU pairs
        for i in 0..detections.len() {
            if matched_dets[i] {
                continue;
            }

            let mut best_j = None;
            let mut best_iou = self.iou_threshold;

            for j in 0..track_indices.len() {
                if matched_tracks[j] {
                    continue;
                }

                if iou_matrix[i][j] > best_iou {
                    best_iou = iou_matrix[i][j];
                    best_j = Some(j);
                }
            }

            if let Some(j) = best_j {
                matched.push((i, track_indices[j]));
                matched_dets[i] = true;
                matched_tracks[j] = true;
            }
        }

        // Collect unmatched detections
        for (i, det) in detections.iter().enumerate() {
            if !matched_dets[i] {
                unmatched_dets.push(det.clone());
            }
        }

        (matched, unmatched_dets)
    }


    fn calculate_iou(&self, det1: &Detection, det2: &Detection) -> f32 {
        let x1 = det1.x.max(det2.x);
        let y1 = det1.y.max(det2.y);
        let x2 = (det1.x + det1.width).min(det2.x + det2.width);
        let y2 = (det1.y + det1.height).min(det2.y + det2.height);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let area1 = det1.width * det1.height;
        let area2 = det2.width * det2.height;
        let union = area1 + area2 - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }

    pub fn get_active_tracks(&self) -> Vec<&Track> {
        self.tracks
            .iter()
            .filter(|t| t.state == TrackState::Confirmed)
            .collect()
    }

    pub fn get_track_count(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.state == TrackState::Confirmed)
            .count()
    }
}

// Zone counting functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub polygon: Vec<(f32, f32)>, // Normalized coordinates
    pub entry_count: u32,
    pub exit_count: u32,
    pub current_count: i32,
}

impl Zone {
    pub fn new(id: String, name: String, polygon: Vec<(f32, f32)>) -> Self {
        Self {
            id,
            name,
            polygon,
            entry_count: 0,
            exit_count: 0,
            current_count: 0,
        }
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        // Point-in-polygon test using ray casting
        let mut inside = false;
        let n = self.polygon.len();

        let mut p1 = self.polygon[n - 1];
        for p2 in &self.polygon {
            if (p2.1 > y) != (p1.1 > y) {
                let slope = (p2.0 - p1.0) / (p2.1 - p1.1);
                if x < slope * (y - p1.1) + p1.0 {
                    inside = !inside;
                }
            }
            p1 = *p2;
        }

        inside
    }

    pub fn update_count(&mut self, prev_inside: bool, curr_inside: bool) {
        if !prev_inside && curr_inside {
            self.entry_count += 1;
            self.current_count += 1;
        } else if prev_inside && !curr_inside {
            self.exit_count += 1;
            self.current_count = (self.current_count - 1).max(0);
        }
    }
}

pub struct ZoneCounter {
    zones: Vec<Zone>,
    track_positions: std::collections::HashMap<u32, (f32, f32)>,
}

impl ZoneCounter {
    pub fn new(zones: Vec<Zone>) -> Self {
        Self {
            zones,
            track_positions: std::collections::HashMap::new(),
        }
    }

    pub fn update(&mut self, tracked_detections: &[Detection]) {
        for det in tracked_detections {
            if let Some(track_id) = det.track_id {
                let (cx, cy) = det.center();

                // Check previous position
                let prev_pos = self.track_positions.get(&track_id).copied();

                // Update zones
                for zone in &mut self.zones {
                    let curr_inside = zone.contains_point(cx, cy);

                    if let Some((prev_x, prev_y)) = prev_pos {
                        let prev_inside = zone.contains_point(prev_x, prev_y);
                        zone.update_count(prev_inside, curr_inside);
                    }
                }

                // Update position
                self.track_positions.insert(track_id, (cx, cy));
            }
        }

        // Clean up old tracks
        let active_ids: std::collections::HashSet<u32> = tracked_detections
            .iter()
            .filter_map(|d| d.track_id)
            .collect();

        self.track_positions.retain(|id, _| active_ids.contains(id));
    }

    pub fn get_zones(&self) -> &[Zone] {
        &self.zones
    }

    pub fn get_zone_stats(&self, zone_id: &str) -> Option<(u32, u32, i32)> {
        self.zones
            .iter()
            .find(|z| z.id == zone_id)
            .map(|z| (z.entry_count, z.exit_count, z.current_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytetrack_basic() {
        let mut tracker = ByteTracker::new();

        let det1 = Detection {
            x: 0.1,
            y: 0.1,
            width: 0.1,
            height: 0.2,
            confidence: 0.9,
            track_id: None,
        };

        let tracked = tracker.update(vec![det1.clone()]);
        assert_eq!(tracked.len(), 0); // Tentative track

        // After min_hits updates
        for _ in 0..2 {
            tracker.update(vec![det1.clone()]);
        }

        let tracked = tracker.update(vec![det1.clone()]);
        assert_eq!(tracked.len(), 1);
        assert!(tracked[0].track_id.is_some());
    }

    #[test]
    fn test_zone_contains_point() {
        let zone = Zone::new(
            "test".to_string(),
            "Test Zone".to_string(),
            vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        );

        assert!(zone.contains_point(0.5, 0.5)); // Inside
        assert!(!zone.contains_point(1.5, 0.5)); // Outside
        assert!(zone.contains_point(0.1, 0.1)); // Inside
    }
}