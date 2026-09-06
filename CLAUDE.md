# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build Commands
- `cargo build` - Build the project in debug mode
- `cargo build --release` - Build optimized release binary
- `cargo run` - Run the MCP server (default command)
- `cargo run -- setup` - Run the interactive setup process

### Testing Commands
- `cargo test` - Run all unit and integration tests (integration tests spawn the server binary)
- `cargo clippy --all-targets -- -D warnings` - Check code quality (must pass for CI)
- `cargo fmt` - Format code (required before PR)

DSP behaviour is verified by rendering to sample buffers and measuring
(Goertzel power, RMS, zero-crossing rate); see `src/expressive/test_util.rs`.
Prefer that over listen-by-ear checks when changing synthesis or effects.

### Listen-by-ear demos (`src/demos.rs`, play audio locally)
- `cargo run -- test-presets` - Classic synthesizer presets
- `cargo run -- test-drums` - Drum synthesis and drum presets
- `cargo run -- test-pads`, `test-volumes`, `test-effects`, `debug-dx7`

### Logging and Debugging
- **Log Location** (cross-platform, daily rotated, pruned after 7 days):
  - **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log.YYYY-MM-DD`
  - **Linux**: `~/.local/share/mcp-muse/mcp-muse.log.YYYY-MM-DD`
  - **Windows**: `%APPDATA%/mcp-muse/mcp-muse.log.YYYY-MM-DD`
- **Log Level**: `MCP_MUSE_LOG` env var (EnvFilter syntax, e.g. `debug` or `mcp_muse::midi=trace`); default `info`. Never log inside per-sample audio loops.

## Architecture Overview

### MCP Server (`src/server/mcp.rs`)
JSON-RPC 2.0 over stdio. `ServerState` holds the audio player (opened on
first playback) and the session's patterns. Six tools:
- `play_notes` - quick sounds and melodies; every note type in one array; takes `mode: replace|layer`
- `define_sequence_pattern` / `play_sequence` / `list_patterns` - reusable bar-based patterns with transposition, repeats and time signature; `play_sequence` also takes `mode`
- `list_sounds` - catalog of presets, GM instruments, drum keys, synthesis types, R2D2 emotions and effects
- `stop_playback` - silence everything currently playing

Conventions: malformed or invalid arguments return JSON-RPC `-32602`;
anything that fails while executing (unknown preset, missing pattern, no
audio device) returns a result with `isError: true` so the model can read
it. Requests without an id are notifications and get no response.

### Audio pipeline (`src/midi/engine.rs`, `translate.rs`, `player.rs`)
One `MidiEngine` per process runs as a never-ending rodio source on the
output mixer: a single OxiSynth (SoundFont loaded once, polyphony 64), a
min-heap of MIDI events keyed to the engine's 44.1 kHz sample clock, the
pre-rendered R2D2/synthesis buffers, and the MIDI bus `EffectsChain` (one
per side). `MidiPlayer::play(sequence, mode)` translates and sends a
`PlayCommand`; it returns the duration including effect tails.

1. `Translator` applies presets (a missing preset is an error) and converts musical time with the sequence's tempo and `beats_per_bar`.
2. R2D2 and synthesis notes are pre-rendered on the tool thread *with their own effects* and scheduled as mono buffers (`SYNTH_BUS_GAIN` applied).
3. MIDI notes become time-ordered events: per call, the first note on a channel sends a program change (its instrument, or 0), controllers only when specified. Channel 9 is OxiSynth's drum channel; no bank select is needed.
4. The engine drains commands per 1024-frame chunk and applies events at their exact frame (`LEAD_FRAMES` = 2048 after the command). It sums the buses, soft-clips, and emits stereo.
5. `mode: replace` (default) fades 6 ms, sends SystemReset, clears the queue and installs the call's bus chain; `layer` mixes on top. `stop_playback` is the same reset with nothing scheduled.

Known limitation: OxiSynth renders all 16 MIDI channels into one bus, so
per-channel effects are not yet possible (pan, volume, reverb/chorus CCs do
work per channel inside OxiSynth). A dedicated render thread behind the same
engine API is the next step if the callback still glitches.

### Synthesis (`src/expressive/`)
- `synth.rs` - `ExpressiveSynth`: R2D2 ring-modulation voice and the general synthesizer. Pipeline per note: oscillator → ADSR → `Svf` filter → `EffectsChain` → amplitude. Swept oscillators use `PhaseAccumulator` (never `sin(2π·f(t)·t)`). Includes PolyBLEP saw/square, DX7 operator routing for algorithms 1/2, 5/6, 16/17, 32.
- `percussion.rs` - kick, snare, hi-hat, cymbal, zap, swoosh; these carry their own envelopes so the ADSR is skipped.
- `effects.rs` - stateful effects: Schroeder reverb, damped feedback delay, 3-voice chorus, TPT state-variable filter, compressor, tanh distortion. `EffectsChain::new(sample_rate, &[EffectConfig])` then `process` per sample or `process_buffer`.
- `effects_presets.rs` - named chains ("studio", "concert_hall", ...).
- `presets/` - 29 classic synth presets in six categories (bass 10, pad 10, drums 5, effects 2, lead 1, keys 1). Preset effects plus `signature_effects` merge into the note's effect chain unless the caller supplies `effects` explicitly.
- `r2d2.rs` - emotion parameter tables (pitch contours, ranges).

### Data model (`src/midi/mod.rs`)
`SimpleNote` is one flat struct covering MIDI, R2D2, synthesis, preset and
effects fields (use `..Default::default()`). `SimpleSequence` carries
`tempo` and `beats_per_bar`. `MusicalDuration` is a number (bars) or a
note-value string. `SequencePattern::quantize_notes` applies
`quantize_grid` when a pattern is defined. `gm_names.rs` has the GM
program and drum-key names used by `list_sounds`.

### Setup (`src/setup/`)
Downloads FluidR3_GM.sf2 (verified against a pinned SHA-256), writes the
Cursor MCP config, stores an optional custom SoundFont path.

## Important Architectural Decisions
- **Unified playback**: every audio type goes through `MidiPlayer::play` and the one `MidiEngine`.
- **Effects are stateful and per note** for synthesis/R2D2, per bus for MIDI. Do not reintroduce per-sample allocation or effect-count caps; the old "max 3 effects" and "2x gain compensation" rules were workarounds for stateless effects and are gone.
- **Stereo throughout** the mixer; mono sources are centered.
- **One engine per process**: `ServerState` owns one `MidiPlayer`, which owns the stream and the single `MidiEngine`; never create a synthesizer per call.
