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
- `cargo run -- test-presets` - Test classic synthesizer preset integration
- `cargo run -- test-polyphony` - Test polyphonic voice management
- `cargo run -- debug-dx7` - Debug DX7 synthesis issues

### Development Utilities
- `cargo run -- server` - Explicitly start MCP server
- `./target/release/mcp-muse --setup` - Production setup command

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
- `player.rs` - Main audio playback with MidiPlayer class supporting:
  - `play_polyphonic()` - Real-time polyphonic voice management
  - `play_enhanced_mixed()` - Hybrid MIDI + synthesis playback
  - `play_mixed()` - Mixed mode combining MIDI and R2D2 expressions
- `polyphonic_source.rs` - Advanced polyphony with voice stealing
- `parser.rs` - MIDI file parsing and timing conversion

**Synthesis System (src/expressive/)**
- `synth.rs` - PolyphonicVoiceManager for real-time voice allocation
- `r2d2.rs` - Ring modulation synthesis with emotion-specific parameters
- `presets/` - 26 classic synthesizer presets (Minimoog, TB-303, Jupiter-8, etc.)

**Setup System (src/setup/)**
- Automatic FluidR3_GM SoundFont download (142MB)
- MCP host configuration (Cursor integration)
- Cross-platform data directory management

### Audio Capabilities

**160+ Sound Options:**
- 128 GM instruments (FluidR3_GM SoundFont)
- 9 R2D2 emotional expressions
- 26 classic synthesizer presets
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
- OxiSynth for SoundFont-based MIDI synthesis
- ExpressiveSynth for R2D2 emotional vocalizations
- rodio-based audio pipeline with proper buffering
- Real-time mixing with sample-accurate timing

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

The system is production-ready with professional audio quality, supporting both nostalgic SNES gaming sounds and expressive R2D2 robotic vocalizations for AI conversation enhancement.