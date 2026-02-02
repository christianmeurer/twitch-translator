@echo off
setlocal

:: Create models directory if it doesn't exist
if not exist "models" mkdir "models"

:: Check if model already exists
if exist "models\ggml-base.en.bin" (
    echo Model already exists
    exit /b 0
)

echo Downloading Whisper base English model...
powershell -Command "Invoke-WebRequest -Uri 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin' -OutFile 'models\ggml-base.en.bin'"

if %errorlevel% equ 0 (
    echo Model downloaded successfully to models\ggml-base.en.bin
) else (
    echo Failed to download model
    exit /b 1
)