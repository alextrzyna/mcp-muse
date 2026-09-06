# Single long-lived MIDI engine (design)

Resolves GitHub issue #95. Follows the analysis in
`2026-09-05-audio-engine-analysis.md` and PR #94.

## Problem

Every `play_notes` / `play_sequence` call builds an independent engine: its
own `OxiSynthSource` (which parses and holds a ~150 MB copy of
FluidR3_GM.sf2), its own pre-rendered R2D2/synthesis buffers, and its own
rodio `Sink`. Calls mix rather than queue.

On 2026-09-05 in the Claude desktop app three sequences overlapped within a
minute. The server sat at 424 MB RSS with two OxiSynth instances rendering
inside the audio callback, and the user heard crackling and dropouts. The
server itself did not fail.

Consequences of the current design:

- memory scales with concurrent playbacks; finished sinks only release their
  SoundFont on the next tool call
- CPU: N overlapping playbacks means N synthesizers on the audio thread
- `stop_playback` stops sinks but cannot silence voices in a shared engine
- there is no way to say "replace what is playing" versus "layer on top"

## Goals

- One `oxisynth::Synth` for the process, SoundFont loaded once.
- Rendering cost independent of how many calls overlap.
- A real stop: silence within one audio chunk, engine back to a known state.
- Play tools gain `mode: replace | layer`, default `replace`.
- Tool names, the note schema and `list_sounds` are unchanged.
- Existing pan and headroom tests carry over.

Non-goals: per-channel MIDI effects (the engine makes them possible later), a
`queue` mode, changing the 44.1 kHz internal rate, a dedicated render thread
(can be added behind the same engine API if the callback still glitches).

## Architecture

```
tool thread                         audio callback thread
-----------                         ---------------------
MidiPlayer                          EngineSource (rodio Source, never ends)
  apply presets, musical time         └─ MidiEngine
  pre-render R2D2 / synthesis              ├─ oxisynth::Synth (one, polyphony 64)
  translate -> PlayCommand                 ├─ BinaryHeap<ScheduledEvent>
  EngineHandle ── mpsc ──────────────────► ├─ Vec<ScheduledBuffer>
  reads AtomicU64 clock ◄─────────────────  ├─ MIDI bus EffectsChain (L, R)
                                            └─ clock: u64 samples
```

### Modules

- `src/midi/engine.rs` (new)
  - `MidiEngine`: scheduler and renderer. No rodio dependency; tests call
    `render(&mut self, left: &mut [f32], right: &mut [f32])` directly.
  - `EngineHandle`: `Arc<AtomicU64>` clock plus `mpsc::Sender<EngineCommand>`.
    Owned by `MidiPlayer`.
  - `EngineSource`: wraps `MidiEngine`, implements `rodio::Source` (stereo,
    44.1 kHz, `total_duration = None`), added to `stream.mixer()` once when
    `MidiPlayer` is created. Returns silence when idle; never returns `None`.
- `src/midi/player.rs`
  - Keeps `MidiPlayer::new`, preset application, musical-time conversion,
    `calculate_tail_time`, and `convert_simple_note_to_synth_params`.
  - `play_enhanced_mixed(sequence)` now translates the sequence into a
    `PlayCommand`, sends it, records the playback's end sample, and returns
    the duration (end + tail) exactly as today.
  - `stop_all()` sends `Stop` and returns how many recorded playbacks had
    not reached their end sample.
  - `OxiSynthSource`, `EnhancedHybridAudioSource`, `ChannelProcessor` and
    the sink list are deleted. `soft_clip`, `MIDI_GAIN`, `OXISYNTH_GAIN`
    and `SYNTH_BUS_GAIN` move to `engine.rs`.

### Time and scheduling

- The engine clock is a `u64` count of frames rendered at 44 100 Hz. It is
  published to the tool thread through `Arc<AtomicU64>` once per chunk.
- A play call computes `start = clock + LEAD` with `LEAD = 2048` frames
  (46 ms) so no event lands in the past, and offsets every note by `start`.
- `ScheduledEvent { at: u64, seq: u64, kind }` in a `BinaryHeap` ordered by
  `(at, seq)` ascending. `seq` is a per-engine counter so events scheduled
  for the same sample keep insertion order (bank select, program change and
  CCs before the note-on they belong to). `kind` is one of `NoteOn`,
  `NoteOff`, `ControlChange`, `ProgramChange`.
- `ScheduledBuffer { start: u64, samples: Vec<f32>, pos: usize }` holds a
  pre-rendered mono R2D2 or synthesis buffer including its effect tail.
  Buffers are mixed only while `start <= clock` and `pos < samples.len()`,
  and dropped when exhausted. Bus gains (`SYNTH_BUS_GAIN` for synthesis,
  1.0 for R2D2) are applied at translation time.
- Per chunk (1024 frames) the engine:
  1. drains the command channel with `try_recv`;
  2. renders OxiSynth frame ranges up to each due event, applying events
     at their exact sample (OxiSynth's `read_next` renders per frame, so
     any range length is fine);
  3. adds the active buffers;
  4. runs the MIDI bus `EffectsChain` per side on the OxiSynth output;
  5. soft-clips and writes stereo frames.
- Polyphony is set to 64 voices.
- OxiSynth keeps rendering while idle so releases finish; there is no
  separate idle path.

### Commands and modes

```rust
enum EngineCommand {
    Play(PlayCommand),
    Stop,
}
struct PlayCommand {
    events: Vec<(u64 /* offset from start */, EventKind)>,
    buffers: Vec<(u64, Vec<f32>)>,
    midi_effects: Option<Vec<EffectConfig>>,
    mode: PlayMode,
}
enum PlayMode { Replace, Layer }
```

- `Replace` (default): fade the current output to silence over 256 frames
  (6 ms, avoids a click), send `MidiEvent::SystemReset` (all voices off,
  programs and controllers back to defaults, reverb and chorus cleared),
  clear the event heap and buffer list, set the MIDI bus chain to the
  call's `midi_effects` (or none), then schedule the call. Each replace
  therefore starts from the same state a fresh synthesizer has today.
- `Layer`: leave everything sounding and scheduled. Set the bus chain only
  if the call carries one. Schedule the call.
- `Stop`: same fade and reset as `Replace`, then nothing is scheduled and
  the bus chain is cleared.
- Translation (tool thread), per call, tracks per-channel state in a map
  as `OxiSynthSource::process_audio_chunk` does today:
  - the first note on a channel emits `ProgramChange` for
    `instrument.unwrap_or(0)`, so an unspecified instrument still means
    piano even when layering over a call that changed the program;
  - channel 9 emits bank select MSB 128 / LSB 0 and program 0 instead;
  - reverb, chorus, volume, pan, balance, expression and sustain CCs are
    emitted only when a note specifies them and the value changed.
- The MIDI bus chain comes from the first MIDI note in the call that has a
  non-empty `effects` list, as today.

### Tool surface

- `PlayMode` is added to `SimpleSequence` and `ExtendedSequence` as
  `#[serde(default)] pub mode: PlayMode` (serde `rename_all = "lowercase"`),
  and `ExtendedSequence::resolve_patterns` copies it through.
- `play_notes` and `play_sequence` schemas gain
  `"mode": {"type": "string", "enum": ["replace", "layer"], "default": "replace", "description": ...}`.
  An unknown value fails deserialization and returns `-32602` like any
  other bad argument.
- The tool text stays "Playback started (...)" and now names the mode.
- `stop_playback` text is unchanged.
- The SoundFont is loaded when the engine is created (first playback,
  ~80 ms in release). If it is missing the engine runs without a synth:
  synthesis-only calls work and a call with MIDI notes returns the existing
  "run `mcp-muse setup`" error as an `isError` result.
- If the audio thread has gone away the command channel is closed;
  `play_enhanced_mixed` returns an error and the tool reports it.
- `CLAUDE.md` "Audio pipeline" section and the README tool docs are updated.

### Error handling

- Unknown preset, invalid R2D2 or synthesis note, missing SoundFont for
  MIDI notes: errors at translation, before anything is sent, so a failed
  call never half-replaces the current playback.
- OxiSynth `send_event` errors are logged at debug level and ignored on the
  audio thread, as today.

## Testing

Engine tests (`src/midi/engine.rs`, offline render, skip MIDI cases when
no SoundFont is installed):

- pan moves the stereo image (carried over)
- typical material stays below the clipper knee, and is not too quiet
  (carried over; drives translation then `render`)
- an event scheduled at sample N sounds from N (RMS of the window before
  N is zero, after N is not)
- a buffer scheduled at T is mixed from T and both sides are identical
- `Replace` silences a queued sequence within one chunk and the new call's
  notes sound
- `Layer` keeps both sequences audible
- `Stop` yields silence and a subsequent play with no instrument is piano
- bus chain rule: replace installs the call's chain, layer without a chain
  keeps the existing one
- clock advances by frames rendered

Player tests (`src/midi/player.rs`):

- translation emits program change / bank select before note-on at the
  same sample, CC dedup per call, `start` offset applied
- `stop_all` counts playbacks that have not reached their end

Integration tests (`tests/integration/mcp_protocol.rs`):

- `mode` appears in both schemas with default `replace`
- invalid `mode` returns `-32602`
- two consecutive `play_notes` calls succeed and `stop_playback` reports
  the count

Manual: `cargo run -- test-presets`, then the issue's desktop-app scenario
(three overlapping sequences) checking for clean audio and a flat RSS.
