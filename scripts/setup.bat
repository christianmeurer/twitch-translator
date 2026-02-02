@echo off
REM Setup script for Twitch Translator on Windows

echo Setting up Twitch Translator...

REM Check if Rust is installed
rustc --version >nul 2>&1
if %errorlevel% neq 0 (
    echo Rust is not installed. Please install Rust from https://www.rust-lang.org/
    pause
    exit /b 1
) else (
    echo Rust is already installed.
)

REM Check if FFmpeg is installed
ffmpeg -version >nul 2>&1
if %errorlevel% neq 0 (
    echo FFmpeg is not installed. Please install FFmpeg from https://ffmpeg.org/download.html
    pause
    exit /b 1
) else (
    echo FFmpeg is already installed.
)

REM Build the project
echo Building the project...
cargo build --release

echo Setup complete!
echo To run the application, use:
echo   cargo run --release -- --channel ^<channel^> --target-lang ^<language^> --deepl-api-key ^<key^> --elevenlabs-api-key ^<key^>
pause