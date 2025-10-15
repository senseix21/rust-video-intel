#!/usr/bin/env python3
"""
ML Inference Server for People Detection
Uses YOLO-NAS for real-time people detection in retail surveillance
"""

import os
import sys
import json
import time
import logging
import argparse
import numpy as np
from typing import List, Dict, Tuple, Optional
from dataclasses import dataclass, asdict
from concurrent.futures import ThreadPoolExecutor
import base64
import io

# Web server
from flask import Flask, request, jsonify
from flask_cors import CORS

# Image processing
import cv2
from PIL import Image

# ML libraries
try:
    import torch
    import onnxruntime as ort
    from super_gradients.training import models
    from super_gradients.common.object_names import Models
except ImportError as e:
    print(f"Error importing ML libraries: {e}")
    print("Please install: pip install torch onnxruntime-gpu super-gradients opencv-python flask flask-cors pillow")
    sys.exit(1)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# Constants
CONFIDENCE_THRESHOLD = 0.5
NMS_THRESHOLD = 0.45
INPUT_WIDTH = 640
INPUT_HEIGHT = 640
PERSON_CLASS_ID = 0  # COCO class ID for person

@dataclass
class Detection:
    """Represents a single person detection"""
    x: float  # Normalized coordinates [0, 1]
    y: float
    width: float
    height: float
    confidence: float
    track_id: Optional[int] = None

    def to_dict(self):
        return asdict(self)

class YoloNASDetector:
    """YOLO-NAS based people detector"""

    def __init__(self, model_path: Optional[str] = None, use_gpu: bool = False):
        self.device = 'cuda' if use_gpu and torch.cuda.is_available() else 'cpu'
        logger.info(f"Using device: {self.device}")

        if model_path and os.path.exists(model_path):
            # Load ONNX model
            logger.info(f"Loading ONNX model from {model_path}")
            providers = ['CUDAExecutionProvider'] if use_gpu else ['CPUExecutionProvider']
            self.session = ort.InferenceSession(model_path, providers=providers)
            self.input_name = self.session.get_inputs()[0].name
            self.output_names = [o.name for o in self.session.get_outputs()]
            self.model = None
        else:
            # Load pretrained YOLO-NAS model
            logger.info("Loading pretrained YOLO-NAS-S model")
            self.model = models.get(Models.YOLO_NAS_S, pretrained_weights="coco")
            self.model = self.model.to(self.device)
            self.model.eval()
            self.session = None

    def preprocess_image(self, image: np.ndarray) -> np.ndarray:
        """Preprocess image for YOLO-NAS input"""
        # Resize to model input size
        resized = cv2.resize(image, (INPUT_WIDTH, INPUT_HEIGHT))

        # Convert BGR to RGB if needed
        if len(resized.shape) == 3 and resized.shape[2] == 3:
            resized = cv2.cvtColor(resized, cv2.COLOR_BGR2RGB)

        # Normalize to [0, 1]
        normalized = resized.astype(np.float32) / 255.0

        # Add batch dimension and transpose to NCHW format
        input_tensor = normalized.transpose(2, 0, 1)[np.newaxis, ...]

        return input_tensor

    def detect_people(self, image: np.ndarray) -> List[Detection]:
        """Detect people in the given image"""
        start_time = time.time()

        # Preprocess image
        input_tensor = self.preprocess_image(image)
        h_orig, w_orig = image.shape[:2]

        detections = []

        if self.session:
            # ONNX Runtime inference
            outputs = self.session.run(
                self.output_names,
                {self.input_name: input_tensor}
            )
            # Process ONNX outputs (format depends on model export)
            detections = self._process_onnx_outputs(outputs, w_orig, h_orig)

        elif self.model:
            # PyTorch inference
            with torch.no_grad():
                input_torch = torch.from_numpy(input_tensor).to(self.device)
                predictions = self.model.predict(input_torch, conf=CONFIDENCE_THRESHOLD)

                # Extract person detections
                for pred in predictions:
                    if hasattr(pred, 'prediction'):
                        boxes = pred.prediction.bboxes_xyxy
                        scores = pred.prediction.confidence
                        labels = pred.prediction.labels

                        for box, score, label in zip(boxes, scores, labels):
                            if label == PERSON_CLASS_ID and score >= CONFIDENCE_THRESHOLD:
                                # Convert to normalized coordinates
                                x1, y1, x2, y2 = box
                                det = Detection(
                                    x=float(x1) / w_orig,
                                    y=float(y1) / h_orig,
                                    width=float(x2 - x1) / w_orig,
                                    height=float(y2 - y1) / h_orig,
                                    confidence=float(score)
                                )
                                detections.append(det)

        # Apply NMS
        detections = self._apply_nms(detections)

        inference_time = (time.time() - start_time) * 1000
        logger.debug(f"Detected {len(detections)} people in {inference_time:.1f}ms")

        return detections

    def _process_onnx_outputs(self, outputs, w_orig, h_orig) -> List[Detection]:
        """Process ONNX model outputs"""
        detections = []

        # This depends on how the model was exported
        # Typical format: [batch, num_detections, 6] where 6 = [x1, y1, x2, y2, score, class]
        if len(outputs) > 0:
            output = outputs[0]
            if len(output.shape) == 3:
                for detection in output[0]:  # First batch
                    if len(detection) >= 6:
                        x1, y1, x2, y2, score, class_id = detection[:6]
                        if int(class_id) == PERSON_CLASS_ID and score >= CONFIDENCE_THRESHOLD:
                            det = Detection(
                                x=float(x1) / w_orig,
                                y=float(y1) / h_orig,
                                width=float(x2 - x1) / w_orig,
                                height=float(y2 - y1) / h_orig,
                                confidence=float(score)
                            )
                            detections.append(det)

        return detections

    def _apply_nms(self, detections: List[Detection]) -> List[Detection]:
        """Apply Non-Maximum Suppression to remove overlapping detections"""
        if len(detections) <= 1:
            return detections

        # Sort by confidence
        detections.sort(key=lambda d: d.confidence, reverse=True)

        keep = []
        for i, det1 in enumerate(detections):
            should_keep = True
            for det2 in keep:
                iou = self._calculate_iou(det1, det2)
                if iou > NMS_THRESHOLD:
                    should_keep = False
                    break
            if should_keep:
                keep.append(det1)

        return keep

    def _calculate_iou(self, det1: Detection, det2: Detection) -> float:
        """Calculate Intersection over Union between two detections"""
        x1 = max(det1.x, det2.x)
        y1 = max(det1.y, det2.y)
        x2 = min(det1.x + det1.width, det2.x + det2.width)
        y2 = min(det1.y + det1.height, det2.y + det2.height)

        if x2 <= x1 or y2 <= y1:
            return 0.0

        intersection = (x2 - x1) * (y2 - y1)
        area1 = det1.width * det1.height
        area2 = det2.width * det2.height
        union = area1 + area2 - intersection

        return intersection / union if union > 0 else 0.0

class InferenceServer:
    """HTTP server for ML inference"""

    def __init__(self, detector: YoloNASDetector, port: int = 8080):
        self.detector = detector
        self.port = port
        self.app = Flask(__name__)
        CORS(self.app)
        self.setup_routes()

        # Metrics
        self.total_requests = 0
        self.total_detections = 0
        self.total_inference_time = 0

    def setup_routes(self):
        """Setup Flask routes"""

        @self.app.route('/health', methods=['GET'])
        def health():
            return jsonify({
                'status': 'healthy',
                'model': 'YOLO-NAS',
                'device': self.detector.device if hasattr(self.detector, 'device') else 'cpu'
            })

        @self.app.route('/detect', methods=['POST'])
        def detect():
            """Detect people in uploaded image"""
            try:
                # Get image from request
                if 'image' in request.files:
                    # File upload
                    file = request.files['image']
                    image = Image.open(file.stream)
                    image_np = np.array(image)

                elif 'image_base64' in request.json:
                    # Base64 encoded image
                    image_data = base64.b64decode(request.json['image_base64'])
                    image = Image.open(io.BytesIO(image_data))
                    image_np = np.array(image)

                elif 'image_bytes' in request.data:
                    # Raw bytes
                    image_np = np.frombuffer(request.data, dtype=np.uint8)
                    height = request.args.get('height', type=int)
                    width = request.args.get('width', type=int)
                    channels = request.args.get('channels', default=3, type=int)
                    image_np = image_np.reshape((height, width, channels))

                else:
                    return jsonify({'error': 'No image provided'}), 400

                # Run detection
                detections = self.detector.detect_people(image_np)

                # Update metrics
                self.total_requests += 1
                self.total_detections += len(detections)

                return jsonify({
                    'detections': [d.to_dict() for d in detections],
                    'count': len(detections),
                    'image_size': image_np.shape[:2]
                })

            except Exception as e:
                logger.error(f"Detection error: {e}")
                return jsonify({'error': str(e)}), 500

        @self.app.route('/detect_batch', methods=['POST'])
        def detect_batch():
            """Detect people in multiple images"""
            try:
                images_data = request.json.get('images', [])
                results = []

                for img_data in images_data:
                    image_bytes = base64.b64decode(img_data['base64'])
                    image = Image.open(io.BytesIO(image_bytes))
                    image_np = np.array(image)

                    detections = self.detector.detect_people(image_np)
                    results.append({
                        'id': img_data.get('id'),
                        'detections': [d.to_dict() for d in detections],
                        'count': len(detections)
                    })

                return jsonify({'results': results})

            except Exception as e:
                logger.error(f"Batch detection error: {e}")
                return jsonify({'error': str(e)}), 500

        @self.app.route('/metrics', methods=['GET'])
        def metrics():
            """Get server metrics"""
            avg_inference = self.total_inference_time / max(1, self.total_requests)
            avg_detections = self.total_detections / max(1, self.total_requests)

            return jsonify({
                'total_requests': self.total_requests,
                'total_detections': self.total_detections,
                'avg_inference_time_ms': avg_inference,
                'avg_detections_per_image': avg_detections
            })

    def run(self):
        """Start the inference server"""
        logger.info(f"Starting ML inference server on port {self.port}")
        self.app.run(host='0.0.0.0', port=self.port, threaded=True)

def export_model_to_onnx(output_path: str = "yolo_nas_s_people.onnx"):
    """Export YOLO-NAS model to ONNX format"""
    logger.info("Exporting YOLO-NAS to ONNX...")

    model = models.get(Models.YOLO_NAS_S, pretrained_weights="coco")
    model.eval()

    # Create dummy input
    dummy_input = torch.randn(1, 3, INPUT_HEIGHT, INPUT_WIDTH)

    # Export to ONNX
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        export_params=True,
        opset_version=11,
        input_names=['input'],
        output_names=['output'],
        dynamic_axes={
            'input': {0: 'batch_size'},
            'output': {0: 'batch_size'}
        }
    )

    logger.info(f"Model exported to {output_path}")
    return output_path

def main():
    parser = argparse.ArgumentParser(description='ML Inference Server for People Detection')
    parser.add_argument('--port', type=int, default=8080, help='Server port')
    parser.add_argument('--model', type=str, help='Path to ONNX model file')
    parser.add_argument('--export', action='store_true', help='Export model to ONNX')
    parser.add_argument('--gpu', action='store_true', help='Use GPU if available')
    parser.add_argument('--debug', action='store_true', help='Enable debug logging')

    args = parser.parse_args()

    if args.debug:
        logging.getLogger().setLevel(logging.DEBUG)

    if args.export:
        model_path = export_model_to_onnx()
        if not args.model:
            args.model = model_path

    # Initialize detector
    detector = YoloNASDetector(model_path=args.model, use_gpu=args.gpu)

    # Start server
    server = InferenceServer(detector, port=args.port)
    server.run()

if __name__ == '__main__':
    main()