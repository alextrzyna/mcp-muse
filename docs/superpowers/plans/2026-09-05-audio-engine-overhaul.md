# Audio Engine Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the DSP paths do what their names say, fix the MCP protocol edges, and remove the dead parallel architecture, without changing the public tool names.

**Architecture:** Effects become stateful per-channel processors driven in blocks. Synthesis gets a real state-variable filter and phase-accumulated oscillators. The mixer becomes stereo. The server owns one audio stream and tracks active sinks so a `stop` tool can cancel playback. Tool failures become `isError` results.

**Tech Stack:** Rust 2024, rodio 0.21, oxisynth 0.1, serde_json, tracing.

**Spec:** `docs/superpowers/specs/2026-09-05-audio-engine-analysis.md`

## Global Constraints

- `cargo clippy -- -D warnings` and `cargo fmt --check` must pass after every task.
- Tool names `play_notes`, `define_sequence_pattern`, `play_sequence`, `list_patterns` stay unchanged; `tests/integration/mcp_protocol.rs` must keep passing (update assertions only where behaviour intentionally changes).
- No new heavy dependencies. `sha2` is the only allowed addition.
- Audio-quality changes are verified by rendering to sample buffers in unit tests (spectral or amplitude assertions), not by listening.

---

### Task 1: Logging that cannot fill the disk

**Files:**
- Modify: `Cargo.toml` (tracing-subscriber `env-filter` feature)
- Modify: `src/main.rs:36-56` (`init_logging`), add `prune_old_logs`
- Modify: `src/midi/player.rs` (delete per-sample `tracing::debug!` calls at ~949, ~1115, ~1147, ~1228, ~1787)
- Test: `src/main.rs` tests module

**Interfaces:**
- Produces: `fn prune_old_logs(dir: &Path, max_age: Duration) -> usize` (returns number of files removed). Env var `MCP_MUSE_LOG` (EnvFilter syntax, default `info`).

- [x] Write failing test `test_prune_old_logs_removes_only_old_rotated_files` in `src/main.rs` tests: create temp dir with `mcp-muse.log.2020-01-01` (mtime set old via `filetime`-free approach: create, then `File::set_modified`), `mcp-muse.log.<today>`, and `config.json`; assert only the old rotated file is removed.
- [x] Implement `prune_old_logs`: iterate dir, match filenames starting with `mcp-muse.log.`, remove when `metadata.modified()` older than `max_age`. Call from `init_logging` with 7 days.
- [x] Switch subscriber to `EnvFilter::try_from_env("MCP_MUSE_LOG").unwrap_or_else(|_| EnvFilter::new("info"))`.
- [x] Delete the per-sample debug logs in `player.rs`.
- [x] `cargo test && cargo clippy -- -D warnings`; commit `fix(logging): default to INFO, prune rotated logs, drop per-sample logging`.

### Task 2: Stateful effects processors

**Files:**
- Rewrite: `src/expressive/fundsp_effects.rs` → `src/expressive/effects.rs`
- Modify: `src/expressive/mod.rs`, `src/midi/player.rs` (`ChannelEffectsChain`, `ChannelProcessor`)
- Test: `src/expressive/effects.rs` tests

**Interfaces:**
- Produces: `pub struct EffectsChain { .. }` with `EffectsChain::new(sample_rate: f32, configs: &[EffectConfig]) -> Self`, `fn process(&mut self, sample: f32) -> f32`, `fn is_empty(&self) -> bool`. Internally `enum EffectNode { Reverb(Reverb), Delay(Delay), Chorus(Chorus), Filter(Svf), Compressor(Compressor), Distortion(Distortion) }`, each with `fn process(&mut self, x: f32) -> f32` and persistent buffers.
- `pub struct Svf` is reused by Task 3: `Svf::new(sample_rate, cutoff, q, mode: SvfMode)`, `fn process(&mut self, x) -> f32`, `fn set_cutoff(&mut self, hz)`.

- [x] Tests: (a) impulse through `Delay{delay_time:0.1}` produces a non-zero sample at index `4410` (sample_rate 44100); (b) white noise through `Svf` LowPass 500 Hz has energy above 5 kHz reduced by >20 dB relative to input (Goertzel at 8 kHz); (c) `Reverb` impulse response has non-zero samples beyond 200 ms; (d) `Compressor` reduces peak of a 0 dBFS sine by >3 dB with threshold −12 dB, ratio 4.
- [x] Implement each node with persistent state; Schroeder reverb keeps its comb/allpass buffers across calls; chorus uses linear-interpolated delay reads.
- [x] Replace `FunDSPEffectsProcessor` usage in `player.rs` with `EffectsChain`; delete the 3-effect cap and 2x gain compensation; delete `bypass_mode`, `mute`, `solo`, `pan` fields that nothing sets.
- [x] Update CLAUDE.md "Effects Limiting" and "Gain Compensation" bullets.
- [x] Test, clippy, commit `fix(effects): stateful per-channel effects chain`.

### Task 3: Real synthesis filter

**Files:**
- Modify: `src/expressive/synth.rs` (`generate_with_custom_dsp`, delete `apply_filter`)
- Test: `src/expressive/synth.rs` tests

- [x] Test: sawtooth 110 Hz with LowPass 500 Hz through `generate_synthesized_samples` has 8 kHz Goertzel energy >20 dB below the unfiltered render.
- [x] Keep one `Svf` per render in `generate_with_custom_dsp`; map `FilterParams.resonance` (0–1) to Q `0.5 + resonance * 9.5`.
- [x] Remove the "volume corrected" comment noise where it referred to the filter attenuation.
- [x] Test, clippy, commit `fix(synth): use a real state-variable filter`.

### Task 4: Phase accumulation for swept oscillators

**Files:**
- Modify: `src/expressive/synth.rs` (R2D2 `generate_r2d2_samples_with_contour`, `generate_r2d2_samples_static`, Kick, Zap, Swoosh), `src/expressive/fundsp_synth.rs` (Kick, Zap, Swoosh)
- Test: `src/expressive/synth.rs` tests

- [x] Test: render R2D2 "Sad" contour (2.2→0.3 multiplier, 1 s, base 300 Hz); estimate zero-crossing rate in the first 100 ms vs the last 100 ms; assert last < first (monotonic descent).
- [x] Introduce `struct PhaseAccumulator { phase: f32, sample_rate: f32 }` with `fn next(&mut self, freq: f32) -> f32` (returns sin of phase, advances). Use it wherever frequency varies per sample.
- [x] Delete the "SPECIAL CASE: Force sad emotion" hack in `interpolate_pitch_contour`; the contour values already describe the descent once phase is accumulated.
- [x] Test, clippy, commit `fix(synth): accumulate phase for frequency sweeps`.

### Task 5: Unify the two synthesis paths

**Files:**
- Rename: `src/expressive/fundsp_synth.rs` → `src/expressive/percussion.rs` (struct `PercussionSynth`, only drum + FX generators kept)
- Modify: `src/expressive/synth.rs` (`generate_synthesized_samples` applies filter, effects and, for non-percussive types, envelope to every render), `src/expressive/mod.rs`, `Cargo.toml` (remove `fundsp`, `base64`)
- Test: `src/expressive/synth.rs`

- [x] Test: `SynthType::Pad` render with envelope attack 0.8 s has RMS over the first 100 ms < 25% of RMS over 0.9–1.0 s.
- [x] Route Pad/Texture/FM/Granular through the custom DSP path (they were duplicated); keep the percussion module for Kick/Snare/HiHat/Cymbal/Zap/Swoosh. Apply `Svf` + `EffectsChain` (converted from `EffectParams`) to percussion output; apply ADSR only to non-percussive types.
- [x] Remove `fundsp` and `base64` from `Cargo.toml`.
- [x] Test, clippy, commit `refactor(synth): single synthesis pipeline, drop unused fundsp/base64`.

### Task 6: Stereo mixer and honest MIDI routing

**Files:**
- Modify: `src/midi/player.rs` (`OxiSynthSource` yields interleaved stereo; `EnhancedHybridAudioSource` `channels() == 2`; remove `has_drums_playing` and the 3x copy; gain 20×0.7 → a single `MIDI_GAIN` constant of 4.0)
- Test: `src/midi/player.rs`

- [x] Test: `OxiSynthSource` with one note panned hard right (pan 127) renders left RMS < 10% of right RMS. Requires the SoundFont; mark `#[ignore]` when `assets/FluidR3_GM.sf2` is missing (skip with early return + eprintln).
- [x] Implement stereo: MIDI L/R each get their own `EffectsChain` (two instances built from the same config); R2D2 and synthesis are mono, added equally to both sides.
- [x] Update CLAUDE.md "Channel Routing" bullet.
- [x] Test, clippy, commit `fix(mixer): stereo output, remove drum-boost routing hack`.

### Task 7: One audio stream per process, stop tool, durations

**Files:**
- Modify: `src/midi/player.rs` (`MidiPlayer` holds `OutputStream` + `Vec<Sink>`; `play_enhanced_mixed` returns `Duration`; add `fn stop_all(&mut self)`, `fn prune_finished(&mut self)`), `src/expressive/synth.rs` (`ExpressiveSynth::new` no longer opens a stream), `src/server/mcp.rs` (player created once in `run_stdio_server`, passed to handlers; add `stop_playback` tool)
- Test: `tests/integration/mcp_protocol.rs` (`stop_playback` appears in tools/list; tool count 6 after Task 9's catalog tool)

- [x] Change `play_enhanced_mixed(&mut self, ..) -> Result<Duration, String>`; new `Sink` per call on the shared mixer, pushed to `self.sinks`; `prune_finished` retains `!sink.empty()`.
- [x] `handle_*` functions take `&mut MidiPlayer`; delete `Box::leak`.
- [x] Add `stop_playback` tool (no args) → `stop_all`, result text "Stopped N active playbacks".
- [x] Success text for play tools includes `duration_seconds` in a second content line.
- [x] Test, clippy, commit `feat(server): shared audio stream, stop_playback tool, playback duration`.

### Task 8: MCP protocol conformance

**Files:**
- Modify: `src/server/mcp.rs`
- Test: `tests/integration/mcp_protocol.rs`

- [x] Tests: (a) `ping` returns `{}` result; (b) a `notifications/cancelled` message produces no output line (send it, then a `tools/list`, assert the next line is the tools/list response); (c) `play_notes` with an unknown preset returns `result.isError == true` and a `content[0].text` mentioning the preset name; (d) parse error response has `"id": null`.
- [x] Implement: `serialize_id` → null; `ping`; skip any method starting with `notifications/`; `tool_error(id, msg)` helper returning `isError: true`; `serverInfo.version = env!("CARGO_PKG_VERSION")`.
- [x] Test, clippy, commit `fix(mcp): isError tool results, ping, notification handling, real version`.

### Task 9: Schema and model fixes, catalog tool

**Files:**
- Modify: `src/midi/mod.rs` (`MusicalDuration` → `Bars(f64) | NoteValue(NoteValue)`; `SequencePattern::quantize` applied on define; `resolve_patterns` uses `pattern.beats_per_bar`), `src/midi/player.rs` (musical time uses sequence `beats_per_bar`, add `beats_per_bar` to `SimpleSequence` default 4), `src/server/mcp.rs` (schema fixes; `list_sounds` tool; drop `organ`/`arp` from enums until presets exist), `src/expressive/presets/library.rs` (`catalog()`), new `src/midi/gm_names.rs` (128 GM program names)
- Test: `src/midi/mod.rs`, `tests/integration/mcp_protocol.rs`

- [x] Tests: (a) `musical_duration: 2` deserializes to `Bars(2.0)`; (b) pattern with `quantize_grid: "16th"` and a note at tick 100 stores tick 120; (c) `list_sounds` returns text containing "Minimoog Bass", "Acoustic Grand Piano", "Happy", "studio".
- [x] Implement; remove `type: object` from `musical_duration` schema; `beat.maximum` → 8.
- [x] Update README preset count and category list.
- [x] Test, clippy, commit `fix(schema): musical duration, quantize, time signature; add list_sounds tool`.

### Task 10: Remove dead architecture and relocate demos

**Files:**
- Delete: `src/expressive/voice.rs`, `src/midi/polyphonic_source.rs`, `src/tests/`
- Move: test commands from `src/main.rs` → `src/demos.rs` (drop `test-polyphony`, `debug-dx7`)
- Modify: `src/main.rs`, `src/expressive/mod.rs`, `src/midi/mod.rs`, `Cargo.toml` (`tokio` → `rt` + `time` only... demos use `std::thread::sleep` instead, remove tokio entirely), CLAUDE.md commands list
- Test: `cargo build`, `cargo test`

- [x] Move `impl Default for SimpleNote` from `main.rs` to `src/midi/mod.rs` and replace the four ~50-line constructors with `..Default::default()`.
- [x] Commit `refactor: remove unused voice manager, move demo commands out of main`.

### Task 11: DX7 operator routing, PolyBLEP, detune

**Files:**
- Modify: `src/expressive/synth.rs`
- Test: `src/expressive/synth.rs`

- [x] Tests: (a) DX7FM with op2 ratio 2.0 modulating op1 produces sidebands: Goertzel at `f0 ± 2·f0` (i.e. 3·f0) exceeds −40 dBFS while pure sine does not; (b) PolyBLEP saw at 4 kHz has less energy above Nyquist-aliased band than naive saw (compare Goertzel at an alias frequency).
- [x] Implement: algorithm table for DX7 algorithms as `modulates: [Option<usize>; 6]` chains; support algorithms 1, 5, 16, 32 exactly and treat others as "op(n) modulates op(n-1)" chains. Detune in cents (`2^(detune/1200)`). PolyBLEP for `Square` and `Sawtooth`.
- [x] Commit `feat(synth): DX7 operator routing, band-limited oscillators`.

### Task 12: SoundFont checksum and dependency trim

**Files:**
- Modify: `src/setup/mod.rs`, `Cargo.toml` (`sha2`, `midly` → dev-dependency)
- Test: `src/setup/mod.rs`

- [x] Constant `SOUNDFONT_SHA256 = "74594e8f4250680adf590507a306655a299935343583256f3b722c48a1bc1cb0"` (hash of the locally installed FluidR3_GM.sf2). Verify after extraction; on mismatch delete the file and return an error naming both hashes.
- [x] Test: `verify_sha256(&[u8], expected_hex)` returns Ok for a known vector.
- [x] Commit `fix(setup): verify SoundFont checksum; trim dependencies`.

### Task 13: Documentation

**Files:**
- Modify: `CLAUDE.md`, `README.md`, `examples/api_reference.md`, `examples/basic_usage.md`

- [x] Replace `play_midi`/`play_r2d2_expression` references with the current tool set; document `MCP_MUSE_LOG`, `stop_playback`, `list_sounds`.
- [x] Commit `docs: align with current tool set and engine behaviour`.
