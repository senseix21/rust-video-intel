#!/usr/bin/env python3
"""
Export YOLO-NAS-S to ONNX format for Rust inference
"""

import torch
from super_gradients.training import models

def export_yolo_nas():
    print("Loading YOLO-NAS-S model...")
    model = models.get('yolo_nas_s', pretrained_weights='coco')

    print("Preparing model for export...")
    model.eval()
    model.prep_model_for_conversion(input_size=[1, 3, 640, 640])

    print("Exporting to ONNX...")
    model.export(
        'yolo_nas_s.onnx',
        confidence_threshold=0.5,
        nms_threshold=0.5,
        output_predictions_format='batch',
    )

    print("✅ ONNX model exported: yolo_nas_s.onnx")
    print("")
    print("Model info:")
    print("  - Input: [1, 3, 640, 640] (NCHW format)")
    print("  - Output: [1, num_detections, 85]")
    print("    - Columns 0-3: bbox (x_center, y_center, width, height)")
    print("    - Column 4: objectness score")
    print("    - Columns 5-84: class probabilities (80 COCO classes)")
    print("")
    print("For INT8 quantization, use:")
    print("  python3 scripts/quantize_int8.py")

if __name__ == '__main__':
    export_yolo_nas()
