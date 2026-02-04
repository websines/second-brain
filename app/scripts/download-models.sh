#!/bin/bash
# Download Sherpa-ONNX WASM files, ASR model, and Silero VAD model
# Run this script once to set up transcription support

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STATIC_DIR="$SCRIPT_DIR/../static"
MODELS_DIR="$STATIC_DIR/models"
WASM_DIR="$STATIC_DIR"

echo "Creating directories..."
mkdir -p "$MODELS_DIR"

# Sherpa-ONNX version
SHERPA_VERSION="1.10.30"

echo ""
echo "=== Downloading Sherpa-ONNX WASM files ==="
# Download pre-built WASM files from GitHub releases
WASM_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}"

# Main WASM files
curl -L "$WASM_URL/sherpa-onnx-wasm-simd-asr.tar.bz2" -o /tmp/sherpa-wasm.tar.bz2
tar -xjf /tmp/sherpa-wasm.tar.bz2 -C /tmp/
cp /tmp/sherpa-onnx-wasm-simd-asr/*.js "$WASM_DIR/"
cp /tmp/sherpa-onnx-wasm-simd-asr/*.wasm "$WASM_DIR/"
rm -rf /tmp/sherpa-wasm.tar.bz2 /tmp/sherpa-onnx-wasm-simd-asr

echo ""
echo "=== Downloading streaming ASR model (20M Zipformer) ==="
# Download the small streaming model (~20MB)
MODEL_NAME="sherpa-onnx-streaming-zipformer-en-20M-2023-02-17"
MODEL_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/${MODEL_NAME}.tar.bz2"

curl -L "$MODEL_URL" -o /tmp/model.tar.bz2
tar -xjf /tmp/model.tar.bz2 -C /tmp/
cp /tmp/${MODEL_NAME}/*.onnx "$MODELS_DIR/"
cp /tmp/${MODEL_NAME}/tokens.txt "$MODELS_DIR/"
rm -rf /tmp/model.tar.bz2 /tmp/${MODEL_NAME}

echo ""
echo "=== Downloading Silero VAD model ==="
# Download Silero VAD v5 ONNX model (~2MB)
VAD_URL="https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx"
curl -L "$VAD_URL" -o "$MODELS_DIR/silero_vad.onnx"

echo ""
echo "=== Downloading ONNX Runtime Web ==="
# Download ONNX Runtime WASM files
ORT_VERSION="1.17.0"
ORT_URL="https://cdn.jsdelivr.net/npm/onnxruntime-web@${ORT_VERSION}/dist"
curl -L "$ORT_URL/ort-wasm.wasm" -o "$WASM_DIR/ort-wasm.wasm"
curl -L "$ORT_URL/ort-wasm-simd.wasm" -o "$WASM_DIR/ort-wasm-simd.wasm"
curl -L "$ORT_URL/ort-wasm-simd-threaded.wasm" -o "$WASM_DIR/ort-wasm-simd-threaded.wasm"

echo ""
echo "=== Done! ==="
echo ""
echo "Files downloaded to:"
echo "  WASM: $WASM_DIR"
echo "  Models: $MODELS_DIR"
echo ""
echo "Model files:"
ls -lah "$MODELS_DIR"
echo ""
echo "WASM files:"
ls -lah "$WASM_DIR"/*.wasm 2>/dev/null || echo "  No .wasm files yet"
ls -lah "$WASM_DIR"/*.js 2>/dev/null | head -5 || echo "  No .js files yet"
