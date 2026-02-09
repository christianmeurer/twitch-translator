#!/bin/bash

# Test script to verify the minimal build works correctly
echo "Testing minimal build..."

# Build with no default features
echo "Building with --no-default-features..."
cargo build --release --no-default-features

if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    
    # Try to run the binary with help flag to verify it works
    echo "Testing binary execution..."
    ./target/release/twitch-translator --help > /dev/null 2>&1
    
    if [ $? -eq 0 ]; then
        echo "✅ Binary executes correctly!"
        echo "✅ Minimal build test PASSED"
    else
        echo "❌ Binary execution failed"
        echo "❌ Minimal build test FAILED"
    fi
else
    echo "❌ Build failed"
    echo "❌ Minimal build test FAILED"
fi