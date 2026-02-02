# Twitch Translator

A low-latency Twitch live translation system (ASR -> Translate -> TTS) written in Rust.

## Features

- **Low-latency audio ingestion** from Twitch HLS streams
- **Streaming ASR** (Automatic Speech Recognition) - currently using a placeholder implementation
- **Emotion analysis** - currently using a placeholder implementation
- **Translation** - currently using a placeholder implementation
- **Emotional TTS** (Text-to-Speech) - currently using a placeholder implementation
- **Pipeline architecture** for efficient processing
- **CLI interface** for easy usage

## Architecture

The system is composed of several components:

1. **Ingestor**: Fetches audio segments from Twitch HLS streams
2. **Decoder**: Converts audio segments to PCM format
3. **ASR**: Transcribes audio to text (placeholder implementation)
4. **Emotion Analyzer**: Analyzes emotion from prosody and text (placeholder implementation)
5. **Translator**: Translates text to target language (placeholder implementation)
6. **TTS**: Synthesizes translated text to speech with emotional prosody (placeholder implementation)
7. **Playback**: Plays synthesized audio (placeholder implementation)
8. **Pipeline**: Orchestrates all components

## Installation

1. Install Rust: https://www.rust-lang.org/tools/install
2. Clone the repository:
   ```
   git clone https://github.com/your-username/twitch-translator.git
   cd twitch-translator
   ```
3. Build the project:
   ```
   cargo build --release
   ```

## Usage

```
cargo run --release -- --channel <channel-name> --target-lang <language-code>
```

Example:
```
cargo run --release -- --channel twitch_channel --target-lang es
```

## Configuration

The application can be configured using command-line arguments or environment variables:

- `--channel`: Twitch channel name
- `--target-lang`: Target language for translation (default: pt-BR)
- `--latency-ms`: Target latency in milliseconds (default: 1500)
- `--twitch-client-id`: Twitch client ID (can also be set via TWITCH_CLIENT_ID environment variable)
- `--twitch-oauth-token`: Twitch OAuth token (can also be set via TWITCH_OAUTH_TOKEN environment variable)
- `--deepl-api-key`: DeepL API key for translation (can also be set via DEEPL_API_KEY environment variable)
- `--elevenlabs-api-key`: ElevenLabs API key for TTS (can also be set via ELEVENLABS_API_KEY environment variable)

## Limitations

This is a proof-of-concept implementation with several limitations:

1. **ASR**: The ASR component is not implemented due to dependency issues with `mutter` crate
2. **Emotion Analysis**: The emotion analysis component is not implemented
3. **Translation**: The translation component is not implemented
4. **TTS**: The TTS component is not implemented
5. **Playback**: The playback component is not implemented

## Future Work

1. Implement a proper ASR backend (e.g., using Whisper.cpp or similar)
2. Implement emotion analysis
3. Integrate with a translation service (e.g., DeepL, Google Translate)
4. Integrate with an emotional TTS service (e.g., ElevenLabs)
5. Implement low-latency audio playback
6. Optimize performance for real-time processing
7. Add support for more input sources
8. Add support for more output languages

## License

This project is licensed under either of:

- Apache License, Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0)
- MIT license (http://opensource.org/licenses/MIT)

at your option.