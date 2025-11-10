# TUI Live Video Integration - Implementation Guide

**Date**: November 10, 2025  
**Goal**: Display live camera feed inside the TUI dashboard  
**Status**: Ready to implement

---

## ✅ Yes, We Can Do This!

Using **`ratatui-image`** crate with automatic protocol detection and fallback.

---

## 🎯 Implementation Plan

### **Technology Stack**
- **ratatui-image** - Display images in TUI (supports Kitty, Sixel, Half-blocks)
- **Existing pipeline** - Already captures and processes frames
- **Channel messaging** - Send frames from pipeline to TUI

---

## 📋 Step-by-Step Implementation

### Step 1: Add Dependency (5 minutes)

Update `gstreamed_ort/Cargo.toml`:
```toml
[dependencies]
ratatui-image = "9.0.0-beta.0"
```

### Step 2: Update TUI App State (30 minutes)

**File**: `gstreamed_ort/src/tui/app.rs`

```rust
use ratatui_image::picker::Picker;
use image::DynamicImage;

pub struct TuiApp {
    // ... existing fields ...
    
    // NEW: Video preview
    pub show_video_preview: bool,
    pub video_picker: Picker,
    pub current_frame_image: Option<DynamicImage>,
    pub video_preview_fps: f32,
}

impl TuiApp {
    pub fn new() -> Self {
        Self {
            // ... existing initialization ...
            show_video_preview: true,  // Show by default
            video_picker: Picker::from_termios().unwrap(),
            current_frame_image: None,
            video_preview_fps: 0.0,
        }
    }
}

// Add new message type
pub enum TuiMessage {
    VideoInfo { ... },           // existing
    FrameProcessed { ... },      // existing
    
    // NEW: Send frame image for preview
    FrameImage {
        frame_num: u64,
        image: DynamicImage,
    },
    
    Error(String),               // existing
    Finished,                    // existing
}

impl TuiApp {
    pub fn handle_message(&mut self, msg: TuiMessage) {
        match msg {
            // ... existing handlers ...
            
            TuiMessage::FrameImage { frame_num, image } => {
                self.current_frame_image = Some(image);
                // Update FPS counter for preview
                // (optional: track timing)
            }
            
            // ... rest of handlers ...
        }
    }
    
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // ... existing key handlers ...
            
            // NEW: Toggle video preview with 'V' key
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.show_video_preview = !self.show_video_preview;
            }
            
            // ... rest of handlers ...
        }
        false
    }
}
```

### Step 3: Add Video Panel to UI (2 hours)

**File**: `gstreamed_ort/src/tui/ui.rs`

```rust
use ratatui_image::{StatefulImage, Resize, protocol::StatefulProtocol};

pub fn draw(frame: &mut Frame, app: &mut TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Min(0),         // Main content (CHANGED)
            Constraint::Length(3),      // Footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], app);
    draw_main_content(frame, chunks[1], app);  // NEW function
    draw_footer(frame, chunks[2], app);
}

fn draw_main_content(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Split main area: video on left, stats on right
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),  // Video panel
            Constraint::Percentage(50),  // Stats panel
        ])
        .split(area);
    
    // Left side: Video + Detections
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70),  // Video preview
            Constraint::Percentage(30),  // Detection list
        ])
        .split(main_chunks[0]);
    
    if app.show_video_preview {
        draw_video_preview(frame, left_chunks[0], app);
    } else {
        draw_video_placeholder(frame, left_chunks[0]);
    }
    
    draw_detection_list(frame, left_chunks[1], app);
    
    // Right side: Crowd stats, ROI zones, Performance
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),  // Crowd monitor
            Constraint::Percentage(30),  // ROI zones
            Constraint::Percentage(30),  // Performance
        ])
        .split(main_chunks[1]);
    
    draw_crowd_panel(frame, right_chunks[0], app);
    draw_roi_panel(frame, right_chunks[1], app);
    draw_performance_panel(frame, right_chunks[2], app);
}

fn draw_video_preview(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    let block = Block::default()
        .title("📹 Live Video Preview")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner_area = block.inner(area);
    frame.render_widget(block, area);
    
    if let Some(img) = &app.current_frame_image {
        // Create image protocol handler
        let dyn_img = app.video_picker.new_resize_protocol(img.clone());
        let mut state = StatefulProtocol::new(dyn_img);
        
        // Create widget with fit resize
        let widget = StatefulImage::new(None)
            .resize(Resize::Fit(None));
        
        frame.render_stateful_widget(widget, inner_area, &mut state);
    } else {
        // Show loading message
        let loading = Paragraph::new("Waiting for video frames...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(loading, inner_area);
    }
}

fn draw_video_placeholder(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title("📹 Video Preview (Disabled)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    let text = Paragraph::new("Press 'V' to enable video preview")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .block(block);
    
    frame.render_widget(text, area);
}
```

### Step 4: Send Frames from Pipeline (3 hours)

**File**: `gstreamed_ort/src/process_video.rs`

```rust
// In the main processing loop, after frame annotation

// Send detection metadata (existing)
if let Some(tx) = &tui_tx {
    tx.send(TuiMessage::FrameProcessed {
        frame_num,
        timestamp_ms,
        detections: detection_logs,
        performance: frame_times.clone(),
    })?;
    
    // NEW: Send video frame preview
    // Only send every Nth frame to reduce bandwidth
    // For 30 FPS video, send every 3rd frame = 10 FPS preview
    const PREVIEW_FRAME_INTERVAL: u64 = 3;
    
    if frame_num % PREVIEW_FRAME_INTERVAL == 0 {
        // Downscale image for preview to reduce memory/CPU
        let preview_size = 320u32; // Width in pixels
        let aspect_ratio = image.height() as f32 / image.width() as f32;
        let preview_height = (preview_size as f32 * aspect_ratio) as u32;
        
        let preview_img = image::imageops::resize(
            &image,
            preview_size,
            preview_height,
            image::imageops::FilterType::Nearest,  // Fast
        );
        
        let preview_dynamic = DynamicImage::ImageRgb8(preview_img);
        
        // Send to TUI (non-blocking)
        let _ = tx.try_send(TuiMessage::FrameImage {
            frame_num,
            image: preview_dynamic,
        });
    }
}
```

### Step 5: Update Footer Help Text

**File**: `gstreamed_ort/src/tui/ui.rs`

```rust
fn draw_footer(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let help_text = if app.roi_mode {
        "[←↑↓→] Move | [Shift+Arrows] Resize | [N] New Zone | [D] Delete | [S] Save | [Esc] Exit ROI"
    } else {
        "[V] Video | [C] Crowd | [R] ROI Zones | [L] Living Beings | [Q] Quit"
    };
    
    // ... rest of footer rendering ...
}
```

---

## 🎨 Final TUI Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ 📹 Surveillance Monitor - rtsp://camera/stream                      │
│ Frame: 1523 | FPS: 29.8 | Resolution: 1920x1080 | CUDA             │
├─────────────────────────────────┬──────────────────────────────────┤
│ 📹 Live Video Preview           │ 👥 CROWD MONITOR                  │
│ ┌─────────────────────────────┐ │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━│
│ │                             │ │ Count: 12 people                 │
│ │     ▄▀▀▀▀▄    ▄▀▀▀▀▄        │ │ Level: ✅ Normal                 │
│ │    █ 👤  █  █ 👤  █        │ │ Trend: ↗️ Rising                 │
│ │    █      █  █      █        │ │                                  │
│ │     ▀▄▄▄▄▀    ▀▄▄▄▄▀        │ │ Zone Breakdown:                  │
│ │           🚗                 │ │ ┌─ Entrance ───────┐            │
│ │  ▄▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▄       │ │ │ 3 people          │            │
│ │ █                    █       │ │ │ ▂▃▄▅              │            │
│ │  ▀▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀       │ │ └───────────────────┘            │
│ └─────────────────────────────┘ │                                  │
│                                 │ ┌─ Main Area ───────┐            │
│ 🔍 DETECTIONS (4)               │ │ 7 people          │            │
│ ┌───────────────────────────┐   │ │ ▃▅▆▇▆▅            │            │
│ │ 👤 person   0.95 #12      │   │ └───────────────────┘            │
│ │ 👤 person   0.89 #15      │   ├──────────────────────────────────┤
│ │ 🚗 car      0.92 #3       │   │ 📍 ROI ZONES (2 active)          │
│ │ 🚴 bicycle  0.78 #8       │   │ • Entrance: 3 detections         │
│ └───────────────────────────┘   │ • Main Area: 7 detections        │
│                                 ├──────────────────────────────────┤
│                                 │ ⚡ PERFORMANCE                    │
│                                 │ Inference:  6.8ms                │
│                                 │ Total:     12.3ms                │
│                                 │ ▃▅▆▇█▇▆▅                         │
└─────────────────────────────────┴──────────────────────────────────┘
│ [V] Video | [C] Crowd | [R] ROI | [L] Living | [Q] Quit            │
└────────────────────────────────────────────────────────────────────┘
```

---

## ⚡ Performance Optimizations

### 1. Frame Rate Control
```rust
const PREVIEW_FRAME_INTERVAL: u64 = 3;  // Send every 3rd frame = 10 FPS
```

### 2. Downscaling
```rust
// Reduce to 320px width (from 1920px)
// Saves ~85% memory and bandwidth
let preview_size = 320;
```

### 3. Non-blocking Send
```rust
// Don't block pipeline if TUI is slow
let _ = tx.try_send(TuiMessage::FrameImage { ... });
```

### 4. Fast Resize Filter
```rust
FilterType::Nearest  // Fastest, acceptable for preview
```

**Expected Overhead**: < 5% CPU, < 50 MB RAM

---

## 🧪 Testing on Different Terminals

### Best Quality (Full Color, High Res)
```bash
# Kitty Terminal
kitty -e cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui

# WezTerm
wezterm -e cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui
```

### Good Quality (Sixel)
```bash
# xterm with sixel
xterm -ti vt340 -e cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui

# foot terminal
foot cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui
```

### Universal (Works Everywhere)
```bash
# Any terminal (fallback to half-blocks)
gnome-terminal -- cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui
alacritty -e cargo run -r -p gstreamed_ort -- rtsp://camera/stream --tui
```

---

## 🎯 Implementation Checklist

- [ ] Add `ratatui-image` dependency to Cargo.toml
- [ ] Update `TuiApp` struct with video fields
- [ ] Add `FrameImage` message variant
- [ ] Implement `draw_video_preview()` function
- [ ] Update main content layout (split screen)
- [ ] Send downscaled frames from pipeline
- [ ] Add 'V' key toggle for video preview
- [ ] Update footer help text
- [ ] Test on Kitty/WezTerm (best quality)
- [ ] Test on standard terminal (fallback)
- [ ] Optimize frame rate (10 FPS preview)
- [ ] Document terminal requirements

---

## 📦 Estimated Time

- **Setup & Dependencies**: 30 minutes
- **TUI State Updates**: 1 hour
- **UI Layout Changes**: 2 hours
- **Pipeline Integration**: 2 hours
- **Testing & Optimization**: 2 hours
- **Documentation**: 1 hour

**Total**: ~2 days (16 hours)

---

## 🚀 Ready to Implement!

This will give you:
✅ Live camera feed inside TUI dashboard  
✅ Works on any terminal (auto-fallback)  
✅ Low overhead (< 5% CPU)  
✅ Toggle on/off with 'V' key  
✅ Professional monitoring interface  

Shall I start implementing this?

