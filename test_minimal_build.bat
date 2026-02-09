@echo off
REM Test script to verify the minimal build works correctly
echo Testing minimal build...

REM Build with no default features
echo Building with --no-default-features...
cargo build --release --no-default-features

if %ERRORLEVEL% EQU 0 (
    echo ✅ Build successful!
    
    REM Try to run the binary with help flag to verify it works
    echo Testing binary execution...
    target\release\twitch-translator.exe --help > nul 2>&1
    
    if %ERRORLEVEL% EQU 0 (
        echo ✅ Binary executes correctly!
        echo ✅ Minimal build test PASSED
    ) else (
        echo ❌ Binary execution failed
        echo ❌ Minimal build test FAILED
    )
) else (
    echo ❌ Build failed
    echo ❌ Minimal build test FAILED
)