# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build Commands
- `cargo build` - Build the project in debug mode
- `cargo build --release` - Build optimized release binary
- `cargo run` - Run the MCP server (default command)
- `cargo run -- --setup` - Run the interactive setup process

### Testing Commands
- `cargo test` - Run all unit and integration tests
- `cargo clippy -- -D warnings` - Check code quality (must pass for CI)
- `cargo fmt` - Format code (required before PR)
- `cargo run -- test-presets` - Test classic synthesizer preset integration
- `cargo run -- test-polyphony` - Test polyphonic voice management
- `cargo run -- test-drums` - Test drum synthesis and preset functionality
- `cargo run -- debug-dx7` - Debug DX7 synthesis issues

### Development Utilities
- `cargo run -- server` - Explicitly start MCP server
- `./target/release/mcp-muse --setup` - Production setup command

### Logging and Debugging
- **Log Location** (cross-platform):
  - **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log`
  - **Linux**: `~/.local/share/mcp-muse/mcp-muse.log`
  - **Windows**: `%APPDATA%/mcp-muse/mcp-muse.log`
- **View Logs**: `tail -f ~/Library/Application\ Support/mcp-muse/mcp-muse.log` (macOS)
- **When Cursor runs the server**: Logs are captured by Cursor and may not appear in the file
- **Log Level**: TRACE (all events logged, see src/main.rs:50)

## Architecture Overview

### Core Components

**MCP Server (src/server/)**
- `mcp.rs` - Complete MCP protocol implementation with three tools:
  - `play_midi` - Legacy base64 MIDI support
  - `play_notes` - JSON-based music creation (primary interface)
  - `play_r2d2_expression` - Robotic emotional expressions

**Audio Engines**
- **OxiSynth Engine** - Professional SoundFont synthesis for SNES-style gaming sounds
- **ExpressiveSynth Engine** - R2D2-style robotic vocalizations with 9 emotions
- **HybridAudioSource** - Real-time mixing of both engines

**MIDI System (src/midi/)**
- `player.rs` - Unified audio playback with MidiPlayer class:
  - `play_enhanced_mixed()` - Universal playback method supporting ALL audio types (MIDI, synthesis, R2D2, presets, effects)
  - Per-channel effects processing with intelligent limiting (max 3 effects per channel)
  - Automatic gain compensation for heavily processed signals
- `parser.rs` - MIDI file parsing and timing conversion

**Synthesis System (src/expressive/)**
- `synth.rs` - PolyphonicVoiceManager for real-time voice allocation
- `r2d2.rs` - Ring modulation synthesis with emotion-specific parameters
- `presets/` - 31 classic synthesizer presets (Minimoog, TB-303, Jupiter-8, TR-808, TR-909, etc.)
- `fundsp_effects.rs` - Professional audio effects processor:
  - Reverb (Schroeder algorithm), Delay, Chorus, Filter, Compressor, Distortion
  - Per-channel processing with automatic limiting
  - Effects presets for common scenarios

**Setup System (src/setup/)**
- Automatic FluidR3_GM SoundFont download (142MB)
- MCP host configuration (Cursor integration)
- Cross-platform data directory management

### Audio Capabilities

**165+ Sound Options:**
- 128 GM instruments (FluidR3_GM SoundFont)
- 9 R2D2 emotional expressions
- 31 classic synthesizer presets (including 5 drum machine presets)
- 19 custom synthesis types

**Key Features:**
- Mixed mode sequences (all audio systems work together)
- Real-time polyphonic voice management (32+ voices)
- Professional audio quality (44.1kHz stereo synthesis)
- Zero-latency performance for AI conversations

### Data Structures

**SimpleNote (src/midi/mod.rs)** - Universal note representation supporting:
- MIDI parameters (note, velocity, channel, instrument)
- R2D2 parameters (emotion, intensity, complexity, pitch_range)
- Synthesis parameters (synth_type, frequency, envelope, effects)
- Classic preset parameters (preset_name, preset_category, preset_variation)

**SimpleSequence** - Collection of SimpleNote objects with tempo control

### Development Notes

**Audio Architecture:**
- OxiSynth for SoundFont-based MIDI synthesis (FluidR3_GM.sf2)
- ExpressiveSynth for R2D2 emotional vocalizations
- rodio-based audio pipeline with proper buffering
- Real-time mixing with sample-accurate timing
- Per-channel effects processing (16 MIDI channels + R2D2 + synthesis)
- Intelligent effects limiting to prevent signal destruction
- Automatic gain compensation for heavily attenuated signals

**Polyphony Management:**
- Voice allocation with intelligent voice stealing
- Real-time envelope processing
- Support for 32+ simultaneous voices
- Dynamic parameter modulation

**Testing Infrastructure:**
- Comprehensive polyphony validation tests
- Classic preset integration testing
- Mixed mode audio system validation
- Real-time audio quality verification

**Setup System:**
- Interactive SoundFont configuration
- Automatic MCP host integration
- Cross-platform data directory handling
- Robust error recovery with manual fallbacks

**Important Architectural Decisions:**
- **Unified Playback**: All audio types use `play_enhanced_mixed()` - no separate methods needed
- **Effects Limiting**: Maximum 3 effects per channel to prevent signal destruction (>3 effects can cause inaudible output)
- **Gain Compensation**: Automatic 2x gain boost when effects attenuate signal below 10% of original
- **Channel Routing**: Currently all MIDI routes to channel 0 (TODO: implement per-channel MIDI separation)
- **Effects Collection**: Effects are collected per audio type (MIDI, R2D2, synthesis) not per individual note

The system is production-ready with professional audio quality, supporting both nostalgic SNES gaming sounds and expressive R2D2 robotic vocalizations for AI conversation enhancement.