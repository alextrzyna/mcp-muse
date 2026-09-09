# Audio Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `export_audio` MCP tool that renders a composition offline and writes it to disk as a stereo mixdown, wet stems or dry tracks in WAV.

**Architecture:** The `Translator` is split so its sound sources (MIDI notes, per-patch buffers, R2D2 buffers, bus chain) stay separate in a `TranslatedParts` struct; playback flattens them into today's `PlayCommand`. A new `src/midi/export.rs` drives a private `MidiEngine` (its own OxiSynth, no audio device) through the existing `apply` and `render_unclipped` calls, one pass for the mixdown or one pass per MIDI channel for splits, and writes WAV with `hound`. The server adds the tool by reusing `play_sequence`'s argument handling.

**Tech Stack:** Rust 2024 edition, OxiSynth, `hound` 3.5 for WAV read/write, existing `Goertzel`/RMS test helpers in `src/expressive/test_util.rs`.

**Spec:** `docs/superpowers/specs/2026-09-09-audio-export-design.md`

## Global Constraints

- WAV output is always stereo at 44 100 Hz (`SAMPLE_RATE`); `bit_depth` is 16, 24 (default) or 32 (32 = IEEE float).
- `path` is a required absolute directory; a relative path is a `-32602`.
- Existing files are never overwritten unless `overwrite: true`; a collision is a tool error (`isError: true`), not `-32602`.
- Stems and tracks are not soft-clipped; the mixdown is soft-clipped with `soft_clip` exactly like playback.
- Tracks bypass the MIDI bus chain, patch effect chains and R2D2 effect chains; OxiSynth's reverb/chorus controller sends stay.
- Every file in one export has the same frame count: the wet duration rounded up to whole frames.
- Names are sanitized to `[A-Za-z0-9_-]`, other characters become `_`. Default base name is `mix`.
- Tests measure buffers (RMS, Goertzel power, frame counts); no listen-by-ear checks. SoundFont-dependent tests skip with a message when `find_soundfont()` fails, like the existing headroom tests.
- Before claiming clippy clean: `rustup update stable`, then `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`.
- Commit messages end with the attribution lines given in the session (`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and the `Claude-Session:` line).
- Work happens in the worktree `/Users/alex/source/mcp-muse/.claude/worktrees/audio-export` on branch `audio-export`. The SoundFont is symlinked at `assets/FluidR3_GM.sf2` there (gitignored).

---

## File structure

- `src/midi/translate.rs` (modify): `Effects`, `PatchRender`, `TranslatedParts`, `Translator::translate_parts`; `translate` becomes a flatten of `translate_parts(.., Wet)`.
- `src/midi/export.rs` (create): `Split`, `BitDepth`, `ExportRequest`, `ExportedFile`, `ExportReport`, `sanitize_name`, `export`, `export_with` plus private `render_offline`, `place`, `sources`, `channel_name`, `write_wav`.
- `src/midi/mod.rs` (modify): `pub mod export;`.
- `src/server/mcp.rs` (modify): `pattern_reference_schema()` extracted from the `play_sequence` schema, `export_audio` tool schema, `ExportOptions`, `resolve_sequence_arguments` extracted from `handle_play_sequence`, `handle_export_audio`, `export_finished_text`.
- `Cargo.toml` (modify): `hound = "3.5"` dependency.
- `tests/integration/mcp_protocol.rs` (modify): tool count 7 → 8.
- `tests/integration/export_audio.rs` (create) and `tests/integration/mod.rs` (modify): end-to-end export through the binary.
- `CLAUDE.md`, `README.md` (modify): document the eighth tool and the export module.

---

### Task 1: Split the translator into parts and a flatten

**Files:**
- Modify: `src/midi/translate.rs` (the `Translation`/`Translator` block starting around line 100 and the body of `translate`, lines 157–390)

**Interfaces:**
- Consumes: `PlayCommand`, `PlayMode`, `SYNTH_BUS_GAIN`, `seconds_to_frames` from `crate::midi::engine`; `MidiNote` from `crate::midi::parser`; `render_patch`, `render_length_seconds`, `Patch`, `EffectsChain` from `crate::expressive`.
- Produces:
  ```rust
  pub enum Effects { Wet, Dry }
  pub struct PatchRender { pub name: String, pub start: u64, pub samples: Vec<[f32; 2]> }
  pub struct TranslatedParts {
      pub midi: Vec<MidiNote>,
      pub midi_effects: Option<Vec<EffectConfig>>,
      pub patches: Vec<PatchRender>,
      pub r2d2: Vec<(u64, Vec<[f32; 2]>)>,
      pub tempo: u32,
      pub duration: Duration,
  }
  impl TranslatedParts { pub fn into_command(self, mode: PlayMode) -> PlayCommand }
  impl Translator {
      pub fn translate_parts(&self, sequence: SimpleSequence, session_patches: &HashMap<String, Patch>, effects: Effects) -> Result<TranslatedParts, String>
  }
  ```
  `translate(sequence, mode, session_patches)` keeps its signature and behaviour.

- [ ] **Step 1: Write the failing tests**

Add to the second `mod tests` block in `src/midi/translate.rs` (the one that defines `seq`, `no_session` and `patch_note`, around line 610), after `patch_notes_become_buffers_at_bus_level`:

```rust
    fn r2d2_note(duration: f64, effects: Option<Vec<EffectConfig>>) -> SimpleNote {
        SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(duration),
            effects,
            ..Default::default()
        }
    }

    fn wet_reverb() -> EffectConfig {
        serde_json::from_value(json!({
            "type": "reverb", "room_size": 0.8, "wet_level": 0.5, "intensity": 0.8
        }))
        .unwrap()
    }

    #[test]
    fn dry_parts_bypass_every_effect_chain() {
        let t = Translator::new(Ok(()));
        let reverb_patch = json!({"name": "wet", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "effects": [{"type": "reverb", "room_size": 0.8, "wet_level": 0.5, "intensity": 0.8}]});
        let notes = || {
            vec![
                patch_note(reverb_patch.clone(), 69, 0.0, 0.5),
                r2d2_note(0.5, Some(vec![wet_reverb()])),
                SimpleNote {
                    note: Some(60),
                    duration: Some(0.5),
                    effects: Some(vec![wet_reverb()]),
                    ..Default::default()
                },
            ]
        };
        let wet = t
            .translate_parts(seq(notes()), &no_session(), Effects::Wet)
            .unwrap();
        let dry = t
            .translate_parts(seq(notes()), &no_session(), Effects::Dry)
            .unwrap();

        assert!(wet.midi_effects.is_some(), "wet keeps the bus chain");
        assert!(dry.midi_effects.is_none(), "dry drops the bus chain");
        assert_eq!(wet.midi.len(), 1);
        assert_eq!(dry.midi.len(), 1);
        assert_eq!(wet.patches.len(), 1);
        assert_eq!(dry.patches.len(), 1);
        assert_eq!(dry.patches[0].name, "wet");
        assert!(
            dry.patches[0].samples.len() < wet.patches[0].samples.len(),
            "a dry patch buffer has no reverb tail"
        );
        assert_eq!(wet.r2d2.len(), 1);
        assert!(
            dry.r2d2[0].1.len() < wet.r2d2[0].1.len(),
            "a dry R2D2 buffer has no reverb tail"
        );
        assert!(dry.duration < wet.duration);
    }

    #[test]
    fn wet_parts_flatten_to_the_playback_command() {
        let t = Translator::new(Ok(()));
        let notes = || {
            vec![
                patch_note(json!({"name": "s", "subtractive": {}}), 60, 0.25, 0.5),
                patch_note(json!("tr_808_kick"), 36, 0.0, 0.25),
                SimpleNote {
                    note: Some(60),
                    duration: Some(0.5),
                    channel: 2,
                    instrument: Some(73),
                    ..Default::default()
                },
            ]
        };
        let direct = t
            .translate(seq(notes()), PlayMode::Layer, &no_session())
            .unwrap();
        let parts = t
            .translate_parts(seq(notes()), &no_session(), Effects::Wet)
            .unwrap();
        assert_eq!(parts.duration, direct.duration);
        assert_eq!(parts.tempo, 120);

        let command = parts.into_command(PlayMode::Layer);
        assert_eq!(command.mode, PlayMode::Layer);
        assert_eq!(command.tempo, direct.command.tempo);
        assert_eq!(command.events, direct.command.events);
        assert_eq!(command.midi_effects.is_some(), direct.command.midi_effects.is_some());
        let shape = |c: &PlayCommand| -> Vec<(u64, usize)> {
            c.buffers.iter().map(|(s, b)| (*s, b.len())).collect()
        };
        assert_eq!(shape(&command), shape(&direct.command));
    }
```

Add `use crate::midi::engine::PlayCommand;` to that test module's imports if `PlayCommand` is not already in scope there (the module starts with `use super::*;`, and `PlayCommand` is imported at the top of the file, so it should be).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse translate::tests::dry_parts_bypass_every_effect_chain translate::tests::wet_parts_flatten 2>&1 | tail -20`
Expected: compile error, `no method named translate_parts` / `cannot find type Effects`.

- [ ] **Step 3: Add the types and refactor `translate`**

Replace the `Translation` struct block (around line 105) with:

```rust
/// Result of translating one call.
#[derive(Debug)]
pub struct Translation {
    pub command: PlayCommand,
    /// Time until the last note plus its effect tail; reported to the caller.
    pub duration: Duration,
}

/// Whether the patch, R2D2 and MIDI-bus effect chains are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effects {
    /// Every chain applied, as in playback.
    Wet,
    /// Every chain bypassed: dry material for mixing elsewhere.
    Dry,
}

/// One patch group's rendered stereo buffer, already at bus level.
#[derive(Debug, Clone)]
pub struct PatchRender {
    /// The patch's key (`Patch::key`), used to name an exported file.
    pub name: String,
    /// Frames after the start of the composition.
    pub start: u64,
    pub samples: Vec<[f32; 2]>,
}

/// A translated call with its sound sources kept apart, so an export can
/// write them separately. Playback flattens it with `into_command`.
#[derive(Debug)]
pub struct TranslatedParts {
    pub midi: Vec<MidiNote>,
    /// MIDI bus chain for this call, if any MIDI note specified one.
    pub midi_effects: Option<Vec<EffectConfig>>,
    pub patches: Vec<PatchRender>,
    /// One buffer per R2D2 note at its start frame.
    pub r2d2: Vec<(u64, Vec<[f32; 2]>)>,
    pub tempo: u32,
    /// Time until the last note plus its effect tail.
    pub duration: Duration,
}

impl TranslatedParts {
    /// Flatten into the engine's command: R2D2 buffers first, then patch
    /// buffers (the order playback has always used).
    pub fn into_command(self, mode: PlayMode) -> PlayCommand {
        let mut buffers = self.r2d2;
        buffers.extend(self.patches.into_iter().map(|p| (p.start, p.samples)));
        PlayCommand {
            events: midi_events(&self.midi),
            buffers,
            midi_effects: self.midi_effects,
            mode,
            tempo: self.tempo,
        }
    }
}
```

Then change `translate` so it delegates, and rename the old body to `translate_parts`. The new `translate`:

```rust
    pub fn translate(
        &self,
        sequence: SimpleSequence,
        mode: PlayMode,
        session_patches: &HashMap<String, Patch>,
    ) -> Result<Translation, String> {
        let parts = self.translate_parts(sequence, session_patches, Effects::Wet)?;
        let duration = parts.duration;
        Ok(Translation {
            command: parts.into_command(mode),
            duration,
        })
    }

    /// Translate without flattening. `effects` selects wet (playback) or
    /// dry (bypassed chains) rendering of the patch and R2D2 buffers and
    /// decides whether the MIDI bus chain is kept.
    pub fn translate_parts(
        &self,
        sequence: SimpleSequence,
        session_patches: &HashMap<String, Patch>,
        effects: Effects,
    ) -> Result<TranslatedParts, String> {
```

Inside the old body make these edits:

1. The empty-sequence early return becomes:
   ```rust
        if sequence.notes.is_empty() {
            return Ok(TranslatedParts {
                midi: Vec::new(),
                midi_effects: None,
                patches: Vec::new(),
                r2d2: Vec::new(),
                tempo: sequence.tempo,
                duration: Duration::ZERO,
            });
        }
   ```
2. The `midi_effects` computation becomes:
   ```rust
        let midi_effects: Option<Vec<EffectConfig>> = match effects {
            Effects::Dry => None,
            Effects::Wet => processed_notes
                .iter()
                .filter(|n| n.note_type != "r2d2" && !n.is_synthesis())
                .find_map(|n| n.effects.clone().filter(|e| !e.is_empty())),
        };
   ```
3. Rename the local `buffers` to `r2d2_buffers` (it now holds only R2D2 buffers) and add `let mut patches: Vec<PatchRender> = Vec::new();` next to it.
4. In the R2D2 branch, the chain input becomes:
   ```rust
                let note_effects: &[EffectConfig] = match effects {
                    Effects::Dry => &[],
                    Effects::Wet => note.effects.as_deref().unwrap_or(&[]),
                };
                let mut chain =
                    EffectsChain::with_tempo(SAMPLE_RATE as f32, sequence.tempo, note_effects);
                let mut tail = 0.0f32;
                if !chain.is_empty() {
                    tail = r2d2_tail_seconds(note_effects, sequence.tempo);
   ```
   and the push goes to `r2d2_buffers`.
5. The patch-group loop becomes:
   ```rust
        for (_, mut patch, timed_events) in patch_groups {
            if effects == Effects::Dry {
                patch.effects.clear();
            }
            let first = timed_events
                .iter()
                .map(|(abs, _)| *abs)
                .fold(f64::INFINITY, f64::min);
            let events: Vec<NoteEvent> = timed_events
                .iter()
                .map(|(abs, event)| NoteEvent {
                    start: (*abs - first) as f32,
                    ..*event
                })
                .collect();
            let mut samples = render_patch(&patch, &events, SAMPLE_RATE as f32, sequence.tempo);
            for frame in &mut samples {
                frame[0] *= SYNTH_BUS_GAIN;
                frame[1] *= SYNTH_BUS_GAIN;
            }
            patches.push(PatchRender {
                name: patch.key(),
                start: seconds_to_frames(Duration::from_secs_f64(first)),
                samples,
            });
            note_end = note_end.max(Duration::from_secs_f64(
                first + render_length_seconds(&patch, &events, sequence.tempo) as f64,
            ));
        }
   ```
6. The `duration` computation's emptiness check becomes `if midi_notes.is_empty() && r2d2_buffers.is_empty() && patches.is_empty()`.
7. The final `tracing::info!` reports `r2d2_buffers.len() + patches.len()` buffers and drops `mode.as_str()` (there is no mode here); adjust the format string to `"Translated {} MIDI notes and {} buffers, {:.2}s including tail"`.
8. The return becomes:
   ```rust
        Ok(TranslatedParts {
            midi: midi_notes,
            midi_effects,
            patches,
            r2d2: r2d2_buffers,
            tempo: sequence.tempo,
            duration,
        })
   ```

- [ ] **Step 4: Run the whole translator test module**

Run: `cargo test --bin mcp-muse translate:: 2>&1 | tail -20`
Expected: all pass, including the two new tests and the existing `notes_on_the_same_patch_share_one_buffer_scheduled_at_the_first_note` (buffer order unchanged: R2D2 then patches).

- [ ] **Step 5: Run the full suite, fmt and clippy**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5 && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: no clippy warnings; both `test result: ok` lines.

- [ ] **Step 6: Commit**

```bash
git add src/midi/translate.rs
git commit -m "Split the translator into parts and a playback flatten

translate_parts keeps MIDI notes, patch buffers, R2D2 buffers and the bus
chain apart and can render them dry; translate flattens the wet parts into
the PlayCommand exactly as before. Groundwork for audio export.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01ED7wujQfBxcx4T4eGirX9i"
```

---

### Task 2: Export module with mixdown, WAV writing and file rules

**Files:**
- Modify: `Cargo.toml` (add `hound = "3.5"` under `[dependencies]`)
- Modify: `src/midi/mod.rs:1-8` (add `pub mod export;`)
- Create: `src/midi/export.rs`

**Interfaces:**
- Consumes: `Translator::translate_parts`, `TranslatedParts`, `Effects` (Task 1); `MidiEngine::new`, `MidiEngine::apply`, `MidiEngine::render_unclipped` (`pub(crate)`), `EngineCommand`, `PlayCommand`, `PlayMode`, `CHUNK_FRAMES`, `LEAD_FRAMES`, `SAMPLE_RATE`, `find_soundfont`, `load_synth`, `soft_clip` from `crate::midi::engine`.
- Produces:
  ```rust
  pub enum Split { Mixdown, Stems, Tracks }          // serde lowercase, Default = Mixdown
  pub enum BitDepth { Int16, Int24, Float32 }          // Default = Int24
  impl BitDepth { pub fn from_bits(bits: u32) -> Result<Self, String> }
  pub struct ExportRequest { pub sequence: SimpleSequence, pub dir: PathBuf, pub name: String, pub split: Split, pub bit_depth: BitDepth, pub overwrite: bool }
  pub struct ExportedFile { pub path: PathBuf, pub peak: f32 }
  pub struct ExportReport { pub files: Vec<ExportedFile>, pub duration: Duration, pub render_time: Duration }
  pub fn sanitize_name(name: &str) -> String
  pub fn export(request: ExportRequest, session_patches: &HashMap<String, Patch>) -> Result<ExportReport, String>
  pub(crate) fn export_with(request: ExportRequest, session_patches: &HashMap<String, Patch>, soundfont: Result<PathBuf, String>) -> Result<ExportReport, String>
  ```
  In this task `Split::Stems` and `Split::Tracks` return `Err("… not implemented yet")`; Task 3 fills them in.

- [ ] **Step 1: Add the dependency and module**

In `Cargo.toml` under `[dependencies]`, after `sha2 = "0.11"`, add:

```toml
hound = "3.5"
```

In `src/midi/mod.rs`, after `pub mod engine;`, add `pub mod export;`.

- [ ] **Step 2: Write the failing tests**

Create `src/midi/export.rs` with only a test module for now:

```rust
//! Offline export of a composition to WAV files: a stereo mixdown, wet stems
//! or dry tracks. Design: docs/superpowers/specs/2026-09-09-audio-export-design.md.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::SimpleNote;
    use std::path::Path;

    /// A fresh, empty directory under the system temp dir, unique per test.
    pub(super) fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mcp-muse-export-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A sine patch note with a short release and no effects.
    pub(super) fn sine_note(start: f64, duration: f64, level: f32) -> SimpleNote {
        SimpleNote {
            note: Some(69),
            velocity: Some(100),
            start_time: Some(start),
            duration: Some(duration),
            synth: Some(
                serde_json::from_value(serde_json::json!({
                    "name": format!("sine_{}", (level * 100.0) as u32),
                    "level": level,
                    "subtractive": {"osc1": {"wave": "sine"},
                        "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}
                }))
                .unwrap(),
            ),
            ..Default::default()
        }
    }

    pub(super) fn midi_note(channel: u8, key: u8, instrument: Option<u8>) -> SimpleNote {
        SimpleNote {
            note: Some(key),
            velocity: Some(100),
            duration: Some(1.0),
            channel,
            instrument,
            ..Default::default()
        }
    }

    pub(super) fn request(
        notes: Vec<SimpleNote>,
        dir: &Path,
        name: &str,
        split: Split,
        bit_depth: BitDepth,
    ) -> ExportRequest {
        ExportRequest {
            sequence: SimpleSequence {
                notes,
                tempo: 120,
                beats_per_bar: 4,
            },
            dir: dir.to_path_buf(),
            name: name.to_string(),
            split,
            bit_depth,
            overwrite: false,
        }
    }

    /// Read a WAV back as normalised stereo frames plus its spec.
    pub(super) fn read_wav(path: &Path) -> (hound::WavSpec, Vec<[f32; 2]>) {
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        let mono: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => {
                let full = ((1u64 << (spec.bits_per_sample - 1)) - 1) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / full)
                    .collect()
            }
        };
        (spec, mono.chunks(2).map(|c| [c[0], c[1]]).collect())
    }

    pub(super) fn rms_left(frames: &[[f32; 2]], from_s: f64, to_s: f64) -> f32 {
        let a = (from_s * SAMPLE_RATE as f64) as usize;
        let b = ((to_s * SAMPLE_RATE as f64) as usize).min(frames.len());
        let left: Vec<f32> = frames[a..b].iter().map(|f| f[0]).collect();
        crate::expressive::test_util::rms(&left)
    }

    #[test]
    fn sanitize_name_keeps_letters_digits_underscore_and_dash() {
        assert_eq!(sanitize_name(" My Mix/01! "), "My_Mix_01_");
        assert_eq!(sanitize_name("take-2_final"), "take-2_final");
        assert_eq!(sanitize_name("   "), "");
    }

    #[test]
    fn bit_depth_parses_16_24_and_32_only() {
        assert_eq!(BitDepth::from_bits(16).unwrap(), BitDepth::Int16);
        assert_eq!(BitDepth::from_bits(24).unwrap(), BitDepth::Int24);
        assert_eq!(BitDepth::from_bits(32).unwrap(), BitDepth::Float32);
        assert!(BitDepth::from_bits(8).unwrap_err().contains("bit_depth"));
    }

    #[test]
    fn a_mixdown_is_silent_before_the_note_and_sounds_after_it() {
        let dir = tmp_dir("mixdown").join("nested");
        let report = export_with(
            request(
                vec![sine_note(0.5, 0.5, 0.8)],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("no soundfont".into()),
        )
        .unwrap();

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, dir.join("mix.wav"));
        let (spec, frames) = read_wav(&report.files[0].path);
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, SAMPLE_RATE);
        assert_eq!(spec.bits_per_sample, 24);
        let expected = (report.duration.as_secs_f64() * SAMPLE_RATE as f64).ceil() as usize;
        assert_eq!(frames.len(), expected, "file length is the reported duration");
        assert_eq!(rms_left(&frames, 0.0, 0.45), 0.0, "silence before the note");
        assert!(rms_left(&frames, 0.55, 0.95) > 0.05, "energy during the note");
        assert!(report.files[0].peak > 0.1);
    }

    #[test]
    fn bit_depths_set_the_wav_spec() {
        let dir = tmp_dir("depths");
        for (depth, bits, format) in [
            (BitDepth::Int16, 16, hound::SampleFormat::Int),
            (BitDepth::Int24, 24, hound::SampleFormat::Int),
            (BitDepth::Float32, 32, hound::SampleFormat::Float),
        ] {
            let report = export_with(
                request(
                    vec![sine_note(0.0, 0.1, 0.5)],
                    &dir,
                    &format!("d{}", bits),
                    Split::Mixdown,
                    depth,
                ),
                &HashMap::new(),
                Err("no soundfont".into()),
            )
            .unwrap();
            let (spec, frames) = read_wav(&report.files[0].path);
            assert_eq!(spec.bits_per_sample, bits);
            assert_eq!(spec.sample_format, format);
            assert!(rms_left(&frames, 0.0, 0.1) > 0.02);
        }
    }

    #[test]
    fn existing_files_are_not_overwritten_unless_asked() {
        let dir = tmp_dir("overwrite");
        let mut req = || {
            request(
                vec![sine_note(0.0, 0.1, 0.5)],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int16,
            )
        };
        export_with(req(), &HashMap::new(), Err("x".into())).unwrap();
        let err = export_with(req(), &HashMap::new(), Err("x".into())).unwrap_err();
        assert!(err.contains("overwrite"), "{}", err);
        assert!(err.contains("mix.wav"), "{}", err);
        let mut again = req();
        again.overwrite = true;
        export_with(again, &HashMap::new(), Err("x".into())).unwrap();
    }

    #[test]
    fn midi_notes_need_a_soundfont_and_nothing_is_written_without_one() {
        let dir = tmp_dir("nosf");
        let err = export_with(
            request(
                vec![midi_note(0, 60, Some(0))],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("SoundFont not found".into()),
        )
        .unwrap_err();
        assert!(err.contains("SoundFont"), "{}", err);
        assert!(!dir.exists(), "no directory is created on failure");
    }

    #[test]
    fn a_name_that_is_empty_after_trimming_is_rejected() {
        // Only whitespace sanitizes to nothing; "!!!" becomes "___" and is allowed.
        let dir = tmp_dir("noname");
        let err = export_with(
            request(
                vec![sine_note(0.0, 0.1, 0.5)],
                &dir,
                "   ",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap_err();
        assert!(err.contains("name"), "{}", err);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse export:: 2>&1 | tail -20`
Expected: compile errors, `cannot find function export_with`, `cannot find type Split`, etc.

- [ ] **Step 4: Write the implementation**

Insert above the test module in `src/midi/export.rs`:

```rust
use crate::expressive::Patch;
use crate::midi::SimpleSequence;
use crate::midi::engine::{
    CHUNK_FRAMES, EngineCommand, LEAD_FRAMES, MidiEngine, PlayCommand, PlayMode, SAMPLE_RATE,
    find_soundfont, load_synth, soft_clip,
};
use crate::midi::translate::{Effects, TranslatedParts, Translator};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Split {
    /// One stereo file, soft-clipped like playback.
    #[default]
    Mixdown,
    /// One file per source with every effect chain applied; they sum to the mix.
    Stems,
    /// One file per source with the effect chains bypassed.
    Tracks,
}

impl Split {
    pub fn as_str(self) -> &'static str {
        match self {
            Split::Mixdown => "mixdown",
            Split::Stems => "stems",
            Split::Tracks => "tracks",
        }
    }
}

/// WAV sample format. Integer formats are clamped to ±1.0; float keeps the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BitDepth {
    Int16,
    #[default]
    Int24,
    Float32,
}

impl BitDepth {
    pub fn from_bits(bits: u32) -> Result<Self, String> {
        match bits {
            16 => Ok(BitDepth::Int16),
            24 => Ok(BitDepth::Int24),
            32 => Ok(BitDepth::Float32),
            other => Err(format!(
                "bit_depth must be 16, 24 or 32 (32 is IEEE float), got {}",
                other
            )),
        }
    }

    fn spec(self) -> hound::WavSpec {
        let (bits_per_sample, sample_format) = match self {
            BitDepth::Int16 => (16, hound::SampleFormat::Int),
            BitDepth::Int24 => (24, hound::SampleFormat::Int),
            BitDepth::Float32 => (32, hound::SampleFormat::Float),
        };
        hound::WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE,
            bits_per_sample,
            sample_format,
        }
    }
}

pub struct ExportRequest {
    pub sequence: SimpleSequence,
    /// Absolute directory the files go under.
    pub dir: PathBuf,
    /// Base name before sanitizing.
    pub name: String,
    pub split: Split,
    pub bit_depth: BitDepth,
    pub overwrite: bool,
}

#[derive(Debug)]
pub struct ExportedFile {
    pub path: PathBuf,
    /// Largest absolute sample before any clamping.
    pub peak: f32,
}

#[derive(Debug)]
pub struct ExportReport {
    pub files: Vec<ExportedFile>,
    /// The composition's length including effect tails; every file has it.
    pub duration: Duration,
    pub render_time: Duration,
}

/// Keep `[A-Za-z0-9_-]`; every other character becomes `_`.
pub fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Render and write the request. Uses the installed SoundFont for MIDI notes.
pub fn export(
    request: ExportRequest,
    session_patches: &HashMap<String, Patch>,
) -> Result<ExportReport, String> {
    export_with(request, session_patches, find_soundfont())
}

/// One target file and what goes into it.
struct Target {
    path: PathBuf,
    source: Source,
}

enum Source {
    Mixdown,
}

/// `export` with the SoundFont lookup injected, so tests can run without one.
pub(crate) fn export_with(
    request: ExportRequest,
    session_patches: &HashMap<String, Patch>,
    soundfont: Result<PathBuf, String>,
) -> Result<ExportReport, String> {
    let started = Instant::now();
    let name = sanitize_name(&request.name);
    if name.is_empty() {
        return Err("name must contain at least one letter, digit, '_' or '-'".to_string());
    }

    let translator = Translator::new(soundfont.clone().map(|_| ()));
    let wet = translator.translate_parts(request.sequence.clone(), session_patches, Effects::Wet)?;
    if wet.midi.is_empty() && wet.patches.is_empty() && wet.r2d2.is_empty() {
        return Err("Nothing to export: the sequence has no notes".to_string());
    }
    let duration = wet.duration;
    let frames = (duration.as_secs_f64() * SAMPLE_RATE as f64).ceil() as usize;
    let parts = match request.split {
        Split::Tracks => {
            translator.translate_parts(request.sequence.clone(), session_patches, Effects::Dry)?
        }
        Split::Mixdown | Split::Stems => wet,
    };

    let targets = plan_targets(&request.dir, &name, request.split, &parts)?;
    check_collisions(&targets, request.overwrite)?;
    for target in &targets {
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }

    let synth = if parts.midi.is_empty() {
        None
    } else {
        Some(load_synth(&soundfont?)?)
    };
    let (mut engine, _handle) = MidiEngine::new(synth);

    let mut files = Vec::with_capacity(targets.len());
    match request.split {
        Split::Mixdown => {
            let target = targets.into_iter().next().expect("mixdown has one target");
            let mut mix = render_offline(&mut engine, parts.into_command(PlayMode::Replace), frames);
            for frame in &mut mix {
                frame[0] = soft_clip(frame[0]);
                frame[1] = soft_clip(frame[1]);
            }
            let peak = write_wav(&target.path, &mix, request.bit_depth)?;
            files.push(ExportedFile {
                path: target.path,
                peak,
            });
        }
        Split::Stems | Split::Tracks => {
            return Err(format!("split {} is not implemented yet", request.split.as_str()));
        }
    }

    let report = ExportReport {
        files,
        duration,
        render_time: started.elapsed(),
    };
    tracing::info!(
        "Exported {} file(s) ({}) to {} in {:.2}s",
        report.files.len(),
        request.split.as_str(),
        request.dir.display(),
        report.render_time.as_secs_f64()
    );
    Ok(report)
}

/// Every file the request will write, derived before anything is rendered.
fn plan_targets(
    dir: &Path,
    name: &str,
    split: Split,
    _parts: &TranslatedParts,
) -> Result<Vec<Target>, String> {
    match split {
        Split::Mixdown => Ok(vec![Target {
            path: dir.join(format!("{}.wav", name)),
            source: Source::Mixdown,
        }]),
        Split::Stems | Split::Tracks => Err(format!("split {} is not implemented yet", split.as_str())),
    }
}

fn check_collisions(targets: &[Target], overwrite: bool) -> Result<(), String> {
    if overwrite {
        return Ok(());
    }
    let existing: Vec<String> = targets
        .iter()
        .filter(|t| t.path.exists())
        .map(|t| t.path.display().to_string())
        .collect();
    if existing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Refusing to overwrite existing files (pass \"overwrite\": true to replace them): {}",
            existing.join(", ")
        ))
    }
}

/// Render `frames` frames of `command` from the engine's current (quiet)
/// state and drop the engine's lead-in. Unclipped; the caller decides.
fn render_offline(engine: &mut MidiEngine, command: PlayCommand, frames: usize) -> Vec<[f32; 2]> {
    engine.apply(EngineCommand::Play(PlayCommand {
        mode: PlayMode::Replace,
        ..command
    }));
    let total = LEAD_FRAMES as usize + frames;
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(total + CHUNK_FRAMES);
    let (mut left, mut right) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
    while out.len() < total {
        engine.render_unclipped(&mut left, &mut right);
        out.extend(left.iter().zip(&right).map(|(&l, &r)| [l, r]));
    }
    out.drain(..LEAD_FRAMES as usize);
    out.truncate(frames);
    out
}

/// Write stereo frames; returns the unclamped peak.
fn write_wav(path: &Path, frames: &[[f32; 2]], depth: BitDepth) -> Result<f32, String> {
    let peak = frames
        .iter()
        .flat_map(|f| f.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let mut writer = hound::WavWriter::create(path, depth.spec())
        .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;
    let samples = frames.iter().flat_map(|f| f.iter().copied());
    let result: Result<(), hound::Error> = match depth {
        BitDepth::Float32 => samples.map(|v| writer.write_sample(v)).collect(),
        BitDepth::Int16 => samples
            .map(|v| writer.write_sample((v.clamp(-1.0, 1.0) * 32_767.0).round() as i16))
            .collect(),
        BitDepth::Int24 => samples
            .map(|v| writer.write_sample((v.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32))
            .collect(),
    };
    result
        .and_then(|()| writer.finalize())
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(peak)
}
```

Notes for the implementer:
- `Target.source` is unused until Task 3 adds the other variants; if clippy flags `dead_code` on the field, add `#[allow(dead_code)] // read by the split renderers in the next commit` on the `source` field and remove it in Task 3.
- `render_unclipped` is `pub(crate)` on `MidiEngine` and `MidiEngine::new` returns `(Self, EngineHandle)`; the handle is kept alive in `_handle` so the command channel stays open.
- `SimpleSequence` must be `Clone` for `request.sequence.clone()`; it derives `Clone` in `src/midi/mod.rs`.

- [ ] **Step 5: Run the export tests**

Run: `cargo test --bin mcp-muse export:: 2>&1 | tail -20`
Expected: all 7 tests pass.

- [ ] **Step 6: Full suite, fmt and clippy**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5 && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: clean; both suites ok.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/midi/mod.rs src/midi/export.rs
git commit -m "Add offline WAV export of a stereo mixdown

A private MidiEngine renders the translated composition without an audio
device; the file is soft-clipped like playback and written with hound at
16, 24 or 32-bit float. Names are sanitized and existing files are kept
unless overwrite is set.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01ED7wujQfBxcx4T4eGirX9i"
```

---

### Task 3: Stems and tracks

**Files:**
- Modify: `src/midi/export.rs` (replace the `Source` enum, `plan_targets`, and the `Stems | Tracks` arm of `export_with`)

**Interfaces:**
- Consumes: `TranslatedParts` fields (`midi: Vec<MidiNote>`, `midi_effects`, `patches: Vec<PatchRender { name, start, samples }>`, `r2d2`, `tempo`), `crate::midi::translate::midi_events(&[MidiNote]) -> Vec<(u64, EventKind)>` (already `pub(crate)`), `crate::midi::gm_names::GM_INSTRUMENTS`.
- Produces: `Split::Stems` and `Split::Tracks` fully working; private `fn sources(parts: &TranslatedParts) -> Vec<(String, Source)>` and `fn channel_name(channel: u8, program: u8) -> String`.

- [ ] **Step 1: Write the failing tests**

Append inside the `mod tests` block of `src/midi/export.rs`:

```rust
    fn parts_with(midi: Vec<crate::midi::parser::MidiNote>, patch_names: &[&str], r2d2: bool) -> TranslatedParts {
        TranslatedParts {
            midi,
            midi_effects: None,
            patches: patch_names
                .iter()
                .map(|n| crate::midi::translate::PatchRender {
                    name: n.to_string(),
                    start: 0,
                    samples: vec![[0.1, 0.1]; 10],
                })
                .collect(),
            r2d2: if r2d2 { vec![(0, vec![[0.2, 0.2]; 10])] } else { Vec::new() },
            tempo: 120,
            duration: Duration::from_secs(1),
        }
    }

    fn parsed_midi(channel: u8, instrument: Option<u8>, start: f64) -> crate::midi::parser::MidiNote {
        crate::midi::parser::MidiNote {
            note: 60,
            velocity: 100,
            channel,
            start_time: Duration::from_secs_f64(start),
            duration: Duration::from_secs(1),
            instrument,
            reverb: None,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
        }
    }

    #[test]
    fn sources_are_named_after_channel_program_patch_and_r2d2() {
        let parts = parts_with(
            vec![
                parsed_midi(3, Some(73), 1.0),
                parsed_midi(3, Some(0), 2.0),
                parsed_midi(0, None, 0.0),
                parsed_midi(9, None, 0.0),
            ],
            &["blip", "blip", "tr_808_kick"],
            true,
        );
        let names: Vec<String> = sources(&parts).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec![
                "ch00_acoustic_grand_piano",
                "ch03_flute",
                "ch09_drums",
                "synth_blip",
                "synth_blip_2",
                "synth_tr_808_kick",
                "r2d2",
            ]
        );
    }

    #[test]
    fn stems_sum_to_the_mixdown_and_share_its_length() {
        let dir = tmp_dir("stems");
        let mut a = sine_note(0.0, 0.4, 0.3);
        let b = sine_note(0.2, 0.4, 0.3);
        // Two different patches so they become two stems.
        a.synth = Some(
            serde_json::from_value(serde_json::json!({
                "name": "square_quiet", "level": 0.3,
                "subtractive": {"osc1": {"wave": "square"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}
            }))
            .unwrap(),
        );
        let notes = || vec![a.clone(), b.clone()];
        let mix = export_with(
            request(notes(), &dir, "mix", Split::Mixdown, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let stems = export_with(
            request(notes(), &dir, "stems", Split::Stems, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();

        assert_eq!(stems.files.len(), 2);
        assert_eq!(stems.duration, mix.duration);
        let names: Vec<String> = stems
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["synth_square_quiet.wav", "synth_sine_30.wav"]);
        assert!(stems.files[0].path.starts_with(dir.join("stems")));

        let (_, mixed) = read_wav(&mix.files[0].path);
        let (_, s0) = read_wav(&stems.files[0].path);
        let (_, s1) = read_wav(&stems.files[1].path);
        assert_eq!(s0.len(), mixed.len());
        assert_eq!(s1.len(), mixed.len());
        // Peaks stay below the soft-clip knee, so the mixdown is the plain sum.
        assert!(mix.files[0].peak < 0.8);
        let worst = mixed
            .iter()
            .zip(&s0)
            .zip(&s1)
            .map(|((m, x), y)| (m[0] - (x[0] + y[0])).abs().max((m[1] - (x[1] + y[1])).abs()))
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "stems must sum to the mixdown, worst diff {}", worst);
    }

    #[test]
    fn r2d2_notes_become_one_stem() {
        let dir = tmp_dir("r2d2");
        let r2d2 = SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(0.5),
            ..Default::default()
        };
        let report = export_with(
            request(
                vec![r2d2.clone(), r2d2],
                &dir,
                "beeps",
                Split::Stems,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, dir.join("beeps").join("r2d2.wav"));
        let (_, frames) = read_wav(&report.files[0].path);
        assert!(rms_left(&frames, 0.0, 0.4) > 0.01);
    }

    #[test]
    fn tracks_bypass_the_patch_effect_chain_but_keep_the_stem_length() {
        let dir = tmp_dir("tracks");
        let mut note = sine_note(0.0, 0.2, 0.8);
        note.synth = Some(
            serde_json::from_value(serde_json::json!({
                "name": "echo", "level": 0.8,
                "subtractive": {"osc1": {"wave": "sine"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
                "effects": [{"type": "delay", "delay_time": 0.5, "feedback": 0.5, "intensity": 0.8}]
            }))
            .unwrap(),
        );
        let wet = export_with(
            request(vec![note.clone()], &dir, "wet", Split::Stems, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let dry = export_with(
            request(vec![note], &dir, "dry", Split::Tracks, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let (_, wet_frames) = read_wav(&wet.files[0].path);
        let (_, dry_frames) = read_wav(&dry.files[0].path);
        assert_eq!(dry_frames.len(), wet_frames.len(), "tracks are padded to the wet length");
        // The first echo lands at 0.5 s; a dry track has nothing there.
        assert!(rms_left(&wet_frames, 1.0, 1.2) > 1e-3, "the stem carries the delay repeats");
        assert!(rms_left(&dry_frames, 1.0, 1.2) < 1e-5, "the track does not");
        assert!(rms_left(&dry_frames, 0.02, 0.18) > 0.05, "the track still has the note");
    }

    #[test]
    fn midi_channels_become_separate_stems() {
        let Ok(soundfont) = find_soundfont() else {
            eprintln!("skipping: SoundFont not installed (run `mcp-muse setup`)");
            return;
        };
        let dir = tmp_dir("channels");
        let report = export_with(
            request(
                vec![
                    midi_note(0, 76, Some(73)), // flute E5, 659.26 Hz
                    midi_note(1, 67, Some(73)), // flute G4, 392.00 Hz
                    midi_note(9, 38, None),     // snare on the drum channel
                ],
                &dir,
                "band",
                Split::Stems,
                BitDepth::Float32,
            ),
            &HashMap::new(),
            Ok(soundfont),
        )
        .unwrap();
        let names: Vec<String> = report
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["ch00_flute.wav", "ch01_flute.wav", "ch09_drums.wav"]);

        use crate::expressive::test_util::goertzel_power;
        let window = |frames: &[[f32; 2]]| -> Vec<f32> {
            let a = (0.1 * SAMPLE_RATE as f64) as usize;
            let b = (0.9 * SAMPLE_RATE as f64) as usize;
            frames[a..b].iter().map(|f| f[0]).collect()
        };
        let (_, ch0) = read_wav(&report.files[0].path);
        let (_, ch1) = read_wav(&report.files[1].path);
        let (e5, g4) = (659.26, 392.0);
        let sr = SAMPLE_RATE as f32;
        let ch0 = window(&ch0);
        let ch1 = window(&ch1);
        assert!(
            goertzel_power(&ch0, e5, sr) > 10.0 * goertzel_power(&ch0, g4, sr),
            "channel 0 carries only its own pitch"
        );
        assert!(
            goertzel_power(&ch1, g4, sr) > 10.0 * goertzel_power(&ch1, e5, sr),
            "channel 1 carries only its own pitch"
        );
        let (_, drums) = read_wav(&report.files[2].path);
        assert!(rms_left(&drums, 0.0, 0.2) > 0.01, "the snare hit is on the drum stem");
    }
```

`sine_note` names its patch from `level` (`sine_30` for 0.3), which the stems test relies on.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse export:: 2>&1 | tail -30`
Expected: compile error `cannot find function sources`; after stubbing, the split tests would fail with "not implemented yet".

- [ ] **Step 3: Implement the split renderers**

In `src/midi/export.rs`, extend the imports:

```rust
use crate::midi::gm_names::GM_INSTRUMENTS;
use crate::midi::parser::MidiNote;
use crate::midi::translate::midi_events;
```

Replace the `Source` enum:

```rust
/// What goes into one target file.
enum Source {
    /// Everything, soft-clipped.
    Mixdown,
    /// One MIDI channel's events plus the bus chain (none for tracks).
    Channel(u8),
    /// Index into `TranslatedParts::patches`.
    Patch(usize),
    /// Every R2D2 note summed.
    R2d2,
}
```

Replace `plan_targets`:

```rust
/// Every file the request will write, derived before anything is rendered.
fn plan_targets(
    dir: &Path,
    name: &str,
    split: Split,
    parts: &TranslatedParts,
) -> Result<Vec<Target>, String> {
    Ok(match split {
        Split::Mixdown => vec![Target {
            path: dir.join(format!("{}.wav", name)),
            source: Source::Mixdown,
        }],
        Split::Stems | Split::Tracks => {
            let folder = dir.join(name);
            sources(parts)
                .into_iter()
                .map(|(source_name, source)| Target {
                    path: folder.join(format!("{}.wav", source_name)),
                    source,
                })
                .collect()
        }
    })
}

/// The split's sources in a stable order: MIDI channels ascending, then
/// patch groups in translation order, then R2D2. Names that repeat (two
/// inline patches with the same name) get `_2`, `_3`, ... appended.
fn sources(parts: &TranslatedParts) -> Vec<(String, Source)> {
    let mut out: Vec<(String, Source)> = Vec::new();
    let mut channels: Vec<u8> = parts.midi.iter().map(|n| n.channel).collect();
    channels.sort_unstable();
    channels.dedup();
    for channel in channels {
        let first = parts
            .midi
            .iter()
            .filter(|n| n.channel == channel)
            .min_by_key(|n| n.start_time)
            .expect("channel has notes");
        out.push((
            channel_name(channel, first.instrument.unwrap_or(0)),
            Source::Channel(channel),
        ));
    }
    for (i, patch) in parts.patches.iter().enumerate() {
        out.push((format!("synth_{}", sanitize_name(&patch.name)), Source::Patch(i)));
    }
    if !parts.r2d2.is_empty() {
        out.push(("r2d2".to_string(), Source::R2d2));
    }
    dedupe_names(&mut out);
    out
}

fn dedupe_names(sources: &mut [(String, Source)]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (name, _) in sources.iter_mut() {
        let count = seen.entry(name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            *name = format!("{}_{}", name, count);
        }
    }
}

/// `ch09_drums` for the drum channel, else `ch<NN>_<gm program name>`.
fn channel_name(channel: u8, program: u8) -> String {
    if channel == 9 {
        return "ch09_drums".to_string();
    }
    let gm = GM_INSTRUMENTS[(program as usize).min(GM_INSTRUMENTS.len() - 1)];
    format!("ch{:02}_{}", channel, sanitize_name(gm).to_lowercase())
}
```

Replace the `Split::Stems | Split::Tracks` arm in `export_with`:

```rust
        Split::Stems | Split::Tracks => {
            for target in targets {
                let samples = match target.source {
                    Source::Mixdown => unreachable!("splits never plan a mixdown target"),
                    Source::Channel(channel) => {
                        let notes: Vec<MidiNote> = parts
                            .midi
                            .iter()
                            .filter(|n| n.channel == channel)
                            .cloned()
                            .collect();
                        render_offline(
                            &mut engine,
                            PlayCommand {
                                events: midi_events(&notes),
                                buffers: Vec::new(),
                                midi_effects: parts.midi_effects.clone(),
                                mode: PlayMode::Replace,
                                tempo: parts.tempo,
                            },
                            frames,
                        )
                    }
                    Source::Patch(i) => {
                        let patch = &parts.patches[i];
                        place(&[(patch.start, patch.samples.as_slice())], frames)
                    }
                    Source::R2d2 => {
                        let buffers: Vec<(u64, &[[f32; 2]])> = parts
                            .r2d2
                            .iter()
                            .map(|(start, samples)| (*start, samples.as_slice()))
                            .collect();
                        place(&buffers, frames)
                    }
                };
                let peak = write_wav(&target.path, &samples, request.bit_depth)?;
                files.push(ExportedFile {
                    path: target.path,
                    peak,
                });
            }
        }
```

Add the placement helper after `render_offline`:

```rust
/// Sum pre-rendered buffers into a zeroed buffer of `frames` frames at their
/// start offsets; anything past the end is dropped.
fn place(buffers: &[(u64, &[[f32; 2]])], frames: usize) -> Vec<[f32; 2]> {
    let mut out = vec![[0.0f32; 2]; frames];
    for (start, samples) in buffers {
        let start = *start as usize;
        for (slot, s) in out.iter_mut().skip(start).zip(samples.iter()) {
            slot[0] += s[0];
            slot[1] += s[1];
        }
    }
    out
}
```

Remove any `#[allow(dead_code)]` added in Task 2.

If `GM_INSTRUMENTS` contains names with characters other than letters, digits, spaces and `_`/`-` (for example `Lead 1 (square)`), `sanitize_name` turns each into `_`; that is the intended, documented behaviour.

- [ ] **Step 4: Run the export tests**

Run: `cargo test --bin mcp-muse export:: 2>&1 | tail -30`
Expected: all pass (the `midi_channels_become_separate_stems` test runs because the SoundFont is symlinked in the worktree; it prints a skip message elsewhere).

If `stems_sum_to_the_mixdown_and_share_its_length` fails on the tolerance, check that `mix.files[0].peak` is below 0.8; if a square wave at level 0.3 pushes the sum over the knee, lower both levels to 0.2 (and the expected stem name to `synth_sine_20`).

- [ ] **Step 5: Full suite, fmt and clippy**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5 && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: clean; both suites ok.

- [ ] **Step 6: Commit**

```bash
git add src/midi/export.rs
git commit -m "Export stems and dry tracks, one file per sound source

Each MIDI channel renders in its own offline pass; patch and R2D2 buffers
are placed at their offsets. Stems keep every chain and sum to the mix,
tracks are translated dry and padded to the same length.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01ED7wujQfBxcx4T4eGirX9i"
```

---

### Task 4: The `export_audio` tool

**Files:**
- Modify: `src/server/mcp.rs` (imports at top; `handle_tools_list` around lines 548–840; `dispatch_tool` around line 890; `handle_play_sequence` around line 1156; the tests module)
- Modify: `tests/integration/mcp_protocol.rs:119` (tool count) and the `tools_list_has_seven_tools...` unit test
- Create: `tests/integration/export_audio.rs`
- Modify: `tests/integration/mod.rs`

**Interfaces:**
- Consumes: `crate::midi::export::{export, sanitize_name, BitDepth, ExportReport, ExportRequest, Split}` (Tasks 2–3).
- Produces: tool `export_audio`; private `pattern_reference_schema()`, `ExportOptions`, `resolve_sequence_arguments`, `handle_export_audio`, `export_finished_text`.

- [ ] **Step 1: Write the failing unit tests**

In the `mod tests` of `src/server/mcp.rs`, rename `tools_list_has_seven_tools_and_the_note_schema_has_synth` to `tools_list_has_eight_tools_and_the_note_schema_has_synth`, change `assert_eq!(tools.as_array().unwrap().len(), 7);` to `8`, and add after the `names.contains(&"define_synth")` assertion:

```rust
        assert!(names.contains(&"export_audio"));
        let export = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "export_audio")
            .unwrap();
        let props = &export["inputSchema"]["properties"];
        assert!(props["path"].is_object());
        assert_eq!(props["split"]["enum"], json!(["mixdown", "stems", "tracks"]));
        assert_eq!(props["bit_depth"]["enum"], json!([16, 24, 32]));
        assert!(props["patterns"]["items"]["properties"]["pattern_name"].is_object());
        assert!(props.get("mode").is_none(), "an export has no play mode");
        assert_eq!(export["inputSchema"]["required"], json!(["path"]));
```

Then add these tests:

```rust
    fn export_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcp-muse-mcp-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn blip() -> Value {
        json!({"synth": {"name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}},
               "note": 84, "duration": 0.1})
    }

    #[test]
    fn export_audio_rejects_bad_options_as_invalid_params() {
        let mut state = ServerState::new();
        let dir = export_dir("bad").to_string_lossy().into_owned();
        for (args, needle) in [
            (json!({"notes": [blip()], "path": "relative/dir"}), "absolute"),
            (json!({"notes": [blip()], "path": dir, "bit_depth": 20}), "bit_depth"),
            (json!({"notes": [blip()], "path": dir, "split": "loud"}), "split"),
            (json!({"notes": [blip()], "path": dir, "name": "  "}), "name"),
            (json!({"notes": [blip()]}), "path"),
        ] {
            let r = call(&mut state, "export_audio", args);
            let err = r.error.as_ref().unwrap_or_else(|| panic!("expected -32602 for {}", needle));
            assert_eq!(err.code, INVALID_PARAMS, "{}", needle);
            assert!(err.message.contains(needle), "{} not in {}", needle, err.message);
        }
    }

    #[test]
    fn export_audio_writes_a_mixdown_and_reports_the_path() {
        let mut state = ServerState::new();
        let dir = export_dir("mix");
        let r = call(
            &mut state,
            "export_audio",
            json!({"notes": [blip()], "path": dir.to_string_lossy(), "name": "Demo Take"}),
        );
        let t = text(&r);
        assert!(r.result.as_ref().unwrap().get("isError").is_none(), "{}", t);
        let expected = dir.join("Demo_Take.wav");
        assert!(t.contains(&expected.display().to_string()), "{}", t);
        assert!(t.contains("mixdown"), "{}", t);
        assert!(expected.exists());
    }

    #[test]
    fn export_audio_reports_runtime_failures_as_tool_errors() {
        let mut state = ServerState::new();
        let dir = export_dir("toolerr");
        let r = call(
            &mut state,
            "export_audio",
            json!({"patterns": [{"pattern_name": "nope"}], "path": dir.to_string_lossy()}),
        );
        assert_eq!(r.result.as_ref().unwrap()["isError"], json!(true));
        assert!(text(&r).contains("nope"));

        let r = call(
            &mut state,
            "export_audio",
            json!({"notes": [{"synth": "no_such_patch", "note": 60}], "path": dir.to_string_lossy()}),
        );
        assert_eq!(r.result.as_ref().unwrap()["isError"], json!(true));
        assert!(text(&r).contains("no_such_patch"));
    }

    #[test]
    fn export_audio_names_hot_files_when_they_are_clamped() {
        let report = ExportReport {
            files: vec![
                ExportedFile {
                    path: "/tmp/x/a.wav".into(),
                    peak: 1.3,
                },
                ExportedFile {
                    path: "/tmp/x/b.wav".into(),
                    peak: 0.5,
                },
            ],
            duration: Duration::from_secs_f64(3.3),
            render_time: Duration::from_millis(400),
        };
        let t = export_finished_text(&report, Split::Stems, BitDepth::Int24);
        assert!(t.contains("2 stems"), "{}", t);
        assert!(t.contains("/tmp/x/a.wav"), "{}", t);
        assert!(t.contains("Duration 3.3 s"), "{}", t);
        let clamped = t.lines().last().unwrap();
        assert!(clamped.contains("Clamped") && clamped.contains("a.wav"), "{}", t);
        assert!(clamped.contains("bit_depth"), "{}", t);
        assert!(!clamped.contains("b.wav"), "quiet files are not listed as clamped: {}", t);
        let f = export_finished_text(&report, Split::Stems, BitDepth::Float32);
        assert!(!f.contains("Clamped"), "float never clamps: {}", f);
    }
```

`JsonRpcError` needs its `code` and `message` fields readable from the tests module; they are private fields of a struct in the same module, so `err.code` works from `mod tests` (child modules see private items).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse server::mcp::tests 2>&1 | tail -20`
Expected: compile errors (`export_finished_text` not found, `ExportReport` not imported) and the eight-tools assertion failing once they compile.

- [ ] **Step 3: Extract the pattern reference schema and add the tool**

At the top of `src/server/mcp.rs`, extend the `crate::midi` import:

```rust
use crate::midi::export::{BitDepth, ExportReport, ExportRequest, Split, export, sanitize_name};
```

Add a function next to `note_schema()`:

```rust
/// Schema of one entry in a `patterns` array (shared by play_sequence and export_audio).
fn pattern_reference_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            ...the exact object currently inlined as play_sequence's "patterns" → "items" (from "pattern_name" through "align_to_bars")...
        },
        "required": ["pattern_name"]
    })
}
```

Move the whole `items` object of `play_sequence`'s `patterns` property into that function verbatim (it begins `"type": "object", "properties": { "pattern_name": {...}` and ends `"required": ["pattern_name"]`), and in the `play_sequence` definition write `"items": pattern_reference_schema()`.

Add the eighth tool at the end of the `tools` array in `handle_tools_list` (after `play_notes`):

```rust
        {
            "name": "export_audio",
            "description": "Save a composition to disk as WAV instead of playing it. Takes the same notes, patterns, tempo and beats_per_bar as play_sequence and renders offline (nothing is heard, live playback is untouched). split selects what is written: \"mixdown\" (default) is one stereo file; \"stems\" is one file per sound source (each MIDI channel, each synth patch, all R2D2 together) with every effect baked in so they sum back to the mix; \"tracks\" is the same split with the MIDI bus, patch and R2D2 effect chains bypassed, for mixing elsewhere. Every file in one export has the same length so they line up at zero in a DAW.

Example: {\"patterns\": [{\"pattern_name\": \"drums\", \"start_bar\": 1, \"repeat_count\": 4}], \"path\": \"/Users/me/Music/demo\", \"name\": \"take1\", \"split\": \"stems\"} writes /Users/me/Music/demo/take1/ch09_drums.wav and friends.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notes": {
                        "type": "array",
                        "description": "🎵 Individual notes (same format as play_notes)",
                        "items": note_schema()
                    },
                    "patterns": {
                        "type": "array",
                        "description": "🎼 Pattern references with transformations (same format as play_sequence)",
                        "items": pattern_reference_schema()
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Tempo in BPM for the entire sequence",
                        "minimum": 20,
                        "maximum": 300,
                        "default": 120
                    },
                    "beats_per_bar": {
                        "type": "integer",
                        "description": "🎶 Time signature numerator used to convert musical_time and musical_duration (4 for 4/4, 3 for 3/4)",
                        "minimum": 2,
                        "maximum": 8,
                        "default": 4
                    },
                    "path": {
                        "type": "string",
                        "description": "📁 Absolute directory to write into; created if missing. A mixdown is written as <path>/<name>.wav, stems and tracks as <path>/<name>/<source>.wav."
                    },
                    "name": {
                        "type": "string",
                        "description": "🏷️ Base file or folder name. Letters, digits, '_' and '-' are kept; anything else becomes '_'.",
                        "default": "mix"
                    },
                    "split": {
                        "type": "string",
                        "enum": ["mixdown", "stems", "tracks"],
                        "default": "mixdown",
                        "description": "mixdown: one soft-clipped stereo file. stems: one file per source with effects, not clipped. tracks: one file per source with the effect chains bypassed, not clipped."
                    },
                    "bit_depth": {
                        "type": "integer",
                        "enum": [16, 24, 32],
                        "default": 24,
                        "description": "WAV sample format: 16 or 24-bit integer (peaks above 0 dBFS are clamped and reported) or 32 for IEEE float (never clamps)."
                    },
                    "overwrite": {
                        "type": "boolean",
                        "default": false,
                        "description": "Replace files that already exist. Without it an existing target is an error listing the collisions."
                    }
                },
                "required": ["path"]
            }
        }
```

In `dispatch_tool`, add before the `other =>` arm:

```rust
        "export_audio" => handle_export_audio(state, tool_params.arguments, id),
```

- [ ] **Step 4: Extract sequence resolution and add the handler**

Replace `handle_play_sequence` with:

```rust
/// The shared front half of play_sequence and export_audio: parse the
/// notes/patterns arguments, validate, resolve patterns against the session
/// and describe the result. `Err` carries the response to send back.
fn resolve_sequence_arguments(
    state: &ServerState,
    arguments: Value,
    id: &Option<Value>,
) -> Result<(SimpleSequence, String), JsonRpcResponse> {
    let extended: ExtendedSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            return Err(JsonRpcResponse::error(
                id.clone(),
                INVALID_PARAMS,
                format!("Failed to parse sequence: {}", e),
            ));
        }
    };
    if extended.notes.is_empty() && extended.patterns.is_empty() {
        return Err(JsonRpcResponse::error(
            id.clone(),
            INVALID_PARAMS,
            "Sequence must contain either notes or pattern references",
        ));
    }
    if let Err(e) = validate_tempo(extended.tempo) {
        return Err(JsonRpcResponse::error(id.clone(), INVALID_PARAMS, e));
    }
    if let Err(e) = validate_notes(&extended.notes) {
        return Err(JsonRpcResponse::error(id.clone(), INVALID_PARAMS, e));
    }

    let resolved = match extended.resolve_patterns(&state.patterns) {
        Ok(seq) => seq,
        Err(e) => {
            let known: Vec<&String> = state.patterns.keys().collect();
            return Err(JsonRpcResponse::tool_error(
                id.clone(),
                format!("{}. Defined patterns: {:?}", e, known),
            ));
        }
    };
    if resolved.notes.is_empty() {
        return Err(JsonRpcResponse::tool_error(
            id.clone(),
            "Resolved sequence contains no notes",
        ));
    }

    let summary = format!(
        "{} pattern references + {} individual notes → {} notes: {}",
        extended.patterns.len(),
        extended.notes.len(),
        resolved.notes.len(),
        describe_sources(&resolved.notes)
    );
    Ok((resolved, summary))
}

fn handle_play_sequence(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let mode = match parse_mode(&arguments) {
        Ok(mode) => mode,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
    let (resolved, summary) = match resolve_sequence_arguments(state, arguments, &id) {
        Ok(r) => r,
        Err(response) => return response,
    };
    start_playback(state, resolved, mode, id, summary)
}

/// Output options of export_audio. The sequence fields in the same object
/// are parsed separately by `resolve_sequence_arguments`.
#[derive(Debug, Deserialize)]
struct ExportOptions {
    path: String,
    #[serde(default = "default_export_name")]
    name: String,
    #[serde(default)]
    split: Split,
    #[serde(default = "default_bit_depth")]
    bit_depth: u32,
    #[serde(default)]
    overwrite: bool,
}

fn default_export_name() -> String {
    "mix".to_string()
}

fn default_bit_depth() -> u32 {
    24
}

fn handle_export_audio(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let options: ExportOptions = match serde_json::from_value(arguments.clone()) {
        Ok(o) => o,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Invalid export options: {} (split must be mixdown|stems|tracks, path is required)", e),
            );
        }
    };
    let dir = std::path::PathBuf::from(&options.path);
    if !dir.is_absolute() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            format!("path must be an absolute directory, got {:?}", options.path),
        );
    }
    if sanitize_name(&options.name).is_empty() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            "name must contain at least one letter, digit, '_' or '-'",
        );
    }
    let bit_depth = match BitDepth::from_bits(options.bit_depth) {
        Ok(b) => b,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
    let (sequence, summary) = match resolve_sequence_arguments(state, arguments, &id) {
        Ok(r) => r,
        Err(response) => return response,
    };

    let request = ExportRequest {
        sequence,
        dir,
        name: options.name,
        split: options.split,
        bit_depth,
        overwrite: options.overwrite,
    };
    match export(request, &state.synths) {
        Ok(report) => {
            tracing::info!("Export finished ({}): {}", options.split.as_str(), summary);
            JsonRpcResponse::tool_text(id, export_finished_text(&report, options.split, bit_depth))
        }
        Err(e) => {
            tracing::error!("Export failed: {}", e);
            JsonRpcResponse::tool_error(id, format!("Export failed: {}", e))
        }
    }
}

fn export_finished_text(report: &ExportReport, split: Split, bit_depth: BitDepth) -> String {
    let what = match split {
        Split::Mixdown => "stereo mixdown",
        Split::Stems => "stems",
        Split::Tracks => "dry tracks",
    };
    let mut out = format!("💾 Exported {} {}:\n", report.files.len(), what);
    for file in &report.files {
        out.push_str(&format!("  {}\n", file.path.display()));
    }
    out.push_str(&format!(
        "Duration {:.1} s (including effect tails), rendered in {:.1} s.",
        report.duration.as_secs_f64(),
        report.render_time.as_secs_f64()
    ));
    if bit_depth != BitDepth::Float32 {
        let hot: Vec<String> = report
            .files
            .iter()
            .filter(|f| f.peak > 1.0)
            .map(|f| {
                f.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.path.display().to_string())
            })
            .collect();
        if !hot.is_empty() {
            out.push_str(&format!(
                "\n⚠️ Clamped at 0 dBFS: {}. Pass \"bit_depth\": 32 to keep the full range.",
                hot.join(", ")
            ));
        }
    }
    out
}
```

The tests module needs `use crate::midi::export::ExportedFile;` alongside its existing imports (add it at the top of `mod tests`).

- [ ] **Step 5: Run the server unit tests**

Run: `cargo test --bin mcp-muse server::mcp::tests 2>&1 | tail -20`
Expected: all pass.

- [ ] **Step 6: Update and add integration tests**

In `tests/integration/mcp_protocol.rs`, change `assert_eq!(tools.len(), 7);` (around line 119) to `assert_eq!(tools.len(), 8);` and add `assert!(tool_names.contains(&"export_audio"));` after the `list_sounds` assertion.

Create `tests/integration/export_audio.rs`:

```rust
//! export_audio through the real binary: a synth note needs no SoundFont and
//! no audio device, so this runs everywhere CI does.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
#[allow(clippy::zombie_processes)]
fn export_audio_writes_a_wav_file() {
    let dir = std::env::temp_dir().join(format!(
        "mcp-muse-integration-export-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut call = |message: Value| -> Value {
        writeln!(stdin, "{}", message).unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("JSON response")
    };

    call(json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
    }));
    let r = call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "export_audio", "arguments": {
            "notes": [{"synth": {"name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}}, "note": 84, "duration": 0.1}],
            "path": dir.to_string_lossy(),
            "name": "blip",
            "bit_depth": 16
        }}
    }));
    assert_eq!(r["id"], 1);
    assert!(r["result"].get("isError").is_none(), "{}", r);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let expected = dir.join("blip.wav");
    assert!(text.contains(&expected.display().to_string()), "{}", text);

    let mut header = [0u8; 12];
    std::fs::File::open(&expected)
        .unwrap()
        .read_exact(&mut header)
        .unwrap();
    assert_eq!(&header[0..4], b"RIFF");
    assert_eq!(&header[8..12], b"WAVE");

    child.kill().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
```

In `tests/integration/mod.rs` add `pub mod export_audio;`.

- [ ] **Step 7: Run everything, fmt and clippy**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5 && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: clean; both suites ok (integration tests spawn `cargo run`, so the first one is slow).

- [ ] **Step 8: Commit**

```bash
git add src/server/mcp.rs tests/integration/mcp_protocol.rs tests/integration/export_audio.rs tests/integration/mod.rs
git commit -m "Add the export_audio tool

Same notes, patterns, tempo and beats_per_bar as play_sequence plus path,
name, split, bit_depth and overwrite; renders offline through the export
module and never opens the audio device. The pattern reference schema and
sequence resolution are shared with play_sequence.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01ED7wujQfBxcx4T4eGirX9i"
```

---

### Task 5: Documentation and final verification

**Files:**
- Modify: `CLAUDE.md` (MCP Server section around lines 36–47; Audio pipeline section; add an Export subsection)
- Modify: `README.md` (features bullet at line 83; new section after "Synth Patches", before "Universal Mixed Mode Examples" around line 332)
- Modify: `docs/superpowers/specs/2026-09-09-audio-export-design.md` (Status line)

- [ ] **Step 1: Update CLAUDE.md**

Change `Seven tools:` to `Eight tools:` and add after the `stop_playback` bullet:

```markdown
- `export_audio` - render a composition offline and write WAV files: `split: mixdown|stems|tracks`, a required absolute `path`, `name`, `bit_depth: 16|24|32` and `overwrite`; never opens the audio device
```

Add a new subsection after the "Audio pipeline" section:

```markdown
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
```

- [ ] **Step 2: Update README.md**

Change the features bullet to:

```markdown
- 🔌 **Eight Focused Tools**: `play_notes`, `define_sequence_pattern`, `play_sequence`, `list_patterns`, `define_synth`, `list_sounds`, `stop_playback`, `export_audio`
```

Add a section before `## 🎭 **Universal Mixed Mode Examples (All Systems Together)**`:

```markdown
## 💾 Saving Audio to Disk

`export_audio` takes the same `notes`, `patterns`, `tempo` and `beats_per_bar` as `play_sequence`, renders offline (nothing plays) and writes 44.1 kHz stereo WAV:

```json
{
  "patterns": [{"pattern_name": "drums", "start_bar": 1, "repeat_count": 4}],
  "notes": [{"synth": "minimoog_bass", "note": 36, "musical_time": "1.1.0", "musical_duration": "1/4"}],
  "path": "/Users/me/Music/demo",
  "name": "take1",
  "split": "stems",
  "bit_depth": 24
}
```

- `split: "mixdown"` (default) writes `<path>/<name>.wav`, soft-clipped like playback.
- `split: "stems"` writes `<path>/<name>/<source>.wav`, one per MIDI channel (`ch09_drums`, `ch00_acoustic_grand_piano`), per synth patch (`synth_minimoog_bass`) and one `r2d2`, with every effect baked in so they sum back to the mix.
- `split: "tracks"` is the same split with the MIDI bus, patch and R2D2 effect chains bypassed, for mixing elsewhere.

Every file in an export has the same length, so they line up at zero in a DAW. `bit_depth` is 16, 24 (default) or 32 (float, never clamps); existing files are kept unless `"overwrite": true`.
```

- [ ] **Step 3: Mark the spec as implemented**

In `docs/superpowers/specs/2026-09-09-audio-export-design.md` change the `Status:` line to `Status: Implemented on branch audio-export (2026-09-09)`.

- [ ] **Step 4: Verify against the latest stable toolchain**

Run:

```bash
rustup update stable 2>&1 | tail -2
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5
cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
```

Expected: fmt check clean, no clippy warnings, both `test result: ok` lines. If clippy reports a new lint from the fresh toolchain, fix it in the file it names before continuing.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md README.md docs/superpowers/specs/2026-09-09-audio-export-design.md
git commit -m "Document export_audio and the export module

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01ED7wujQfBxcx4T4eGirX9i"
```
