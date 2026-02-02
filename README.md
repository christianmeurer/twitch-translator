# Twitch Translator

A low-latency Twitch live translation system that captures audio from a Twitch stream, translates it to a specified language, and provides emotional text-to-speech output.

## Features

- **Audio Ingestion**: Captures audio from Twitch streams via HLS
- **Speech Recognition**: Real-time ASR using Whisper
- **Emotion Analysis**: Detects emotions from both prosody and text
- **Translation**: Translates speech to target language using DeepL
- **Emotional TTS**: Converts translated text to speech with emotional prosody using ElevenLabs
- **Low Latency**: Optimized pipeline for minimal delay
- **High Performance**: Written in Rust for maximum efficiency

## Requirements

- Rust (latest stable version)
- FFmpeg installed and available in PATH
- DeepL API key
- ElevenLabs API key

## Installation

```bash
git clone https://github.com/your-username/twitch-translator.git
cd twitch-translator
cargo build --release
```

## Usage

```bash
# Translate a Twitch channel
cargo run --release -- --channel <channel-name> --target-lang <language-code> --deepl-api-key <deepl-key> --elevenlabs-api-key <elevenlabs-key>

# Translate from a direct URL
cargo run --release -- --url <stream-url> --target-lang <language-code> --deepl-api-key <deepl-key> --elevenlabs-api-key <elevenlabs-key>
```

### Options

- `--channel <CHANNEL>`: Twitch channel name to translate
- `--url <URL>`: Direct stream URL to translate
- `--target-lang <TARGET_LANG>`: Target language for translation (default: pt-BR)
- `--deepl-api-key <DEEPL_API_KEY>`: DeepL API key for translation
- `--elevenlabs-api-key <ELEVENLABS_API_KEY>`: ElevenLabs API key for TTS
- `--latency-ms <LATENCY_MS>`: Target latency in milliseconds (default: 1500)
- `--twitch-client-id <TWITCH_CLIENT_ID>`: Twitch client ID (default: kimne78kx3ncx6brgo4mv6wki5h1ko)
- `--twitch-oauth-token <TWITCH_OAUTH_TOKEN>`: Twitch OAuth token for authentication
- `--hls-audio-only`: Only ingest audio from HLS stream
- `--log-level <LOG_LEVEL>`: Log level (default: info)

## Architecture

The system is built as a pipeline with the following components:

1. **Ingestor**: Captures audio from Twitch HLS streams
2. **Decoder**: Decodes audio segments to PCM
3. **ASR**: Transcribes speech to text using Whisper
4. **Translator**: Translates text to target language using DeepL
5. **TTS**: Converts translated text to speech with emotional prosody using ElevenLabs
6. **Playback**: Plays the synthesized audio

## Configuration

API keys can be provided via command line arguments or environment variables:

- `DEEPL_API_KEY`: DeepL API key
- `ELEVENLABS_API_KEY`: ElevenLabs API key
- `TWITCH_CLIENT_ID`: Twitch client ID
- `TWITCH_OAUTH_TOKEN`: Twitch OAuth token

## Performance Optimization

The system is designed for low latency with several optimization techniques:

- Asynchronous pipeline processing
- Efficient buffering strategies
- Minimal memory allocations
- Optimized audio processing

## License

This project is licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.