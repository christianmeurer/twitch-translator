#!/bin/bash

# Create models directory if it doesn't exist
mkdir -p models

# Check if model already exists
if [ -f "models/ggml-base.en.bin" ]; then
    echo "Model already exists"
    exit 0
fi

echo "Downloading Whisper base English model..."
curl -L "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin" -o "models/ggml-base.en.bin"

if [ $? -eq 0 ]; then
    echo "Model downloaded successfully to models/ggml-base.en.bin"
else
    echo "Failed to download model"
    exit 1
fi