# Retail Surveillance System - Phase 1

People counting from video feeds using YOLO-NAS and Rust.

## Features

- RTSP stream capture from IP cameras
- YOLO-NAS-S people detection
- Real-time inference with ONNX Runtime
- GStreamer video pipeline
- Test mode with synthetic video

## Prerequisites

### macOS
```bash
brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly
brew install pkg-config
```

### Ubuntu/Debian
```bash
sudo apt-get install -y \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-good1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    pkg-config
```

### Python (for YOLO-NAS export)
```bash
pip install super-gradients onnx torch
```

## Quick Start

### 1. Export YOLO-NAS to ONNX
```bash
python3 scripts/export_yolo_nas.py
```

This downloads the pretrained YOLO-NAS-S model and exports it to `yolo_nas_s.onnx` (~50MB).

### 2. Build the project
```bash
cargo build --release
```

### 3. Run with test pattern (no camera required)
```bash
cargo run --release -- yolo_nas_s.onnx
```

This uses GStreamer's `videotestsrc` to generate synthetic video for testing.

### 4. Run with RTSP camera
```bash
cargo run --release -- yolo_nas_s.onnx rtsp://admin:password@192.168.1.100:554/stream
```

Replace with your camera's RTSP URL.

## Expected Output

```
INFO retail_surveillance: Retail Surveillance System - Phase 1 Demo
INFO retail_surveillance: =========================================
INFO retail_surveillance: Loading ONNX model from: yolo_nas_s.onnx
INFO retail_surveillance: Model loaded successfully
INFO retail_surveillance: Creating test pipeline
INFO retail_surveillance: Pipeline started, processing frames...
INFO retail_surveillance: Frame processed in 45ms (22.2 FPS) - People detected: 0
INFO retail_surveillance: Frame processed in 43ms (23.3 FPS) - People detected: 0
```

## Performance Targets

- **CPU (Apple M1/M2):** 15-25 FPS
- **CPU (AMD Ryzen 5):** 10-15 FPS
- **GPU (with CUDA/ROCm):** 60+ FPS (future)

## Architecture

```
RTSP Camera
    │
    ▼
GStreamer Pipeline
    │ (H.264 decode)
    │ (videoconvert to RGB)
    │ (videoscale to 640x640)
    ▼
appsink
    │
    ▼
PeopleCounter
    │ (preprocess: resize, normalize)
    │ (ONNX Runtime inference)
    │ (postprocess: NMS, filter person class)
    ▼
Detections
    │
    ▼
Console output
```

## Next Steps

1. **Add ByteTrack tracking** - Track people across frames
2. **Add MQTT integration** - Receive POS events
3. **Add PostgreSQL storage** - Store detections and correlations
4. **Add multi-camera support** - Process multiple streams
5. **Add web API** - Expose metrics via REST

## Troubleshooting

### "Model file not found"
Run `python3 scripts/export_yolo_nas.py` first.

### "GStreamer plugin not found"
Install all GStreamer plugins (see Prerequisites).

### "Cannot connect to RTSP"
- Check camera IP and credentials
- Test with VLC: `vlc rtsp://admin:password@192.168.1.100:554/stream`
- Check firewall settings

### Low FPS
- Use `--release` build (10x faster than debug)
- Reduce video resolution in pipeline
- Consider INT8 quantization (future)

## License

MIT
