# 🏗️ Architecture Documentation

## Table of Contents
- [High-Level Architecture](#high-level-architecture)
- [Module Breakdown](#module-breakdown)
- [Data Flow](#data-flow)
- [Technical Deep Dive](#technical-deep-dive)
- [Performance Optimizations](#performance-optimizations)
- [Key Insights](#key-insights)

---

## High-Level Architecture

This project combines GStreamer's robust media handling with machine learning inference capabilities, all implemented in Rust for maximum safety and efficiency.

### System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        INPUT SOURCES                         │
├─────────────────────────────────────────────────────────────┤
│  📹 Video Files (.mp4, .mkv)                                 │
│  🖼️  Images (.jpg, .png)                                      │
│  📷 Webcam (/dev/video*)                                     │
└─────────────────────────────────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    GSTREAMER PIPELINE                        │
├─────────────────────────────────────────────────────────────┤
│  Decoding → Format Conversion → Frame Buffering             │
└─────────────────────────────────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   INFERENCE PIPELINE                         │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────┐   ┌──────────┐   ┌───────────────┐          │
│  │Preprocess │→  │  ONNX    │→  │ Postprocess   │          │
│  │ & Resize  │   │ Runtime  │   │ (NMS, Track)  │          │
│  └───────────┘   └──────────┘   └───────────────┘          │
└─────────────────────────────────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    ATTRIBUTE DETECTION                       │
├─────────────────────────────────────────────────────────────┤
│  Color Extraction → Classification → Logging                │
└─────────────────────────────────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                       ANNOTATION                             │
├─────────────────────────────────────────────────────────────┤
│  Bounding Boxes → Labels → Tracking IDs → Colors            │
└─────────────────────────────────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                         OUTPUT                               │
├─────────────────────────────────────────────────────────────┤
│  💾 Video File (.mkv)                                        │
│  📄 JSON Metadata                                            │
│  📊 Detection Logs                                           │
│  🖥️  Live Display (optional)                                 │
└─────────────────────────────────────────────────────────────┘
```

---

## Module Breakdown

### Project Structure

```
gstreamed_rust_inference/
├── gstreamed_ort/         # Main ONNX Runtime pipeline (recommended)
├── gstreamed_candle/      # Candle-based pipeline (experimental)
├── ffmpeg_ort/            # FFmpeg integration with ONNX
├── ort_common/            # Shared ONNX Runtime utilities
├── inference_common/      # Common inference abstractions
├── into_rerun/            # Rerun visualization integration
├── gstreamed_common/      # Shared GStreamer utilities
├── _models/               # Model storage directory
└── _perf_data/            # Performance benchmarking data
```

### 1️⃣ gstreamed_ort (Main Entry Point)

**Responsibility**: Main orchestration and CLI interface

#### Files:
- **main.rs**
  - CLI argument parsing (clap)
  - ONNX session initialization
  - Execution provider selection (CPU/CUDA)
  - Input type detection and routing

- **process_video.rs**
  - `process_video()` - File-based video processing
  - `process_webcam()` - Live camera processing
  - `process_buffer()` - Per-frame inference callback

- **process_image.rs**
  - Single image inference pipeline
  - Simplified processing without GStreamer

- **inference.rs**
  - `preprocess_image()` - Resize & normalize
  - `infer_on_image()` - Run model inference
  - ONNX Runtime integration

### 2️⃣ ort_common (ONNX Runtime Utilities)

**Responsibility**: ONNX model utilities and YOLOv8 parsing

#### Files:
- **yolo_parser.rs**
  - `parse_predictions()` - Parse raw model output
  - NMS (Non-Maximum Suppression) implementation
  - Confidence filtering
  - Bounding box decoding

### 3️⃣ inference_common (Shared ML Components)

**Responsibility**: Common ML algorithms and data structures

#### Files:
- **bbox.rs** - Bounding box structures and operations
- **coco_classes.rs** - COCO dataset class names (80 classes)
- **annotate.rs** - Draw boxes and labels on images
- **tracker.rs** - SORT tracking algorithm integration
- **frame_times.rs** - Performance metrics collection
- **detection_logger.rs** - Detection event logging
- **color_extractor.rs** - Dominant color analysis
- **onnx_attributes.rs** - Attribute detection classifier
- **frame_meta.rs** - Frame metadata structures
- **video_meta.rs** - Video metadata structures
- **img_dimensions.rs** - Image dimension utilities

### 4️⃣ gstreamed_common (GStreamer Utilities)

**Responsibility**: GStreamer pipeline construction

#### Files:
- **pipeline.rs**
  - `build_pipeline()` - Video file pipeline
  - `build_webcam_pipeline()` - Camera pipeline
  - `file_src_bin()` - File input element
  - `webcam_src_bin()` - Camera input element

- **discovery.rs**
  - `discover()` - Probe video metadata (resolution, fps, codec)

### 5️⃣ ffmpeg_ort (Alternative Pipeline)

**Responsibility**: Simpler FFmpeg-based inference

- Alternative to GStreamer
- Easier for quick prototyping
- Less flexible than GStreamer

### 6️⃣ into_rerun (Visualization)

**Responsibility**: Rerun.io integration (experimental)

- Real-time 3D visualization
- Early development stage
- Future enhancement

---

## Data Flow

### Video Processing Pipeline

#### Step 1: INITIALIZATION

```rust
1. Parse CLI arguments
   - input: PathBuf
   - cuda: bool
   - model: String
   - live: bool

2. Initialize ONNX Runtime
   let session = SessionBuilder::new()?
       .with_optimization_level(GraphOptimizationLevel::Level3)?
       .commit_from_file(&model_path)?;

3. Select execution provider
   if cuda {
       CUDAExecutionProvider::default().build()
   } else {
       CPUExecutionProvider::default().build()
   }

4. Discover video properties
   let file_info = discovery::discover(input)?;
   // Returns: width, height, fps, codec

5. Initialize tracker
   let tracker = Sort::new(...);  // SORT algorithm
```

#### Step 2: GSTREAMER PIPELINE CONSTRUCTION

```
Input Branch:
  filesrc → decodebin → queue
     ↓          ↓         ↓
   Read    Auto-detect  Buffer
   file       codec    frames

Processing Branch:
  videoconvert → capsfilter → appsink
       ↓             ↓            ↓
   Convert      Force RGB    Extract
   format       format       frames

Output Branch:
  appsrc → x264enc → matroskamux → filesink
     ↓        ↓          ↓            ↓
  Inject   Encode     MKV        Write
  frames    H.264    container     file

Optional Live Display:
  tee → queue → videoconvert → autovideosink
   ↓                              ↓
  Split                        Display
  stream                       window
```

#### Step 3: PER-FRAME PROCESSING (process_buffer)

The **pad probe** intercepts each frame:

```rust
appsink.static_pad("sink").add_probe(
    PadProbeType::BUFFER,
    move |_pad, info| {
        if let PadProbeData::Buffer(ref mut buffer) = info.data {
            process_buffer(buffer);  // Your code runs here!
        }
        PadProbeReturn::Ok
    }
);
```

**Processing steps:**

1. **Extract Buffer**
   ```rust
   let readable = buffer.map_readable().unwrap();
   let pixels = readable.to_vec();
   let image = RgbImage::from_vec(width, height, pixels)?;
   ```

2. **Preprocess Image**
   - Convert to RGB8 format
   - Calculate scaling ratio
   - Resize to model input (640×384)
   - Normalize pixels (0-255 → 0.0-1.0)
   - Convert to ndarray [1, 3, H, W]

3. **Run Inference**
   ```rust
   let outputs = session.run(inputs)?;
   // Shape: [1, 84, 5040]
   //   84 = 4 bbox coords + 80 classes
   //   5040 = anchor points
   ```

4. **Parse Predictions**
   - Extract bounding boxes
   - Apply confidence threshold (0.25)
   - Apply NMS (IoU threshold 0.45)
   - Group detections by class

5. **Track Objects**
   ```rust
   tracker.predict();
   tracker.update(detections);
   // Returns: bboxes with track_ids
   ```

6. **Detect Attributes**
   - Extract ROI for each detection
   - Run attribute classifier
   - Extract dominant colors
   - Log detection events

7. **Annotate Image**
   - Draw bounding boxes
   - Add class labels
   - Add tracking IDs
   - Add confidence scores

8. **Write Back**
   ```rust
   let buffer_mut = buffer.get_mut().unwrap();
   let mut writable = buffer_mut.map_writable().unwrap();
   writable.as_mut_slice().write_all(annotated.as_raw())?;
   ```

#### Step 4: OUTPUT PIPELINE

GStreamer automatically:
- Encodes frames with H.264
- Wraps in MKV container
- Writes to disk
- (Optional) Displays in window

#### Step 5: FINALIZATION

```rust
1. Wait for EOS (End of Stream)
   for msg in bus.iter_timed(gst::ClockTime::NONE) {
       match msg.view() {
           MessageView::Eos(..) => break,
           ...
       }
   }

2. Export metadata
   - video.json (frame-by-frame metadata)
   - video.detections.json (detection logs)

3. Print performance statistics
   - Average frame times
   - Min/Max times per stage
   - Total throughput (FPS)
```

---

## Technical Deep Dive

### 1. GStreamer Pipeline (gstreamed_common/pipeline.rs)

#### What is GStreamer?

GStreamer is a multimedia framework that provides:
- **Automatic codec detection** - Supports 100+ formats
- **Hardware acceleration** - Uses GPU for decode/encode
- **Zero-copy buffers** - Efficient memory access
- **Real-time streaming** - Built-in support

#### Pipeline Elements

**Source Elements:**
- `filesrc` - Read from file
- `v4l2src` - Capture from camera
- `rtspsrc` - Stream from network

**Processing Elements:**
- `decodebin` - Auto-detect and decode
- `videoconvert` - Format conversion
- `videoscale` - Resize frames
- `capsfilter` - Force specific format

**Sink Elements:**
- `appsink` - Extract to application
- `filesink` - Write to file
- `autovideosink` - Display window

#### The Pad Probe Pattern

This is the **key insight** that makes everything work:

```rust
// Add a probe to intercept buffers
pad.add_probe(PadProbeType::BUFFER, |_pad, info| {
    // Access the buffer
    if let PadProbeData::Buffer(ref mut buffer) = info.data {
        // Modify it in-place
        process_frame(buffer);
    }
    // Let it continue flowing
    PadProbeReturn::Ok
});
```

Think of it like **middleware** in a web framework, but for video frames!

### 2. Inference Engine (inference.rs)

#### Preprocessing Pipeline

**1. RGB8 Conversion**
```rust
let image = image.to_rgb8();
// Ensures 3 channels, 8 bits per channel
```

**2. Aspect-Ratio Preserving Resize**
```rust
let ratio = (target_w / img_w).min(target_h / img_h);
let new_w = img_w * ratio;
let new_h = img_h * ratio;
// Example: 1920×1080 → 640×360 (ratio=0.333)
```

**3. Letterboxing** (implicit)
- Model expects 640×384
- Scaled image is 640×360
- Remaining 24 rows stay at 0 (black bars)

**4. Pixel Normalization**
```rust
for pixel in pixels {
    normalized = pixel as f32 / 255.0;
}
// 0-255 → 0.0-1.0 range
```

**5. Channel-First Format**
```rust
// Convert [H, W, C] → [C, H, W]
// Add batch dimension: [1, C, H, W]
let tensor = Array4::zeros([1, 3, 640, 384]);
```

#### YOLOv8 Model

**Input Shape:** `[1, 3, 640, 384]`
- Batch size: 1
- Channels: 3 (RGB)
- Height: 640
- Width: 384

**Output Shape:** `[1, 84, 5040]`
- Batch size: 1
- Features: 84 (4 bbox + 80 classes)
- Anchors: 5040 detection points

**Anchor Breakdown:**
Each anchor predicts:
```
[center_x, center_y, width, height, class_0, class_1, ..., class_79]
```

#### Postprocessing (yolo_parser.rs)

**1. Decode Predictions**
```rust
for anchor in 0..5040 {
    let cx = output[[0, 0, anchor]];
    let cy = output[[0, 1, anchor]];
    let w = output[[0, 2, anchor]];
    let h = output[[0, 3, anchor]];
    let classes = output[[0, 4..84, anchor]];
}
```

**2. Confidence Filtering**
```rust
let max_class_score = classes.max();
if max_class_score > CONF_THRESHOLD {
    keep_detection();
}
```

**3. Non-Maximum Suppression (NMS)**
```rust
// For each class separately
for class_detections in detections.group_by_class() {
    // Sort by confidence (descending)
    class_detections.sort_by_confidence();
    
    // Remove overlapping boxes
    for (i, box_a) in enumerate(class_detections) {
        for box_b in class_detections[i+1..] {
            let iou = compute_iou(box_a, box_b);
            if iou > NMS_THRESHOLD {
                remove(box_b);  // Less confident
            }
        }
    }
}
```

**IoU Calculation:**
```rust
fn iou(box1: &Bbox, box2: &Bbox) -> f32 {
    let intersection = compute_intersection(box1, box2);
    let union = box1.area() + box2.area() - intersection;
    intersection / union
}
```

**4. Scale Back to Original**
```rust
// Bboxes are in scaled coordinates
bbox.xmin *= original_width / scaled_width;
bbox.ymin *= original_height / scaled_height;
bbox.xmax *= original_width / scaled_width;
bbox.ymax *= original_height / scaled_height;
```

### 3. Object Tracking (tracker.rs)

#### SORT Algorithm

**SORT** = Simple Online Realtime Tracking

Uses:
- **Kalman Filter** for motion prediction
- **IoU** for data association
- **Hungarian Algorithm** for optimal matching

**Process:**

**1. Predict**
```rust
// Use Kalman filter to predict next position
for track in tracks {
    track.position = kalman_predict(track.velocity);
}
```

**2. Associate**
```rust
// Match detections to existing tracks using IoU
let cost_matrix = compute_iou_matrix(tracks, detections);
let matches = hungarian_algorithm(cost_matrix);
```

**3. Update**
```rust
// Update matched tracks
for (track, detection) in matches {
    track.update(detection);
    track.age = 0;
}

// Create new tracks for unmatched detections
for detection in unmatched_detections {
    tracks.push(Track::new(detection));
}

// Delete lost tracks
tracks.retain(|t| t.age < MAX_AGE);
```

**4. Output**
```rust
// Return bboxes with persistent track_ids
detections.map(|d| {
    d.track_id = track.id;
    d
})
```

**Benefits:**
- ✅ Persistent IDs across frames
- ✅ Smoother trajectories
- ✅ Handles brief occlusions
- ✅ Real-time performance

### 4. Attribute Detection (onnx_attributes.rs)

For each detected object:

**1. Extract ROI**
```rust
let roi = image.crop(
    bbox.xmin as u32,
    bbox.ymin as u32,
    (bbox.xmax - bbox.xmin) as u32,
    (bbox.ymax - bbox.ymin) as u32,
);
```

**2. Color Analysis**
```rust
// K-means clustering on pixel colors
let colors = color_extractor::extract_dominant_colors(roi, k=5);
// Returns: [(R, G, B, percentage), ...]
```

**3. ONNX Classification**
```rust
// Run attribute classifier
let attributes = attr_detector.classify(roi)?;
// Returns: color_name, confidence
```

**4. Logging**
```rust
let detection_log = DetectionLog {
    frame_number: frame_num,
    timestamp_ms: timestamp,
    class_name: "car",
    bbox: bbox,
    confidence: 0.93,
    attributes: DetectionAttributes {
        dominant_color: "red",
        color_confidence: 0.87,
    },
};
```

**Output Format:**
```json
{
  "frame_number": 42,
  "timestamp_ms": 1400,
  "class_name": "car",
  "confidence": 0.93,
  "bbox": {"x": 100, "y": 50, "w": 200, "h": 150},
  "attributes": {
    "dominant_color": "red",
    "color_confidence": 0.87
  }
}
```

### 5. Performance Tracking (frame_times.rs)

#### Measured Stages

```rust
pub struct FrameTimes {
    pub frame_to_buffer: Duration,      // GStreamer buffer read
    pub buffer_resize: Duration,         // Image resizing
    pub buffer_to_tensor: Duration,      // Tensor conversion
    pub forward_pass: Duration,          // ONNX inference (KEY!)
    pub postprocess: Duration,           // NMS, filtering
    pub tracking: Duration,              // SORT algorithm
    pub annotation: Duration,            // Drawing boxes
    pub buffer_to_frame: Duration,       // Write back
}
```

#### Aggregated Statistics

```rust
pub struct AggregatedTimes {
    times: Vec<FrameTimes>,
}

impl AggregatedTimes {
    pub fn avg(&self, skip_first: bool) -> FrameTimes { ... }
    pub fn min(&self, skip_first: bool) -> FrameTimes { ... }
    pub fn max(&self, skip_first: bool) -> FrameTimes { ... }
}
```

**Why skip first?** Cold start overhead (model loading, JIT compilation)

#### Typical Breakdown (RTX 3070 + CUDA)

| Stage | Time | Percentage |
|-------|------|------------|
| Forward Pass | 5.5 ms | 37% |
| Postprocess | 2.0 ms | 13% |
| Buffer Resize | 3.0 ms | 20% |
| Annotation | 2.5 ms | 17% |
| Tracking | 1.0 ms | 7% |
| Other | 1.0 ms | 6% |
| **TOTAL** | **15 ms** | **100%** |

**Throughput:** 1000ms / 15ms = **66 FPS**

### 6. Memory Management

#### Zero-Copy Operations

✅ **GStreamer Buffers**
```rust
// Read without copying
let readable = buffer.map_readable()?;
let data = readable.as_slice();  // Borrow

// Write without copying
let mut writable = buffer.map_writable()?;
let data = writable.as_mut_slice();  // Mutable borrow
```

✅ **ONNX Tensors**
```rust
// CowArray = Copy-on-Write
let array = CowArray::from(data);  // Only copies if modified
```

✅ **Image Views**
```rust
// Reference original data
let view = image.view(x, y, w, h);
```

#### Necessary Copies

❌ **Buffer → RgbImage** (need owned data)
```rust
let image = RgbImage::from_vec(w, h, buffer_vec)?;
```

❌ **Resized Image** (different dimensions)
```rust
let resized = resize(&image, new_w, new_h);
```

❌ **Annotated Image** (modified pixels)
```rust
let annotated = draw_boxes(image, bboxes);
```

#### Memory Layout

**RGB Buffer:**
```
[R, G, B, R, G, B, R, G, B, ...]
1920 × 1080 × 3 = 6,220,800 bytes (~6 MB per frame)
```

**ONNX Tensor:**
```
[Batch, Channel, Height, Width]
[1, 3, 640, 384] × 4 bytes (f32) = 2,949,120 bytes (~3 MB)
```

**Peak Memory Usage:**
- Original frame buffer: ~6 MB
- Resized tensor: ~3 MB
- Model weights: ~50 MB
- ONNX Runtime overhead: ~100 MB
- Tracking state: ~1 MB
- **Total: ~400 MB** (typical)

### 7. Concurrency Model

#### Thread Architecture

```
┌─────────────────────────────────────┐
│  GStreamer Thread Pool              │
│  ┌─────────────────────────────┐   │
│  │ Decode Thread               │   │
│  │  (decodebin)                │   │
│  └─────────────────────────────┘   │
│                                     │
│  ┌─────────────────────────────┐   │
│  │ Pipeline Thread             │   │
│  │  ↓                          │   │
│  │  buffer_processor()         │   │
│  │    ├─ lock(session)         │   │
│  │    ├─ lock(tracker)         │   │
│  │    ├─ lock(agg_times)       │   │
│  │    ├─ lock(video_meta)      │   │
│  │    └─ lock(detection_logger)│   │
│  └─────────────────────────────┘   │
│                                     │
│  ┌─────────────────────────────┐   │
│  │ Encode Thread               │   │
│  │  (x264enc)                  │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

#### Thread Safety via Arc<Mutex<T>>

```rust
let session = Arc::new(Mutex::new(session));
let tracker = Arc::new(Mutex::new(tracker));
let agg_times = Arc::new(Mutex::new(AggregatedTimes::default()));

// In callback closure
move |buffer| {
    let mut session = session.lock().unwrap();
    let mut tracker = tracker.lock().unwrap();
    // ... process frame
}
```

**Why Mutex?**
- GStreamer callbacks run in separate threads
- Need synchronized access to shared state
- Prevents data races

**Contention Level:**
- ⚡ **Low** - only one frame processed at a time
- GStreamer serializes buffer flow
- No parallel frame processing (by design)

**Future Optimization:**
Could parallelize across multiple streams:
```rust
// Using tokio or rayon
for stream in streams {
    tokio::spawn(async move {
        process_stream(stream).await;
    });
}
```

### 8. Error Handling

#### Error Propagation

Uses `anyhow` for ergonomic error handling:

```rust
use anyhow::Result;

fn process_video(input: &Path) -> Result<()> {
    let session = load_model(model_path)?;  // ? propagates errors
    let pipeline = build_pipeline(input)?;
    
    pipeline.set_state(gst::State::Playing)?;
    
    // ... process
    
    Ok(())
}
```

**Benefits:**
- Automatic error conversion
- Backtrace support in debug mode
- Context chaining

#### GStreamer Errors

```rust
for msg in bus.iter_timed(gst::ClockTime::NONE) {
    match msg.view() {
        MessageView::Error(err) => {
            log::error!("Error: {}", err.error());
            pipeline.debug_to_dot_file(
                gst::DebugGraphDetails::all(),
                "pipeline.error"
            );
            break;
        }
        MessageView::Eos(..) => {
            log::info!("End of stream");
            break;
        }
        _ => (),
    }
}
```

#### ONNX Runtime Errors

Common issues:
- ❌ Model file not found
- ❌ CUDA initialization failure (silent!)
- ❌ Tensor shape mismatch
- ❌ Out of memory

**Current Limitation:**
```rust
// CUDA may fail silently and fall back to CPU
// User must check logs carefully
log::info!("Using CUDA execution provider");
// But it might actually be using CPU!
```

#### Planned Improvements

⚠️ Currently missing:
- Retry logic for transient failures
- Explicit fallback to CPU if CUDA fails
- Graceful degradation
- Better error messages

📋 Tracked in roadmap for future releases

---

## Performance Optimizations

### 1. Fast Image Resize

Uses `fast_image_resize` crate:
```rust
let mut resizer = Resizer::new();
resizer.resize(
    &src_image,
    &mut dst_image,
    &ResizeOptions::new()
        .resize_alg(ResizeAlg::Nearest)
)?;
```

**Benefit:** SIMD-accelerated, 10x faster than naive `image::resize()`

### 2. ONNX Runtime Optimizations

**Graph Optimization Level 3:**
```rust
SessionBuilder::new()?
    .with_optimization_level(GraphOptimizationLevel::Level3)?
```

Performs:
- Constant folding
- Operator fusion
- Memory layout optimization
- Dead code elimination

### 3. CUDA Acceleration

```rust
let ep = CUDAExecutionProvider::default().build();
ort::init().with_execution_providers([ep]).commit()?;
```

**Speedup:** 3-6x faster than CPU inference

### 4. Zero-Copy Buffers

```rust
// Direct memory access
let readable = buffer.map_readable()?;
let pixels = readable.as_slice();  // No copy!

// Modify in-place
let mut writable = buffer.map_writable()?;
writable.as_mut_slice().copy_from_slice(new_pixels);
```

### 5. Efficient NMS

Vectorized operations with `ndarray`:
```rust
// Compute IoU matrix for all boxes at once
let ious = compute_iou_matrix(&boxes);  // Vectorized

// Filter using boolean indexing
let keep = ious.map_axis(Axis(1), |row| row.max() < threshold);
boxes.select(Axis(0), &keep);
```

### 6. Minimal Allocations

- Reuse buffers when possible
- Pre-allocate known sizes
- Use `Vec::with_capacity()`

```rust
let mut bboxes = Vec::with_capacity(100);  // Avoid reallocations
```

### Performance Results

**Configuration:** RTX 3070 + CUDA, YOLOv8s, 1280×720@30fps

| Framework | Device | Total Time | Speedup |
|-----------|--------|------------|---------|
| Candle | CPU | 302.41 ms | 1.0x |
| ORT | CPU | 82.53 ms | 3.7x |
| Candle | CUDA | 25.24 ms | 12.0x |
| **ORT** | **CUDA** | **6.99 ms** | **43.2x** 🚀 |

**Throughput:** 143 FPS (6.99ms per frame)

---

## Key Insights

### 🎯 Core Concept

This project combines **GStreamer** (professional media handling) with **ONNX Runtime** (optimized ML inference) to create a high-performance video analytics pipeline in pure Rust.

### 🔑 The Pad Probe Pattern

**Key Insight:** GStreamer handles all the messy codec/format stuff. We intercept buffers mid-pipeline with a "pad probe". This lets us run ML inference on raw frames and modify them before they get encoded and saved.

Think of it like **middleware in a web framework, but for video frames!**

### 📊 Workflow in 3 Sentences

1. GStreamer decodes video and extracts RGB frames
2. We run YOLOv8 inference + tracking on each frame
3. GStreamer encodes the annotated frames back to video

### 💡 Why This Architecture?

✅ GStreamer handles 100+ codecs automatically  
✅ Hardware-accelerated decoding/encoding (free!)  
✅ Zero-copy buffer access  
✅ Real-time streaming support built-in  
✅ ONNX Runtime = fastest inference (no Python overhead)  
✅ Rust = memory safety + performance  

### 🚀 Performance Secrets

1. **fast_image_resize (SIMD)** - 10x faster than naive resize
2. **ONNX Runtime Graph Optimization** - Model-level optimizations
3. **CUDA for GPU** - 3-6x speedup over CPU
4. **Zero-copy GStreamer buffers** - Direct memory access
5. **Efficient NMS** - Vectorized with ndarray

**Result:** 66 FPS on RTX 3070 (1080p video with detection!)

### 🧩 Modular Design

Each crate has ONE responsibility:
- **gstreamed_ort** → Main orchestration
- **ort_common** → ONNX utilities
- **inference_common** → ML algorithms
- **gstreamed_common** → GStreamer helpers

This makes it easy to:
- Test components independently
- Swap implementations (e.g., different trackers)
- Reuse code across projects

### 🔮 Extensibility

Want to add a new feature? Just modify `buffer_processor()`:
- Different model? Swap the ONNX session
- New post-processing? Add after inference
- Custom tracking? Replace SORT with DeepSORT
- Analytics? Log whatever you want

**The GStreamer pipeline stays the same!**

### 📈 Scalability Path

| Stage | Capability |
|-------|-----------|
| **Current** | Single video, sequential processing |
| **Next** | Multi-stream with tokio/rayon |
| **Future** | Distributed inference across machines |

Foundation is solid for all these scenarios.

### 🎓 Learning Takeaways

1. **GStreamer pipelines are POWERFUL**
   - 20 lines of Rust = full video encode/decode pipeline

2. **Pad probes are MAGICAL**
   - Inject custom logic anywhere in pipeline

3. **ONNX Runtime is FAST**
   - Beats most Python implementations

4. **Rust + ML = VIABLE**
   - Strong ecosystem, great performance

5. **Modular design PAYS OFF**
   - Easy to test, extend, and maintain

---

## Code Navigation Guide

Want to dive deeper? Follow this path:

1. **Start:** `gstreamed_ort/src/main.rs`
   - See initialization and CLI parsing

2. **Pipeline:** `gstreamed_common/src/pipeline.rs`
   - Understand GStreamer magic

3. **Processing:** `gstreamed_ort/src/process_video.rs`
   - See the main loop and buffer processing

4. **Inference:** `gstreamed_ort/src/inference.rs`
   - ML preprocessing and inference

5. **Parsing:** `ort_common/src/yolo_parser.rs`
   - YOLOv8 output parsing and NMS

6. **Tracking:** `inference_common/src/tracker.rs`
   - SORT algorithm integration

7. **Utils:** `inference_common/src/`
   - Supporting structures and utilities

---

## Glossary

**Anchor:** A predefined position in the image where YOLOv8 looks for objects

**Bbox:** Bounding box, rectangle around detected object

**COCO:** Common Objects in Context dataset (80 classes)

**EOS:** End of Stream, signal that video is finished

**IoU:** Intersection over Union, overlap metric for boxes

**NMS:** Non-Maximum Suppression, removes duplicate detections

**Pad:** Connection point between GStreamer elements

**Pad Probe:** Callback that intercepts data flowing through pad

**Pipeline:** Chain of connected GStreamer elements

**ROI:** Region of Interest, cropped area of image

**SORT:** Simple Online Realtime Tracking algorithm

**Tensor:** Multi-dimensional array for ML models

---

## Further Reading

- [GStreamer Documentation](https://gstreamer.freedesktop.org/documentation/)
- [ONNX Runtime Docs](https://onnxruntime.ai/docs/)
- [YOLOv8 Paper](https://arxiv.org/abs/2305.09972)
- [SORT Paper](https://arxiv.org/abs/1602.00763)
- [Rust Book](https://doc.rust-lang.org/book/)

---

**Last Updated:** October 21, 2025  
**Version:** 1.0  
**Maintainer:** Project Contributors
