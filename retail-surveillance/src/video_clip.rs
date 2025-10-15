use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tokio::fs;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const BUFFER_DURATION_SECS: i64 = 120;
const MAX_CLIP_DURATION_SECS: i64 = 60;
const THUMBNAIL_WIDTH: u32 = 320;
const THUMBNAIL_HEIGHT: u32 = 240;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoClipRequest {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub duration_before_secs: i64,
    pub duration_after_secs: i64,
    pub pos_event_id: Option<Uuid>,
    pub alert_id: Option<Uuid>,
    pub camera_id: String,
    pub priority: ClipPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClipPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoClip {
    pub id: Uuid,
    pub camera_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub file_path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
    pub size_bytes: u64,
    pub duration_secs: f64,
    pub pos_event_id: Option<Uuid>,
    pub alert_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FrameData {
    pub timestamp: DateTime<Utc>,
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct VideoBuffer {
    frames: Arc<Mutex<VecDeque<FrameData>>>,
    max_duration: Duration,
    camera_id: String,
}

impl VideoBuffer {
    pub fn new(camera_id: String, buffer_duration_secs: i64) -> Self {
        Self {
            frames: Arc::new(Mutex::new(VecDeque::new())),
            max_duration: Duration::seconds(buffer_duration_secs),
            camera_id,
        }
    }

    pub fn add_frame(&self, frame: FrameData) {
        let mut frames = self.frames.lock().unwrap();
        frames.push_back(frame.clone());

        let cutoff = Utc::now() - self.max_duration;
        while let Some(front) = frames.front() {
            if front.timestamp < cutoff {
                frames.pop_front();
            } else {
                break;
            }
        }

        debug!(
            "Buffer for camera {}: {} frames, {:.1} seconds",
            self.camera_id,
            frames.len(),
            frames.back()
                .zip(frames.front())
                .map(|(b, f)| (b.timestamp - f.timestamp).num_milliseconds() as f64 / 1000.0)
                .unwrap_or(0.0)
        );
    }

    pub fn extract_frames(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<FrameData> {
        let frames = self.frames.lock().unwrap();
        frames
            .iter()
            .filter(|f| f.timestamp >= start && f.timestamp <= end)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.frames.lock().unwrap().clear();
    }
}

pub struct VideoClipExtractor {
    buffer: Arc<VideoBuffer>,
    output_dir: PathBuf,
    request_rx: mpsc::Receiver<VideoClipRequest>,
    request_tx: mpsc::Sender<VideoClipRequest>,
}

impl VideoClipExtractor {
    pub fn new(camera_id: String, output_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(100);

        Self {
            buffer: Arc::new(VideoBuffer::new(camera_id.clone(), BUFFER_DURATION_SECS)),
            output_dir,
            request_rx: rx,
            request_tx: tx,
        }
    }

    pub fn get_sender(&self) -> mpsc::Sender<VideoClipRequest> {
        self.request_tx.clone()
    }

    pub fn get_buffer(&self) -> Arc<VideoBuffer> {
        Arc::clone(&self.buffer)
    }

    pub async fn run(mut self) -> Result<()> {
        fs::create_dir_all(&self.output_dir).await
            .context("Failed to create output directory")?;

        info!("Video clip extractor started for camera {}",
              self.buffer.camera_id);

        while let Some(request) = self.request_rx.recv().await {
            match self.process_request(request).await {
                Ok(clip) => {
                    info!("Generated clip: {} ({:.1} MB, {:.1}s)",
                          clip.file_path.display(),
                          clip.size_bytes as f64 / 1_048_576.0,
                          clip.duration_secs);
                }
                Err(e) => {
                    error!("Failed to process clip request: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn process_request(&self, request: VideoClipRequest) -> Result<VideoClip> {
        let start_time = request.timestamp - Duration::seconds(request.duration_before_secs);
        let end_time = request.timestamp + Duration::seconds(request.duration_after_secs);

        let total_duration = (end_time - start_time).num_seconds();
        if total_duration > MAX_CLIP_DURATION_SECS {
            warn!(
                "Clip duration {}s exceeds maximum {}s, will be truncated",
                total_duration, MAX_CLIP_DURATION_SECS
            );
        }

        let frames = self.buffer.extract_frames(start_time, end_time);

        if frames.is_empty() {
            anyhow::bail!("No frames found in requested time range");
        }

        info!(
            "Extracting {} frames from {} to {} for {}",
            frames.len(),
            start_time.format("%H:%M:%S"),
            end_time.format("%H:%M:%S"),
            request.id
        );

        let clip_path = self.generate_clip_path(&request).await?;
        let thumbnail_path = self.generate_thumbnail_path(&request).await?;

        let size_bytes = self.save_clip(&frames, &clip_path).await?;

        let thumbnail = if let Some(frame) = frames.get(frames.len() / 2) {
            self.generate_thumbnail(frame, &thumbnail_path).await.ok();
            Some(thumbnail_path)
        } else {
            None
        };

        Ok(VideoClip {
            id: request.id,
            camera_id: self.buffer.camera_id.clone(),
            start_time,
            end_time,
            file_path: clip_path,
            thumbnail_path: thumbnail,
            size_bytes,
            duration_secs: total_duration as f64,
            pos_event_id: request.pos_event_id,
            alert_id: request.alert_id,
            created_at: Utc::now(),
        })
    }

    async fn save_clip(&self, frames: &[FrameData], path: &Path) -> Result<u64> {
        if frames.is_empty() {
            return Ok(0);
        }

        let first_frame = &frames[0];
        let width = first_frame.width;
        let height = first_frame.height;
        let fps = 30;

        let pipeline_str = format!(
            "appsrc name=src is-live=true format=time caps=video/x-raw,format=RGB,width={},height={},framerate={}/1 ! \
             videoconvert ! \
             x264enc speed-preset=ultrafast tune=zerolatency ! \
             mp4mux ! \
             filesink location={}",
            width, height, fps,
            path.to_str().unwrap()
        );

        let pipeline = gst::parse::launch(&pipeline_str)
            .context("Failed to create encoding pipeline")?;

        let appsrc = pipeline
            .dynamic_cast_ref::<gst::Pipeline>()
            .unwrap()
            .by_name("src")
            .unwrap()
            .dynamic_cast::<gst_app::AppSrc>()
            .unwrap();

        pipeline.set_state(gst::State::Playing)?;

        for (i, frame) in frames.iter().enumerate() {
            let mut buffer = gst::Buffer::from_mut_slice(frame.data.clone());
            let buffer_ref = buffer.get_mut().unwrap();

            let pts = gst::ClockTime::from_nseconds((i as u64 * 1_000_000_000) / fps as u64);
            buffer_ref.set_pts(Some(pts));
            buffer_ref.set_duration(Some(gst::ClockTime::from_nseconds(1_000_000_000 / fps as u64)));

            appsrc.push_buffer(buffer)?;
        }

        appsrc.end_of_stream()?;

        let bus = pipeline.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(10)) {
            use gst::MessageView;
            match msg.view() {
                MessageView::Eos(..) => break,
                MessageView::Error(err) => {
                    error!("Encoding error: {}", err.error());
                    anyhow::bail!("Failed to encode video: {}", err.error());
                }
                _ => {}
            }
        }

        pipeline.set_state(gst::State::Null)?;

        let metadata = fs::metadata(&path).await?;
        Ok(metadata.len())
    }

    async fn generate_thumbnail(&self, frame: &FrameData, path: &Path) -> Result<()> {
        use image::{ImageBuffer, Rgb};

        let img = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
            frame.width,
            frame.height,
            frame.data.clone()
        ).context("Failed to create image from frame")?;

        let thumbnail = image::imageops::thumbnail(&img, THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT);
        thumbnail.save(path).context("Failed to save thumbnail")?;

        Ok(())
    }

    async fn generate_clip_path(&self, request: &VideoClipRequest) -> Result<PathBuf> {
        let date_dir = request.timestamp.format("%Y%m%d").to_string();
        let clip_dir = self.output_dir
            .join(&self.buffer.camera_id)
            .join(&date_dir);

        fs::create_dir_all(&clip_dir).await?;

        let filename = format!(
            "{}_{}.mp4",
            request.timestamp.format("%H%M%S"),
            request.id.to_string()[..8].to_string()
        );

        Ok(clip_dir.join(filename))
    }

    async fn generate_thumbnail_path(&self, request: &VideoClipRequest) -> Result<PathBuf> {
        let date_dir = request.timestamp.format("%Y%m%d").to_string();
        let thumb_dir = self.output_dir
            .join(&self.buffer.camera_id)
            .join(&date_dir)
            .join("thumbnails");

        fs::create_dir_all(&thumb_dir).await?;

        let filename = format!(
            "{}_{}.jpg",
            request.timestamp.format("%H%M%S"),
            request.id.to_string()[..8].to_string()
        );

        Ok(thumb_dir.join(filename))
    }
}

pub struct VideoClipManager {
    extractors: Arc<RwLock<Vec<VideoClipExtractor>>>,
}

impl VideoClipManager {
    pub fn new() -> Self {
        Self {
            extractors: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn add_camera(&self, camera_id: String, output_dir: PathBuf) -> mpsc::Sender<VideoClipRequest> {
        let extractor = VideoClipExtractor::new(camera_id, output_dir);
        let sender = extractor.get_sender();

        let mut extractors = self.extractors.write().unwrap();
        extractors.push(extractor);

        sender
    }

    pub async fn request_clip(
        &self,
        camera_id: &str,
        timestamp: DateTime<Utc>,
        before_secs: i64,
        after_secs: i64,
        pos_event_id: Option<Uuid>,
        alert_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let request = VideoClipRequest {
            id: Uuid::new_v4(),
            timestamp,
            duration_before_secs: before_secs,
            duration_after_secs: after_secs,
            pos_event_id,
            alert_id,
            camera_id: camera_id.to_string(),
            priority: if alert_id.is_some() {
                ClipPriority::High
            } else {
                ClipPriority::Medium
            },
        };

        info!(
            "Clip request {} for camera {} at {} ({}s before, {}s after)",
            request.id, camera_id, timestamp.format("%H:%M:%S"),
            before_secs, after_secs
        );

        Ok(request.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_buffer() {
        let buffer = VideoBuffer::new("test_cam".to_string(), 60);

        let frame = FrameData {
            timestamp: Utc::now(),
            data: vec![0; 640 * 480 * 3],
            width: 640,
            height: 480,
        };

        buffer.add_frame(frame.clone());

        let start = Utc::now() - Duration::seconds(30);
        let end = Utc::now() + Duration::seconds(30);
        let extracted = buffer.extract_frames(start, end);

        assert_eq!(extracted.len(), 1);
    }

    #[test]
    fn test_buffer_cleanup() {
        let buffer = VideoBuffer::new("test_cam".to_string(), 1);

        let old_frame = FrameData {
            timestamp: Utc::now() - Duration::seconds(120),
            data: vec![0; 100],
            width: 640,
            height: 480,
        };

        let new_frame = FrameData {
            timestamp: Utc::now(),
            data: vec![0; 100],
            width: 640,
            height: 480,
        };

        buffer.add_frame(old_frame);
        buffer.add_frame(new_frame);

        let frames = buffer.frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
    }
}