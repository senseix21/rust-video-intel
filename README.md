# GStreamer × ML Inference in Rust 🦀

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

A high-performance computer vision pipeline framework that combines GStreamer's robust media handling with machine learning inference capabilities, all implemented in Rust for maximum safety and efficiency.

## 🎯 Overview

This project provides a modular, production-ready framework for running real-time computer vision pipelines on video streams and images. It leverages:

- **[GStreamer](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs)** - Professional-grade video decoding, encoding, and display
- **[ONNX Runtime](https://github.com/pykeio/ort)** - High-performance ML inference via `ort` crate
- **[Candle](https://github.com/huggingface/candle)** - Alternative ML inference backend (optional)
- **[Similari](https://github.com/insight-platform/Similari)** - SORT-based object tracking

### Key Features

- ✅ **Real-time Object Detection** - YOLOv8 with ONNX Runtime or Candle
- ✅ **Object Tracking** - SORT tracker implementation
- ✅ **Multiple Input Formats** - Images, video files, and live streams
- ✅ **Hardware Acceleration** - CUDA support for GPU inference
- ✅ **Attribute Detection** - Enhanced object classification with attribute analysis
- ✅ **Interactive TUI** - Terminal UI dashboard with real-time metrics ([NEW!](#-tui-dashboard))
- ✅ **Modular Architecture** - Clean separation of concerns with workspace structure
- ✅ **Performance Optimized** - Up to 15x faster inference with ONNX Runtime + CUDA

## 📦 Project Structure

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

## 🚀 Quick Start

### Prerequisites

```bash
# Install GStreamer development libraries
# Ubuntu/Debian:
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
     libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-base \
     gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
     gstreamer1.0-plugins-ugly gstreamer1.0-libav

# Fedora/RHEL:
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel

# macOS:
brew install gstreamer
```

### Installation

```bash
# Clone the repository
git clone <repository-url>
cd gstreamed_rust_inference

# Build the project
cargo build --release
```

## 💻 Usage

### Basic Object Detection

#### Process Video File
```bash
cargo run -r -p gstreamed_ort -- video.mp4
# Output: video.mp4.out.mkv
```

#### Process Image
```bash
cargo run -r -p gstreamed_ort -- image.jpg
# Output: image.out.jpg
```

#### Live Display Mode
```bash
cargo run -r -p gstreamed_ort -- video.mp4 --live
```

### 🎨 TUI Dashboard

**NEW!** Interactive terminal UI with real-time metrics and visualizations:

```bash
# Video file with TUI
cargo run -r -p gstreamed_ort -- video.mp4 --tui

# Webcam with TUI
cargo run -r -p gstreamed_ort -- webcam --tui

# Combine with CUDA
cargo run -r -p gstreamed_ort -- video.mp4 --cuda --tui
```

**Features:**
- 📊 Real-time performance graphs (FPS, inference time, sparklines)
- 🎯 Live detection table with scrolling
- 📈 Class distribution histogram
- 🔍 Interactive object inspection
- ⌨️ Keyboard controls (`Q` quit, `↑↓` navigate, `P` pause)

See [TUI_README.md](TUI_README.md) for detailed documentation.

### Advanced Options

#### GPU Acceleration (CUDA)
```bash
cargo run -r -p gstreamed_ort -- video.mp4 --cuda
```

#### Custom Model
```bash
cargo run -r -p gstreamed_ort -- video.mp4 --model path/to/yolov8.onnx
```

#### FFmpeg-based Processing
```bash
cargo run -r -p ffmpeg_ort -- input.mp4
```

### Command-Line Reference

| Option | Description | Default |
|--------|-------------|---------|
| `<INPUT>` | Input file path (video/image) | Required |
| `--cuda` | Enable CUDA acceleration | CPU |
| `--model <PATH>` | Path to custom ONNX model | Built-in YOLOv8 |
| `--live` | Display output in real-time | Disabled |
| `--tui` | Enable interactive TUI dashboard | Disabled |

## 🧠 Models

### Obtaining YOLOv8 Models

1. **Install Ultralytics CLI**
```bash
pip install ultralytics
```

2. **Export ONNX Model**
```bash
# YOLOv8 Small (fastest)
yolo export model=yolov8s.pt format=onnx simplify dynamic

# YOLOv8 Medium (balanced)
yolo export model=yolov8m.pt format=onnx simplify dynamic

# YOLOv8 Large (most accurate)
yolo export model=yolov8l.pt format=onnx simplify dynamic
```

3. **Place Model in Project**
```bash
mv yolov8*.onnx _models/
```

### Supported Model Formats

- ✅ ONNX (recommended)
- ✅ Candle-compatible models (experimental)
- 🚧 TensorFlow Lite (planned)
- 🚧 PyTorch (via Candle, planned)

### Model Classes

Currently supports COCO dataset classes (80 objects):
- Person, bicycle, car, motorcycle, airplane, bus, train, truck, boat...
- Full list: [COCO classes](https://github.com/ultralytics/ultralytics/blob/main/ultralytics/cfg/datasets/coco.yaml)

## ⚡ Performance

### Benchmark Results

Test Configuration: 1280×720 @ 30fps, YOLOv8-small model

#### Machine A: AMD Ryzen 5900X + RTX 3070

| Framework | Device | Preprocess | Inference | Postprocess | **Total** |
|-----------|--------|-----------|-----------|-------------|-----------|
| Candle    | CPU    | 1.14 ms   | 298.64 ms | 2.63 ms     | **302.41 ms** |
| ORT       | CPU    | 0.75 ms   | 80.91 ms  | 0.87 ms     | **82.53 ms** |
| Candle    | CUDA   | 0.09 ms   | 21.76 ms  | 3.39 ms     | **25.24 ms** |
| **ORT**   | **CUDA** | **0.78 ms** | **5.53 ms** | **0.68 ms** | **6.99 ms** ⚡ |

#### Machine B: Intel 12700H + RTX A2000

| Framework | Device | Preprocess | Inference | Postprocess | **Total** |
|-----------|--------|-----------|-----------|-------------|-----------|
| Candle    | CPU    | 3.13 ms   | 589.98 ms | 7.55 ms     | **600.66 ms** |
| ORT       | CPU    | 1.85 ms   | 86.67 ms  | 1.33 ms     | **89.85 ms** |
| Candle    | CUDA   | 0.16 ms   | 38.92 ms  | 6.22 ms     | **45.30 ms** |
| **ORT**   | **CUDA** | **1.37 ms** | **10.06 ms** | **1.20 ms** | **12.63 ms** ⚡ |

### Key Insights

- 🚀 **ONNX Runtime + CUDA is 15-43x faster** than Candle on CPU
- 🎯 **GPU acceleration provides 3-6x speedup** over CPU inference
- ⚡ **ORT consistently outperforms Candle** across all configurations
- 📊 Raw benchmark data available in `_perf_data/` directory

## 🏗️ Architecture

### Module Responsibilities

#### `gstreamed_ort` (Primary Pipeline)
- Main entry point for ONNX-based inference
- GStreamer pipeline management
- Video/image processing orchestration
- Output encoding and display

#### `ort_common`
- ONNX Runtime session management
- Model loading and configuration
- Tensor operations and conversions
- Device selection (CPU/CUDA)

#### `inference_common`
- Inference abstraction layer
- Detection result structures
- Post-processing utilities (NMS, filtering)
- Class label management

#### `gstreamed_common`
- GStreamer buffer utilities
- Format conversions
- Pipeline helpers

#### `ffmpeg_ort`
- Alternative FFmpeg-based pipeline
- Simpler architecture for quick prototyping
- Standalone binary

#### `into_rerun`
- Integration with Rerun visualization
- 3D visualization support
- Real-time monitoring

## 🔧 Development

### Building from Source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Build specific package
cargo build -p gstreamed_ort --release
```

### Running Tests

```bash
cargo test --workspace
```

### Code Formatting

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
```

## 📋 Current Capabilities

- ✅ Object detection (YOLOv8)
- ✅ Multi-object tracking (SORT)
- ✅ Video file processing
- ✅ Image processing
- ✅ Live stream display
- ✅ CUDA acceleration
- ✅ Attribute detection
- ✅ JSON output format

## 🚧 Limitations

- ❌ No segmentation support yet
- ❌ No pose estimation
- ❌ Live display slow on NVIDIA GPUs
- ⚠️ CUDA may fail silently - check logs
- ⚠️ Candle pipeline disabled by default (requires CUDA build)

## 🤝 Contributing

Contributions welcome! Areas of interest:
- Instance segmentation support
- Pose estimation
- Additional model formats
- Performance optimizations
- Documentation improvements

## 📄 License

Dual-licensed under:
- MIT License
- Apache License 2.0

Choose the license that best suits your needs.

## 🙏 Acknowledgments

- [GStreamer](https://gstreamer.freedesktop.org/) - Media framework
- [ONNX Runtime](https://onnxruntime.ai/) - ML inference
- [Ultralytics](https://ultralytics.com/) - YOLOv8 models
- [Hugging Face Candle](https://github.com/huggingface/candle) - ML framework
- [Similari](https://github.com/insight-platform/Similari) - Tracking algorithms

## 📞 Support

For issues, questions, or contributions, please open an issue on the project repository.

---

**Built with ❤️ in Rust**