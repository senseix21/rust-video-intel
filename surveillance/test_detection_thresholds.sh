#!/bin/bash
# Test detection with different thresholds

echo "=== Testing Detection Thresholds ==="
echo ""

echo "1. Default threshold (0.5) - Production recommended:"
./target/release/gstreamed_ort test_bus.jpg 2>&1 | grep -A 15 "Frame 0"
echo ""

echo "2. Low threshold (0.25) - Original behavior (shows false positives):"
./target/release/gstreamed_ort test_bus.jpg --conf-threshold 0.25 2>&1 | grep -A 20 "Frame 0"
echo ""

echo "3. High threshold (0.7) - Very strict:"
./target/release/gstreamed_ort test_bus.jpg --conf-threshold 0.7 2>&1 | grep -A 10 "Frame 0"

