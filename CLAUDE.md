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
- `cargo clippy --all-targets --all-features -- -D warnings` - Check code quality (must pass for CI)
- CI runs clippy on the latest stable toolchain (`dtolnay/rust-toolchain@stable`), not a pinned one. Run `rustup update stable` before clippy so local lints match CI; a stale local toolchain passes locally and fails CI on newer lints.
- `cargo fmt` - Format code (required before PR)

DSP behaviour is verified by rendering to sample buffers and measuring
(Goertzel power, RMS, zero-crossing rate); see `src/expressive/test_util.rs`.
Prefer that over listen-by-ear checks when changing synthesis or effects.

### Listen-by-ear demos (`src/demos.rs`, play audio locally)
- `cargo run -- test-synths` - Every built-in synth patch, by category
- `cargo run -- test-drums` - A bar of the 808/909 percussion patches
- `cargo run -- test-effects` - A MIDI piano chord dry, then with reverb, then delay

### Logging and Debugging
- **Log Location** (cross-platform, daily rotated, pruned after 7 days):
  - **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log.YYYY-MM-DD`
  - **Linux**: `~/.local/share/mcp-muse/mcp-muse.log.YYYY-MM-DD`
  - **Windows**: `%APPDATA%/mcp-muse/mcp-muse.log.YYYY-MM-DD`
- **Log Level**: `MCP_MUSE_LOG` env var (EnvFilter syntax, e.g. `debug` or `mcp_muse::midi=trace`); default `info`. Never log inside per-sample audio loops.

## Architecture Overview

### MCP Server (`src/server/mcp.rs`)
JSON-RPC 2.0 over stdio. `ServerState` holds the audio player (opened on
first playback) and the session's patterns and synth patches. Eight tools:
- `play_notes` - quick sounds and melodies; every note type in one array; takes `mode: replace|layer`
- `define_sequence_pattern` / `play_sequence` / `list_patterns` - reusable bar-based patterns with transposition, repeats and time signature; `play_sequence` also takes `mode`
- `define_synth` - store a validated synth patch for the session; notes reference it by name via `synth`
- `list_sounds` - catalog of synth patches, GM instruments, drum keys, R2D2 emotions and effects
- `stop_playback` - silence everything currently playing
- `export_audio` - render a composition offline and write WAV files: `split: mixdown|stems|tracks`, a required absolute `path`, `name`, `bit_depth: 16|24|32` and `overwrite`; never opens the audio device

Conventions: malformed or invalid arguments (including a patch that fails
validation) return JSON-RPC `-32602`; anything that fails while executing
(unknown synth or pattern name, no audio device) returns a result with
`isError: true` so the model can read it. Requests without an id are
notifications and get no response.

### Audio pipeline (`src/midi/engine.rs`, `translate.rs`, `player.rs`)
One `MidiEngine` per process runs as a never-ending rodio source on the
output mixer: a single OxiSynth (SoundFont loaded once, polyphony 256), a
min-heap of MIDI events keyed to the engine's 44.1 kHz sample clock, the
pre-rendered R2D2/synthesis buffers, and the MIDI bus `EffectsChain` (one
per side). `MidiPlayer::play(sequence, mode, &session_patches)` translates and sends a
`PlayCommand`; it returns the duration including effect tails.
`PlayCommand.tempo` carries the sequence tempo to the MIDI bus chain;
`render_patch` and the R2D2 chain take it directly; render tails come from
`EffectConfig::tail_seconds`.

1. `Translator` resolves `synth` references (session patches, then built-ins, then inline; an unknown name is an error) and converts musical time with the sequence's tempo and `beats_per_bar`.
2. Synthesis notes are grouped by patch, rendered on the tool thread by `render_patch` into one stereo buffer per patch (voices summed, the patch's optional LFO read once per sample and mapped onto `Modulation`, the patch's effects chain applied once, then `SYNTH_BUS_GAIN`) and scheduled as buffers. Between the chain and the gain sits a per-patch peak limiter (`PATCH_LIMITER_CEILING` 1.5), so a chord on one patch stays under the 0.8 soft-clip knee; the sum of several patches still relies on the mixer's soft clipper.
3. MIDI notes become time-ordered events: per call, the first note on a channel sends a program change (its instrument, or 0), controllers only when specified. Channel 9 is OxiSynth's drum channel; no bank select is needed.
4. The engine drains commands per 1024-frame chunk and applies events at their exact frame (`LEAD_FRAMES` = 2048 after the command). It sums the buses, soft-clips, and emits stereo.
5. `mode: replace` (default) fades 6 ms, sends SystemReset, clears the queue and installs the call's bus chain; `layer` mixes on top. `stop_playback` is the same reset with nothing scheduled.
6. When a command is scheduled, the engine remaps each of its melodic MIDI channels onto a physical channel no active playback owns (`allocate_channels`, issue #98): the logical channel itself when free, else the lowest free one, else it shares the channel that frees soonest and logs at info. A channel taken over from a finished playback first gets CC 121 plus volume, pan and reverb/chorus sends reset. Channel 9 is never remapped; replace and stop clear all ownership.

Known limitation: OxiSynth renders all 16 MIDI channels into one bus, so
per-channel effects are not yet possible (pan, volume, reverb/chorus CCs do
work per channel inside OxiSynth). A dedicated render thread behind the same
engine API is the next step if the callback still glitches.

### Export (`src/midi/export.rs`)
`export_audio` renders without a device. `Translator::translate_parts`
returns the sources kept apart (`TranslatedParts`: MIDI notes and bus
chain, one `PatchRender` per patch group, R2D2 buffers); `translate` is
that plus `into_command`. The exporter builds a private `MidiEngine` with
its own OxiSynth (about 70 ms to load, a deliberate exception to "one
engine per process" because it never touches the live path), replays a
`PlayCommand` through `apply` and `render_unclipped`, and drops the
lead-in. A mixdown is one pass through `soft_clip`; stems and tracks do
one pass per MIDI channel (OxiSynth cannot split channels otherwise) and
place patch and R2D2 buffers directly, unclipped. Tracks are translated
with `Effects::Dry` (bus, patch and R2D2 chains bypassed) and padded to the
wet length so every file lines up. Files are written with `hound`; names
are sanitized to `[A-Za-z0-9_-]`, channels are `ch<NN>_<gm name>`
(`ch09_drums`), patches `synth_<key>`, R2D2 `r2d2`. Tests read the files
back and measure them (`src/midi/export.rs` tests).

### Synthesis (`src/expressive/`)
- `synth.rs` - `ExpressiveSynth`: the R2D2 ring-modulation voice only. Swept oscillators use `PhaseAccumulator` (never `sin(2π·f(t)·t)`).
- `patch.rs` / `envelope.rs` / `oscillator.rs` / `engines/` / `wavetables.rs` / `render.rs` / `patches/*.json` - agent-defined synth patches: `Patch` (subtractive, fm, wavetable, granular and percussion engines plus an optional per-patch `lfo` and an effects chain), `SynthRef` (a name or an inline patch) and `render_patch`, which renders one patch's notes into a stereo buffer.
- `percussion.rs` - kick, snare, hi-hat, cymbal, zap, swoosh, chime, burst; these carry their own envelopes so the ADSR is skipped. Hits are peak-normalised to `PERCUSSION_PEAK` at note-on so `level` is comparable across kinds.
- `wavetables.rs` - builds eight procedural tables once (`OnceLock`) as ten per-octave band-limited levels; `WavetableVoice` picks the level from the note's pitch.
- `engines/granular.rs` - `GranularVoice`: at note-on builds one peak-normalised source cycle (`harmonics|noise|formant|inharmonic`; `noise` is a fresh random cycle each time) and scatters up to 32 overlapping grains across it, summed with 1/sqrt(active) normalisation and panned for true stereo width.
- `lfo.rs` - five-shape LFO; `render_patch` runs one per patch and maps it onto `Modulation` (`engines/mod.rs`) each sample: cutoff, pitch, amplitude, wavetable morph, grain density.
- `effects.rs` - stateful effects: Schroeder reverb, damped feedback delay with Time Fracture (beat-synced random time, pitch-shifted repeats), 3-voice chorus, TPT state-variable filter, compressor, tanh distortion. `EffectsChain::with_tempo(sample_rate, tempo, &[EffectConfig])` (or `new` for 120 BPM) then `process` per sample or `process_buffer`.
- `effects_presets.rs` - named chains ("studio", "concert_hall", ...).
- `patches/` - 44 built-in patches as JSON, embedded at compile time and loaded by `PatchLibrary`; a note's `synth` name resolves against the session's patches first, then these.
- `r2d2.rs` - emotion parameter tables (pitch contours, ranges).

### Data model (`src/midi/mod.rs`)
`SimpleNote` is one flat struct covering MIDI, R2D2, `synth` (a patch
reference) and effects fields (use `..Default::default()`); it is
`deny_unknown_fields`, so a misspelled key is a `-32602`. `MAX_NOTE_SECONDS`
(300) bounds `duration` and `start_time` so no note can size an unbounded
render buffer. `SimpleSequence` carries
`tempo` and `beats_per_bar`. `MusicalDuration` is a number (bars) or a
note-value string. `SequencePattern::quantize_notes` applies
`quantize_grid` when a pattern is defined. `gm_names.rs` has the GM
program and drum-key names used by `list_sounds`.

### Setup (`src/setup/`)
Downloads FluidR3_GM.sf2 (verified against a pinned SHA-256), writes the
Cursor MCP config, stores an optional custom SoundFont path.

## Important Architectural Decisions
- **Unified playback**: every audio type goes through `MidiPlayer::play` and the one `MidiEngine`.
- **Effects are stateful**: one instance per synth patch render (shared across every note of that patch in the call), one per R2D2 note, and one per MIDI bus. Do not reintroduce per-sample allocation or effect-count caps; the old "max 3 effects" and "2x gain compensation" rules were workarounds for stateless effects and are gone.
- **Stereo throughout** the mixer; mono sources are centered.
- **One engine per process**: `ServerState` owns one `MidiPlayer`, which owns the stream and the single `MidiEngine`; never create a synthesizer per call; the one exception is `export_audio`, which builds a private engine for an offline render (see Export above).
- **Headroom is measured, not assumed**: the library test renders each category's demo phrase (a chord for pads) and a held full-velocity note and asserts a 1.4 pre-bus ceiling below the limiter's 1.5; set levels with `cargo test print_builtin_headroom_survey -- --ignored --nocapture`.
