#!/bin/bash

# Setup script for Twitch Translator

echo "Setting up Twitch Translator..."

# Check if Rust is installed
if ! command -v rustc &> /dev/null
then
    echo "Rust is not installed. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source $HOME/.cargo/env
else
    echo "Rust is already installed."
fi

# Check if FFmpeg is installed
if ! command -v ffmpeg &> /dev/null
then
    echo "FFmpeg is not installed. Please install FFmpeg manually:"
    echo "  - Ubuntu/Debian: sudo apt install ffmpeg"
    echo "  - macOS: brew install ffmpeg"
    echo "  - Windows: Download from https://ffmpeg.org/download.html"
else
    echo "FFmpeg is already installed."
fi

# Build the project
echo "Building the project..."
cargo build --release

echo "Setup complete!"
echo "To run the application, use:"
echo "  cargo run --release -- --channel <channel> --target-lang <language> --deepl-api-key <key> --elevenlabs-api-key <key>"