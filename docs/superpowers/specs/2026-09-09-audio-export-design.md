# Exporting compositions to disk

Date: 2026-09-09
Status: Approved design, not yet implemented

## Goal

Let the calling agent save a composition as audio files instead of, or in
addition to, hearing it: a stereo mixdown, a set of stems (one file per
sound source with every effect baked in, summing back to the mix), or a set
of dry tracks (the same split with the effect chains bypassed) for mixing
elsewhere later.

## Decisions taken

- **A dedicated `export_audio` tool.** It takes `play_sequence`'s inputs
  plus output options, renders offline and writes files without playing.
  It never opens the audio device and never touches live playback. The
  tool count goes from seven to eight. (Alternative rejected: an `export`
  object on the play tools, which would couple saving to hearing and delay
  playback by the render time.)
- **Wet stems, dry tracks.** Both splits produce one file per source (one
  per MIDI channel with notes, one per synth patch group, one for all R2D2
  notes). Stems keep the MIDI bus chain, patch effect chains and R2D2
  effect chains. Tracks bypass all three. OxiSynth's own reverb and chorus
  sends (the `reverb` and `chorus` note controllers) stay in tracks: they
  are part of the GM instrument rendition, not a mix-bus effect.
- **WAV, 44.1 kHz stereo, 24-bit by default.** `bit_depth` may be 16, 24
  or 32; 32 means IEEE float. Stems and tracks skip the mixer's soft
  clipper so they sum exactly; the mixdown is soft-clipped like playback.
- **A required absolute directory.** The agent must say where files go.
  Existing files are never overwritten unless `overwrite: true`.
- **Offline exporter on the existing translator and engine.** The
  `Translator` already produces separate MIDI events, per-patch buffers,
  R2D2 buffers and the bus chain; `MidiEngine` already renders without a
  device (the headroom tests do exactly this). A private engine with its
  own OxiSynth is built per export: loading the SoundFont takes about
  70 ms in release, and a fresh synthesizer makes every render
  deterministic and free of live-playback state. This is a deliberate,
  narrow exception to "one engine per process": that rule guards the
  live audio path, where a synthesizer per call lost state between calls.
  (Alternatives rejected: recording the live engine, which needs a device
  and cannot split MIDI channels; and a standalone renderer driving
  OxiSynth directly, which would duplicate event timing, fade and bus
  semantics and drift from playback.)
- **WAV writing via the `hound` crate**, also used by tests to read the
  files back.

## 1. Tool surface

```json
{
  "name": "export_audio",
  "inputSchema": {
    "notes": "same as play_sequence",
    "patterns": "same as play_sequence",
    "tempo": "same as play_sequence",
    "beats_per_bar": "same as play_sequence",
    "path": "absolute directory (required)",
    "name": "base name, default \"mix\"",
    "split": "\"mixdown\" (default) | \"stems\" | \"tracks\"",
    "bit_depth": "16 | 24 | 32, default 24",
    "overwrite": "boolean, default false"
  }
}
```

There is no `mode`: an export is always a clean render. Pattern
references resolve against the session's patterns and `synth` names
against the session's patches then the built-ins, exactly as in
`play_sequence`.

`name` is sanitized to `[A-Za-z0-9_-]`; every other character becomes `_`.
A name that is empty after sanitizing is a parameter error.

## 2. Files written

All files are stereo 44.1 kHz WAV at the requested bit depth.

- `split: mixdown` writes `<path>/<name>.wav`, soft-clipped exactly like
  playback.
- `split: stems` and `split: tracks` write `<path>/<name>/<source>.wav`,
  one file per source, not soft-clipped.

Source names:

- `ch<NN>_<gm name>` for each MIDI channel that has notes, where `NN` is
  the two-digit channel and the GM name is the channel's first program's
  name from `gm_names.rs`, sanitized the same way as `name` and lowercased
  (`ch00_acoustic_grand_piano`). Channel 9 is `ch09_drums`.
- `synth_<patch key>` for each patch group (the translator's grouping:
  named patches by key, identical inline patches together).
- `r2d2` for all R2D2 notes summed.

Every file in one export has the same length: the composition's reported
duration including tails, rounded up to whole frames, zero-padded. Files
therefore line up at zero on a DAW timeline. Tracks are the same length as
stems would be even though their tails are shorter.

## 3. Response

Tool text listing each written path on its own line, then the total
duration in seconds and the wall-clock render time. When a 16 or 24-bit
file peaked above 0 dBFS the response names those files, says they were
clamped, and suggests `bit_depth: 32`. Peaks are measured before clamping.

Example:

```
💾 Exported 3 stems to /Users/alex/Music/demo/mix/
  /Users/alex/Music/demo/mix/ch00_acoustic_grand_piano.wav
  /Users/alex/Music/demo/mix/synth_minimoog_bass.wav
  /Users/alex/Music/demo/mix/r2d2.wav
Duration 12.4 s (including effect tails), rendered in 0.9 s.
```

## 4. Translator refactor (`src/midi/translate.rs`)

The body of `translate` moves into

```rust
pub enum Effects { Wet, Dry }

pub struct PatchRender { pub key: String, pub start: u64, pub samples: Vec<[f32; 2]> }

pub struct TranslatedParts {
    pub midi: Vec<MidiNote>,
    pub midi_effects: Option<Vec<EffectConfig>>,
    pub patches: Vec<PatchRender>,
    pub r2d2: Vec<(u64, Vec<[f32; 2]>)>,
    pub tempo: u32,
    pub duration: Duration,
}

impl Translator {
    pub fn translate_parts(&self, sequence, session_patches, effects: Effects)
        -> Result<TranslatedParts, String>;
}
```

`translate(sequence, mode, patches)` becomes `translate_parts(.., Wet)`
followed by a flatten into `PlayCommand` (patch buffers already carry
`SYNTH_BUS_GAIN`; R2D2 buffers are stereo as today). Playback behaviour and
its tests do not change.

`Dry` renders each patch with a clone whose `effects` is empty, skips the
R2D2 chain, and sets `midi_effects` to `None`. `duration` in dry parts is
computed the same way and so is shorter; the exporter pads every file to
the wet length (it translates wet first to learn it, then dry for the
track contents, when `split: tracks`).

`midi_events(&[MidiNote])` already works on any slice; a per-channel event
list is a filter on `channel` before calling it, which also makes each
channel's own program change land in its pass.

## 5. Exporter (`src/midi/export.rs`)

```rust
pub enum Split { Mixdown, Stems, Tracks }
pub enum BitDepth { Int16, Int24, Float32 }

pub struct ExportRequest {
    pub sequence: SimpleSequence,
    pub dir: PathBuf,
    pub name: String,
    pub split: Split,
    pub bit_depth: BitDepth,
    pub overwrite: bool,
}

pub struct ExportedFile { pub path: PathBuf, pub peak: f32 }

pub struct ExportReport {
    pub files: Vec<ExportedFile>,
    pub duration: Duration,
    pub render_time: Duration,
}

pub fn export(request: ExportRequest, session_patches: &HashMap<String, Patch>)
    -> Result<ExportReport, String>;
```

Steps, ordered so nothing is written until everything is known:

1. Translate wet (and dry for `Tracks`). Needs a `Translator`; the
   exporter builds one with `midi_available` set from `find_soundfont()`,
   so a missing SoundFont with MIDI notes fails here with the same message
   playback gives.
2. Derive the target file list from the parts. Refuse if any target exists
   and `overwrite` is false, listing the collisions. Create `dir` (and the
   split subdirectory) with `create_dir_all`.
3. If the parts contain MIDI notes, load the SoundFont into a private
   `MidiEngine::new(Some(synth))`; otherwise `MidiEngine::new(None)`.
4. Render passes through one helper:
   `render_offline(&mut engine, PlayCommand, frames) -> Vec<[f32; 2]>`
   applies the command in `Replace` mode, pulls `CHUNK_FRAMES` chunks
   through `render_unclipped` for `LEAD_FRAMES + frames` frames, and
   discards the lead-in. The engine is reused across passes: a replace
   resets OxiSynth and clears buffers, and the previous pass has already
   rung out so the declick fade is silent.
   - `Mixdown`: one pass with the flattened command, then `soft_clip` per
     sample.
   - `Stems`/`Tracks`: one pass per MIDI channel with only that channel's
     events and the parts' bus chain (present for stems, `None` for
     tracks; no buffers); each patch and the R2D2 sum are
     placed at their offsets into zeroed full-length buffers without
     rendering.
5. Write each buffer with `hound` (`WavSpec { channels: 2, sample_rate:
   44_100, bits_per_sample, sample_format }`). Integer formats clamp to
   ±1.0 after recording the unclamped peak. A write failure returns an
   error naming the file; files already written stay on disk.

`render_unclipped` is `pub(crate)` today and stays so; the exporter is in
the same crate. `engine_with_soundfont` stays test-only; the exporter uses
`find_soundfont` and `load_synth` directly.

Everything runs synchronously on the tool thread. OxiSynth renders far
faster than real time; a 16-channel, five-minute split is tens of seconds
at worst, and `render_time` in the response lets the agent calibrate.
Render lengths are already bounded by `MAX_NOTE_SECONDS` at the tool
boundary and `MAX_RENDER_SECONDS` in the patch renderer.

## 6. Server wiring (`src/server/mcp.rs`)

`handle_export_audio` parses the output options, then reuses the
`play_sequence` path: `ExtendedSequence` parsing, `validate_tempo`,
`validate_notes`, `resolve_patterns` against `state.patterns`, and the
"resolved sequence contains no notes" check. It calls `export` with
`state.synths` and never calls `state.player()`, so exports work on a
machine without an audio device.

Errors:

- Parameter errors (`-32602`): malformed JSON or unknown fields, a relative
  `path`, an empty sanitized `name`, `split` or `bit_depth` outside their
  enums, tempo or note validation failures.
- Tool errors (`isError: true`): unknown pattern or synth names, a
  directory that cannot be created, existing files without `overwrite`,
  SoundFont missing while MIDI notes are present, any write failure, a
  sequence that resolves to no notes.
- Not an error: a 16 or 24-bit file peaking above 0 dBFS. It is written
  clamped and the response says so.

Documentation: `CLAUDE.md` lists the eighth tool and the export module;
README gets a short "Saving audio" section with the three splits.

## 7. Tests

Measure buffers; never listen by ear.

- Translator: `Dry` parts have no bus chain and their patch and R2D2
  buffers are shorter than `Wet` ones by the effect tail; `Wet` parts
  flattened equal today's `translate` output (same events, same buffer
  offsets and lengths).
- Exporter without a SoundFont: a patch-only export writes files; a MIDI
  export returns the SoundFont error and writes nothing.
- Offline rendering: a patch note exported as a mixdown and read back with
  `hound` has silence before its offset and energy after it, the frame
  count equals the reported duration, and the spec matches the requested
  bit depth for 16, 24 and 32.
- Splits: stems of a patch plus R2D2 composition sum to the unclipped
  mixdown within a small tolerance. SoundFont-gated (skips with a message
  when not installed, like the headroom tests): two MIDI channels at
  different pitches produce two files, each with Goertzel energy only at
  its own pitch, and channel 9 is named `ch09_drums`.
- Files: name sanitizing, overwrite refusal that lists the collisions,
  overwrite success, directory creation, relative path rejected as a
  parameter error, every file in a split has the same frame count.
- Server: `tools/list` has eight tools and `export_audio` exposes `path`,
  `split` and `bit_depth`; one integration test spawns the binary, exports
  a synth note into a temporary directory and checks for a valid
  RIFF/WAVE header.
- `cargo fmt`, and `cargo clippy --all-targets --all-features -- -D
  warnings` on an updated stable toolchain.
