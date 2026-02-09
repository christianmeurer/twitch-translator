# Installation Guide

## Prerequisites

Before building Twitch Translator, you need to install the following dependencies:

### Required Dependencies

1. **Rust** - Install using rustup:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **FFmpeg** - Required for audio processing:
   - **Ubuntu/Debian**: `sudo apt install ffmpeg`
   - **macOS**: `brew install ffmpeg`
   - **Windows**: Download from https://ffmpeg.org/download.html

### Optional Dependencies (for Whisper ASR support)

To enable Whisper ASR functionality, you need to install additional dependencies:

#### Windows

1. **Install Visual Studio Build Tools** with C++ support
2. **Install LLVM** which includes libclang:
   - Download from https://github.com/llvm/llvm-project/releases
   - Or install via chocolatey: `choco install llvm`
3. **Set environment variables**:
   ```cmd
   set LIBCLANG_PATH=C:\Program Files\LLVM\bin
   ```

#### macOS

1. **Install Xcode Command Line Tools**:
   ```bash
   xcode-select --install
   ```

2. **Install LLVM via Homebrew**:
   ```bash
   brew install llvm
   ```

3. **Set environment variables**:
   ```bash
   export LIBCLANG_PATH="$(brew --prefix)/opt/llvm/lib"
   ```

#### Ubuntu/Debian

1. **Install build dependencies**:
   ```bash
   sudo apt update
   sudo apt install build-essential cmake libclang-dev
   ```

## Building the Project

### Without Whisper ASR (minimal build) - RECOMMENDED

```bash
cargo build --release --no-default-features
```

This is the **recommended approach** for most users, especially on Windows. This build will work for all components except ASR (speech recognition). You can still test the emotion analysis, translation, and TTS components.

### With Whisper ASR (Advanced Users Only)

```bash
# On Unix-like systems (Linux/macOS):
LIBCLANG_PATH=/path/to/llvm/lib cargo build --release

# On Windows:
set LIBCLANG_PATH=C:\path\to\llvm\bin
cargo build --release
```

Or you can set the environment variable permanently and then build:

```bash
cargo build --release
```

**Important Notes**:
1. The Whisper ASR feature has known compatibility issues, especially on Windows
2. Building with ASR requires additional complex dependencies (LLVM/Clang)
3. If you encounter build errors with the default build, use the `--no-default-features` flag
4. The minimal build provides access to all core functionality except speech recognition

## Downloading Whisper Models

To use Whisper ASR, you need to download a model:

```bash
# Run the download script
./scripts/download-model.sh
```

This will download the base English model to the `models/` directory.

## Running the Application

After building, you can run the application:

```bash
cargo run --release -- --channel <channel> --target-lang <language> --deepl-api-key <key> --elevenlabs-api-key <key>
```

Example:
```bash
cargo run --release -- --channel svinin_ --target-lang en --deepl-api-key YOUR_DEEPL_KEY --elevenlabs-api-key YOUR_ELEVENLABS_KEY
```

## Configuration

You can also use environment variables for API keys:

```bash
export DEEPL_API_KEY=your_deepl_api_key
export ELEVENLABS_API_KEY=your_elevenlabs_api_key
export TWITCH_CLIENT_ID=your_twitch_client_id
export TWITCH_OAUTH_TOKEN=your_twitch_oauth_token
```

Then run without specifying keys in the command line:
```bash
cargo run --release -- --channel svinin_ --target-lang en
```

## Troubleshooting

### Common Issues

1. **libclang not found**: Make sure you've installed LLVM and set the LIBCLANG_PATH environment variable correctly.

2. **FFmpeg not found**: Ensure FFmpeg is installed and available in your PATH.

3. **Model not found**: Run the download-model script to get the required Whisper model.

4. **Twitch API errors**: Verify your Twitch client ID and OAuth token are correct.

### Getting Help

If you encounter issues, check the logs by setting the log level:
```bash
RUST_LOG=debug cargo run --release -- --channel svinin_ --target-lang en