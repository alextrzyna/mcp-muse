# Single Long-Lived MIDI Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the per-call OxiSynth/Sink pipeline with one process-wide engine that owns the SoundFont, schedules events on a sample clock, and supports `replace`/`layer` play modes and a real stop.

**Architecture:** A new `MidiEngine` (in `src/midi/engine.rs`) runs inside a never-ending rodio `Source` on the output mixer. Tool calls translate a `SimpleSequence` into a `PlayCommand` (events at frame offsets plus pre-rendered mono buffers) in `src/midi/translate.rs` and send it over an `mpsc` channel; the engine applies commands at chunk boundaries and applies events sample-accurately. `src/midi/player.rs` shrinks to the audio stream, the engine handle, the translator and playback bookkeeping.

**Tech Stack:** Rust 2024, rodio 0.21 (`OutputStream`, `Mixer::add`, `Source`), oxisynth 0.1 (`Synth`, `MidiEvent`, `SystemReset`), std `mpsc` + `AtomicU64`, existing `EffectsChain`/`ExpressiveSynth`.

**Spec:** `docs/superpowers/specs/2026-09-05-single-midi-engine-design.md` (resolves GitHub issue #95).

## Global Constraints

- Internal sample rate stays `44_100` Hz, chunk size `1024` frames, schedule lead `2048` frames, fade `256` frames, polyphony `256`.
- Tool names, `note_schema()`, and `list_sounds` output are unchanged. `mode` is the only new tool argument, enum `["replace", "layer"]`, default `replace`.
- Never log inside per-sample loops (CLAUDE.md). Debug logging of OxiSynth errors happens per event, not per sample.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass at every commit. `cargo test` must pass; MIDI tests skip (with an `eprintln!`) when no SoundFont is installed.
- Commit messages end with the trailer block used in this repo:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs
  ```
- Work on branch `feat/single-midi-engine` (already created from `origin/main`).

## File structure

| File | Responsibility |
|---|---|
| `src/midi/engine.rs` (new) | `PlayMode`, `EventKind`, `PlayCommand`, `EngineCommand`, `MidiEngine` (scheduler + renderer), `EngineHandle`, `EngineSource` (rodio wrapper), `find_soundfont`, `load_synth`, gain constants, `soft_clip`, `seconds_to_frames`. |
| `src/midi/translate.rs` (new) | `Translator`: presets, musical time, R2D2/synthesis pre-render, MIDI event generation, tail time. Code moved from `player.rs`. |
| `src/midi/player.rs` (rewritten) | `MidiPlayer`: opens the stream once, owns `EngineHandle` + `Translator`, `play(sequence, mode)`, `stop_all()`, playback-end bookkeeping. Pipeline level tests. |
| `src/midi/mod.rs` | Declare the two new modules, re-export `PlayMode`. |
| `src/server/mcp.rs` | Parse `mode`, schema entries, tool text. |
| `src/demos.rs` | Call `play(seq, PlayMode::Layer)` (same overlap behaviour as before). |
| `tests/integration/mcp_protocol.rs` | `mode` schema/validation, consecutive plays + stop count. |
| `CLAUDE.md`, `README.md`, spec | Docs. |

Tasks 1–6 build the new modules while the old pipeline still compiles. To keep clippy green, `src/midi/engine.rs` and `src/midi/translate.rs` start with `#![allow(dead_code)]`; Task 8 removes both lines when the player switches over.

---

### Task 1: Engine skeleton, clock, commands

**Files:**
- Create: `src/midi/engine.rs`
- Modify: `src/midi/mod.rs:1-5`

**Interfaces:**
- Produces: `PlayMode`, `EventKind`, `PlayCommand`, `EngineCommand`, `MidiEngine::new(synth: Option<oxisynth::Synth>) -> (MidiEngine, EngineHandle)`, `MidiEngine::apply(&mut self, EngineCommand)`, `MidiEngine::render(&mut self, &mut [f32], &mut [f32])`, `MidiEngine::render_unclipped`, `EngineHandle::clock() -> u64`, `EngineHandle::send(EngineCommand) -> Result<(), String>`, constants `SAMPLE_RATE`, `CHUNK_FRAMES`, `LEAD_FRAMES`, `SYNTH_BUS_GAIN`, `seconds_to_frames(Duration) -> u64`, `soft_clip(f32) -> f32`.

- [ ] **Step 1: Declare the module**

In `src/midi/mod.rs` replace lines 1-5 with:

```rust
pub mod engine;
pub mod gm_names;
pub mod parser;
pub mod player;
pub mod translate;

pub use engine::PlayMode;
pub use player::*;
```

(`translate` does not exist until Task 7; create an empty file `src/midi/translate.rs` containing only `#![allow(dead_code)]` now so the crate builds.)

- [ ] **Step 2: Write the failing tests**

Create `src/midi/engine.rs` with the tests first (the types are filled in during Step 4):

```rust
//! One long-lived MIDI/synthesis engine for the process (GitHub issue #95).
//!
//! `MidiEngine` owns the single OxiSynth instance, a queue of MIDI events keyed
//! to its sample clock, and the pre-rendered R2D2/synthesis buffers scheduled
//! on a shared mono bus. Tool calls send `EngineCommand`s over a channel; the
//! engine drains them at chunk boundaries and applies events at their exact
//! sample.
#![allow(dead_code)]

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Render `frames` frames in engine-sized chunks and return (left, right).
    pub(crate) fn render_all(engine: &mut MidiEngine, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
        let (mut l, mut r) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
        while left.len() < frames {
            engine.render(&mut l, &mut r);
            left.extend_from_slice(&l);
            right.extend_from_slice(&r);
        }
        left.truncate(frames);
        right.truncate(frames);
        (left, right)
    }

    #[test]
    fn idle_engine_renders_silence_and_advances_the_clock() {
        let (mut engine, handle) = MidiEngine::new(None);
        assert_eq!(handle.clock(), 0);
        let (left, right) = render_all(&mut engine, 3 * CHUNK_FRAMES);
        assert!(left.iter().chain(right.iter()).all(|s| *s == 0.0));
        assert_eq!(handle.clock(), 3 * CHUNK_FRAMES as u64);
    }

    #[test]
    fn commands_sent_through_the_handle_are_applied_at_the_next_chunk() {
        let (mut engine, handle) = MidiEngine::new(None);
        handle
            .send(EngineCommand::Play(PlayCommand {
                buffers: vec![(0, vec![0.25; 10])],
                ..Default::default()
            }))
            .unwrap();
        assert!(engine.buffers.is_empty(), "nothing applied before render");
        let (mut l, mut r) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
        engine.render(&mut l, &mut r);
        assert_eq!(engine.buffers.len(), 1);
        assert_eq!(engine.buffers[0].start, LEAD_FRAMES);
    }

    #[test]
    fn a_dropped_handle_does_not_break_rendering() {
        let (mut engine, handle) = MidiEngine::new(None);
        drop(handle);
        let (left, _) = render_all(&mut engine, CHUNK_FRAMES);
        assert_eq!(left.len(), CHUNK_FRAMES);
    }

    #[test]
    fn seconds_to_frames_rounds_to_nearest() {
        assert_eq!(seconds_to_frames(Duration::from_secs(1)), 44_100);
        assert_eq!(seconds_to_frames(Duration::from_secs_f64(0.5)), 22_050);
        assert_eq!(seconds_to_frames(Duration::ZERO), 0);
    }

    #[test]
    fn soft_clip_is_transparent_then_bounded() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-0.5), -0.5);
        assert!(soft_clip(3.0) <= 1.0 && soft_clip(3.0) > 0.9);
        assert!(soft_clip(-3.0) >= -1.0);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse midi::engine 2>&1 | tail -20`
Expected: compile errors (`MidiEngine`, `PlayCommand` ... not found).

- [ ] **Step 4: Write the implementation**

Insert between the module doc comment and `#[cfg(test)]`:

```rust
use crate::expressive::EffectsChain;
use crate::midi::EffectConfig;
use oxisynth::{MidiEvent, SoundFont, Synth};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::Arc;
use std::time::Duration;

/// Internal render rate. The output mixer resamples if the device differs.
pub const SAMPLE_RATE: u32 = 44_100;
/// Frames rendered per engine call; commands are drained at this granularity.
pub const CHUNK_FRAMES: usize = 1024;
/// Frames between a play command and its first event, so nothing lands in the past.
pub const LEAD_FRAMES: u64 = 2048;
/// Length of the declick fade applied before a replace or stop (6 ms).
const FADE_FRAMES: usize = 256;
/// Voice cap; FluidR3 material never needs OxiSynth's default 256.
const POLYPHONY: u16 = 64;
/// Synthesizer gain passed to OxiSynth (its default of 0.2 is very quiet).
/// At 1.0 a single velocity-100 note peaks around 0.35 and a four-note
/// chord around 0.5, leaving headroom before the soft clipper (knee 0.8).
const OXISYNTH_GAIN: f32 = 1.0;
/// Gain on the pre-rendered synthesis bus so preset notes (amplitude ~0.8)
/// sit at the same level as MIDI instruments. Applied by the translator.
pub const SYNTH_BUS_GAIN: f32 = 0.5;

/// How a play call relates to whatever is already sounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayMode {
    /// Stop everything, reset the synthesizer, then play (default).
    #[default]
    Replace,
    /// Mix on top of the current playback.
    Layer,
}

impl PlayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PlayMode::Replace => "replace",
            PlayMode::Layer => "layer",
        }
    }
}

/// A MIDI message the engine forwards to OxiSynth at a scheduled frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
}

/// Everything one play call schedules. Offsets are frames after the
/// command's start; the engine picks the start as `clock + LEAD_FRAMES`.
#[derive(Debug, Clone, Default)]
pub struct PlayCommand {
    /// MIDI events in time order (stable: setup before note-on at equal offsets).
    pub events: Vec<(u64, EventKind)>,
    /// Pre-rendered mono buffers (R2D2, synthesis) already at bus level.
    pub buffers: Vec<(u64, Vec<f32>)>,
    /// MIDI bus effects chain for this call, if any note specified one.
    pub midi_effects: Option<Vec<EffectConfig>>,
    pub mode: PlayMode,
}

#[derive(Debug)]
pub enum EngineCommand {
    Play(PlayCommand),
    Stop,
}

/// Duration in seconds to engine frames, rounded to nearest.
pub fn seconds_to_frames(duration: Duration) -> u64 {
    (duration.as_secs_f64() * SAMPLE_RATE as f64).round() as u64
}

/// Transparent below ±0.8, then a smooth tanh knee so summed buses cannot hard-clip.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    if x.abs() <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((x.abs() - KNEE) / (1.0 - KNEE)).tanh())
    }
}

struct ScheduledEvent {
    at: u64,
    seq: u64,
    kind: EventKind,
}

// `BinaryHeap` is a max-heap; invert the ordering so the earliest event
// (then the earliest scheduled) is on top.
impl PartialEq for ScheduledEvent {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at && self.seq == other.seq
    }
}
impl Eq for ScheduledEvent {}
impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.at, other.seq).cmp(&(self.at, self.seq))
    }
}

struct ScheduledBuffer {
    start: u64,
    samples: Vec<f32>,
    pos: usize,
}

/// Tool-thread side of the engine: the published clock and the command channel.
#[derive(Clone)]
pub struct EngineHandle {
    clock: Arc<AtomicU64>,
    sender: Sender<EngineCommand>,
}

impl EngineHandle {
    /// Frames rendered so far, as of the last completed chunk.
    pub fn clock(&self) -> u64 {
        self.clock.load(AtomicOrdering::Relaxed)
    }

    /// Fails only if the audio thread has dropped the engine.
    pub fn send(&self, command: EngineCommand) -> Result<(), String> {
        self.sender
            .send(command)
            .map_err(|_| "Audio engine is no longer running".to_string())
    }
}

pub struct MidiEngine {
    /// `None` when no SoundFont is installed: MIDI events are dropped, buffers still play.
    synth: Option<Synth>,
    events: BinaryHeap<ScheduledEvent>,
    next_seq: u64,
    buffers: Vec<ScheduledBuffer>,
    bus_left: EffectsChain,
    bus_right: EffectsChain,
    clock: u64,
    shared_clock: Arc<AtomicU64>,
    commands: Receiver<EngineCommand>,
    /// Declick tail rendered from the state before the last replace/stop.
    fade_tail: Vec<(f32, f32)>,
    fade_pos: usize,
}

impl MidiEngine {
    pub fn new(synth: Option<Synth>) -> (Self, EngineHandle) {
        let clock = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = channel();
        let engine = Self {
            synth,
            events: BinaryHeap::new(),
            next_seq: 0,
            buffers: Vec::new(),
            bus_left: EffectsChain::default(),
            bus_right: EffectsChain::default(),
            clock: 0,
            shared_clock: Arc::clone(&clock),
            commands: receiver,
            fade_tail: Vec::new(),
            fade_pos: 0,
        };
        (engine, EngineHandle { clock, sender })
    }

    /// Apply a command now (tests call this directly; the audio thread calls
    /// it from `drain_commands`).
    pub fn apply(&mut self, command: EngineCommand) {
        match command {
            EngineCommand::Play(play) => self.schedule(play),
            EngineCommand::Stop => self.silence(),
        }
    }

    fn drain_commands(&mut self) {
        loop {
            match self.commands.try_recv() {
                Ok(command) => self.apply(command),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn schedule(&mut self, play: PlayCommand) {
        match play.mode {
            PlayMode::Replace => {
                self.silence();
                self.set_bus_effects(play.midi_effects.as_deref().unwrap_or(&[]));
            }
            PlayMode::Layer => {
                if let Some(effects) = &play.midi_effects {
                    self.set_bus_effects(effects);
                }
            }
        }
        let start = self.clock + LEAD_FRAMES;
        for (offset, kind) in play.events {
            self.push_event(start + offset, kind);
        }
        for (offset, samples) in play.buffers {
            self.buffers.push(ScheduledBuffer {
                start: start + offset,
                samples,
                pos: 0,
            });
        }
        tracing::info!(
            "Scheduled {} at frame {}: {} MIDI events, {} buffers, {} queued playbacks",
            play.mode.as_str(),
            start,
            self.events.len(),
            self.buffers.len(),
            self.buffers.len()
        );
    }

    fn push_event(&mut self, at: u64, kind: EventKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push(ScheduledEvent { at, seq, kind });
    }

    fn set_bus_effects(&mut self, effects: &[EffectConfig]) {
        self.bus_left = EffectsChain::new(SAMPLE_RATE as f32, effects);
        self.bus_right = EffectsChain::new(SAMPLE_RATE as f32, effects);
    }

    /// Fade the current output over `FADE_FRAMES`, then reset everything:
    /// voices off, programs and controllers to defaults, queue and buffers
    /// cleared, bus chain removed.
    fn silence(&mut self) {
        let (mut left, mut right) = (vec![0.0f32; FADE_FRAMES], vec![0.0f32; FADE_FRAMES]);
        self.render_frames(&mut left, &mut right);
        self.fade_tail = left
            .iter()
            .zip(&right)
            .enumerate()
            .map(|(i, (&l, &r))| {
                let gain = 1.0 - (i + 1) as f32 / FADE_FRAMES as f32;
                (l * gain, r * gain)
            })
            .collect();
        self.fade_pos = 0;
        if let Some(synth) = &mut self.synth
            && let Err(e) = synth.send_event(MidiEvent::SystemReset)
        {
            tracing::debug!("SystemReset rejected: {:?}", e);
        }
        self.events.clear();
        self.buffers.clear();
        self.set_bus_effects(&[]);
    }

    fn apply_event(&mut self, kind: EventKind) {
        let Some(synth) = &mut self.synth else { return };
        let event = match kind {
            EventKind::NoteOn { channel, key, velocity } => MidiEvent::NoteOn {
                channel,
                key,
                vel: velocity,
            },
            EventKind::NoteOff { channel, key } => MidiEvent::NoteOff { channel, key },
            EventKind::ControlChange { channel, controller, value } => MidiEvent::ControlChange {
                channel,
                ctrl: controller,
                value,
            },
            EventKind::ProgramChange { channel, program } => MidiEvent::ProgramChange {
                channel,
                program_id: program,
            },
        };
        if let Err(e) = synth.send_event(event) {
            tracing::debug!("OxiSynth rejected event: {:?}", e);
        }
    }

    /// Produce `left.len()` unclipped frames from the current state without
    /// applying events or advancing the clock. Buffers are positioned
    /// relative to `self.clock`.
    fn render_frames(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len();
        left.fill(0.0);
        right.fill(0.0);
        if let Some(synth) = &mut self.synth {
            synth.write((&mut left[..], &mut right[..]));
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                *l = self.bus_left.process(*l);
                *r = self.bus_right.process(*r);
            }
        }
        let base = self.clock;
        for buffer in &mut self.buffers {
            if buffer.start >= base + n as u64 {
                continue;
            }
            let offset = buffer.start.saturating_sub(base) as usize;
            let count = (buffer.samples.len() - buffer.pos).min(n - offset);
            for i in 0..count {
                let s = buffer.samples[buffer.pos + i];
                left[offset + i] += s;
                right[offset + i] += s;
            }
            buffer.pos += count;
        }
        self.buffers.retain(|b| b.pos < b.samples.len());
    }

    /// Render one chunk before the soft clipper (used by the headroom tests).
    pub(crate) fn render_unclipped(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        self.drain_commands();
        let frames = left.len();
        let mut pos = 0;
        while pos < frames {
            while self.events.peek().is_some_and(|e| e.at <= self.clock) {
                let event = self.events.pop().expect("peeked");
                self.apply_event(event.kind);
            }
            let remaining = (frames - pos) as u64;
            let until_next = self
                .events
                .peek()
                .map(|e| e.at - self.clock)
                .unwrap_or(remaining);
            let n = until_next.min(remaining) as usize;
            let (l, r) = (&mut left[pos..pos + n], &mut right[pos..pos + n]);
            self.render_frames(l, r);
            pos += n;
            self.clock += n as u64;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let Some(&(fl, fr)) = self.fade_tail.get(self.fade_pos) else { break };
            *l += fl;
            *r += fr;
            self.fade_pos += 1;
        }
        self.shared_clock.store(self.clock, AtomicOrdering::Relaxed);
    }

    /// Render one chunk of stereo frames and advance the clock.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.render_unclipped(left, right);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = soft_clip(*l);
            *r = soft_clip(*r);
        }
    }

    #[cfg(test)]
    pub(crate) fn synth(&self) -> Option<&Synth> {
        self.synth.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn bus_has_effects(&self) -> bool {
        !self.bus_left.is_empty()
    }
}
```

Note on `render_frames`: the loop body `while self.events.peek().is_some_and(...)` pops every event due at or before the clock, so `until_next` is at least 1 and the outer loop always progresses.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --bin mcp-muse midi::engine 2>&1 | tail -20`
Expected: 5 passed.

- [ ] **Step 6: Format, lint, commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/engine.rs src/midi/translate.rs src/midi/mod.rs
git commit -m "feat(engine): MidiEngine skeleton with sample clock and command channel

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 2: Scheduled buffers, layer vs replace, bus chain rule

No SoundFont needed. Exercises `render_frames` buffer mixing, `schedule` and `silence`.

**Files:**
- Modify: `src/midi/engine.rs` (tests module)

**Interfaces:**
- Consumes: Task 1 types.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src/midi/engine.rs`:

```rust
    use crate::expressive::test_util::{goertzel_power, rms, sine};
    use crate::midi::EffectConfig;

    fn reverb() -> EffectConfig {
        serde_json::from_value(serde_json::json!({
            "type": "reverb", "room_size": 0.6, "intensity": 0.4
        }))
        .unwrap()
    }

    fn play(buffers: Vec<(u64, Vec<f32>)>, mode: PlayMode) -> EngineCommand {
        EngineCommand::Play(PlayCommand {
            buffers,
            mode,
            ..Default::default()
        })
    }

    #[test]
    fn a_buffer_scheduled_at_t_is_mixed_from_t_on_both_sides() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(10, vec![0.5; 1000])], PlayMode::Replace));
        let (left, right) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        let start = LEAD_FRAMES as usize + 10;
        assert_eq!(left[start - 1], 0.0);
        assert_eq!(left[start], 0.5);
        assert_eq!(left[start + 999], 0.5);
        assert_eq!(left[start + 1000], 0.0);
        assert_eq!(left, right, "mono buffers must be centered");
        assert!(engine.buffers.is_empty(), "exhausted buffers are dropped");
    }

    #[test]
    fn layer_keeps_the_current_buffer_and_replace_cuts_it() {
        let sr = SAMPLE_RATE as f32;
        let a = sine(440.0, 3.0, sr, 0.3);
        let b = sine(1000.0, 3.0, sr, 0.3);

        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, a.clone())], PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES); // A is now sounding
        engine.apply(play(vec![(0, b.clone())], PlayMode::Layer));
        let (left, _) = render_all(&mut engine, 8 * CHUNK_FRAMES);
        let window = &left[4 * CHUNK_FRAMES..];
        assert!(goertzel_power(window, 440.0, sr) > 1e-3, "layer lost A");
        assert!(goertzel_power(window, 1000.0, sr) > 1e-3, "layer lost B");

        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, a)], PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES);
        engine.apply(play(vec![(0, b)], PlayMode::Replace));
        let (left, _) = render_all(&mut engine, 8 * CHUNK_FRAMES);
        let window = &left[4 * CHUNK_FRAMES..];
        assert!(goertzel_power(window, 440.0, sr) < 1e-6, "replace kept A");
        assert!(goertzel_power(window, 1000.0, sr) > 1e-3, "replace lost B");
    }

    #[test]
    fn replace_and_stop_fade_within_one_chunk() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, vec![0.5; 44_100])], PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES);
        engine.apply(EngineCommand::Stop);
        let (left, _) = render_all(&mut engine, CHUNK_FRAMES);
        assert!(left[0] > 0.4, "fade starts from the current level");
        assert!(left[FADE_FRAMES / 2] > 0.0 && left[FADE_FRAMES / 2] < left[0]);
        assert!(rms(&left[FADE_FRAMES..]) == 0.0, "silent after the fade");
        assert!(engine.buffers.is_empty());
    }

    #[test]
    fn bus_chain_follows_the_replace_and_layer_rules() {
        let (mut engine, _handle) = MidiEngine::new(None);
        let with = |effects: Option<Vec<EffectConfig>>, mode| {
            EngineCommand::Play(PlayCommand {
                midi_effects: effects,
                mode,
                ..Default::default()
            })
        };
        engine.apply(with(Some(vec![reverb()]), PlayMode::Replace));
        assert!(engine.bus_has_effects(), "replace installs the call's chain");
        engine.apply(with(None, PlayMode::Layer));
        assert!(engine.bus_has_effects(), "layer without a chain keeps it");
        engine.apply(with(Some(Vec::new()), PlayMode::Layer));
        assert!(!engine.bus_has_effects(), "layer with an explicit chain replaces it");
        engine.apply(with(Some(vec![reverb()]), PlayMode::Layer));
        engine.apply(with(None, PlayMode::Replace));
        assert!(!engine.bus_has_effects(), "replace without a chain clears it");
        engine.apply(EngineCommand::Stop);
        assert!(!engine.bus_has_effects());
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --bin mcp-muse midi::engine 2>&1 | tail -20`
Expected: all pass with the Task 1 implementation. If `replace_and_stop_fade_within_one_chunk` fails on `left[0] > 0.4`, the fade tail is being added after clipping or from the wrong state; check that `silence()` renders the tail before clearing buffers.

- [ ] **Step 3: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/engine.rs
git commit -m "test(engine): buffer scheduling, layer/replace, fade and bus chain rules

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 3: SoundFont loading and sample-accurate MIDI events

**Files:**
- Modify: `src/midi/engine.rs`
- Reference: `src/midi/player.rs:536-586` (`find_soundfont`, moved verbatim)

**Interfaces:**
- Produces: `pub fn find_soundfont() -> Result<PathBuf, String>`, `pub fn load_synth(path: &Path) -> Result<Synth, String>`, test helper `tests::engine_with_soundfont() -> Option<(MidiEngine, EngineHandle)>`.

- [ ] **Step 1: Move `find_soundfont`**

Cut the whole `fn find_soundfont` (`src/midi/player.rs:536-586`, from `fn find_soundfont() -> Result<PathBuf, String> {` to its closing brace) and paste it into `src/midi/engine.rs` after `soft_clip`, prefixed with `pub`. In `player.rs` add `use crate::midi::engine::find_soundfont;` so the old code keeps compiling. Then add below it in `engine.rs`:

```rust
/// Parse the SoundFont and build the process's one synthesizer.
pub fn load_synth(path: &Path) -> Result<Synth, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open SoundFont file {:?}: {}", path, e))?;
    let soundfont =
        SoundFont::load(&mut file).map_err(|e| format!("Failed to parse SoundFont: {}", e))?;
    let mut synth = Synth::default();
    synth.set_sample_rate(SAMPLE_RATE as f32);
    synth.set_gain(OXISYNTH_GAIN);
    synth
        .set_polyphony(POLYPHONY)
        .map_err(|e| format!("Failed to set polyphony: {:?}", e))?;
    synth.add_font(soundfont, true);
    tracing::info!("Loaded SoundFont from {:?}", path);
    Ok(synth)
}
```

- [ ] **Step 2: Write the failing tests**

Add to `mod tests`:

```rust
    /// Engine with the real SoundFont, or `None` (after printing why) when it is not installed.
    pub(crate) fn engine_with_soundfont() -> Option<(MidiEngine, EngineHandle)> {
        let path = match find_soundfont() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skipping: SoundFont not installed (run `mcp-muse setup`)");
                return None;
            }
        };
        Some(MidiEngine::new(Some(load_synth(&path).unwrap())))
    }

    fn flute(offset: u64, seconds: f64, pan: Option<u8>) -> Vec<(u64, EventKind)> {
        let mut events = vec![(
            offset,
            EventKind::ProgramChange { channel: 0, program: 73 },
        )];
        if let Some(pan) = pan {
            events.push((offset, EventKind::ControlChange { channel: 0, controller: 10, value: pan }));
        }
        events.push((offset, EventKind::NoteOn { channel: 0, key: 76, velocity: 100 }));
        events.push((
            offset + seconds_to_frames(Duration::from_secs_f64(seconds)),
            EventKind::NoteOff { channel: 0, key: 76 },
        ));
        events
    }

    fn play_events(events: Vec<(u64, EventKind)>, mode: PlayMode) -> EngineCommand {
        EngineCommand::Play(PlayCommand {
            events,
            mode,
            ..Default::default()
        })
    }

    #[test]
    fn a_note_scheduled_at_n_sounds_from_n_not_from_the_chunk_boundary() {
        let Some((mut engine, _handle)) = engine_with_soundfont() else { return };
        engine.apply(play_events(flute(100, 0.5, None), PlayMode::Replace));
        let (left, _) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        let start = LEAD_FRAMES as usize + 100;
        assert!(left[..start].iter().all(|s| *s == 0.0), "sound before the scheduled frame");
        assert!(rms(&left[start..start + 2000]) > 0.001, "no sound after the scheduled frame");
    }

    fn render_panned_flute(pan: u8) -> (f32, f32) {
        let (mut engine, _handle) = engine_with_soundfont().expect("checked by caller");
        engine.apply(play_events(flute(0, 0.5, Some(pan)), PlayMode::Replace));
        let (left, right) = render_all(&mut engine, LEAD_FRAMES as usize + 26_460);
        let window = LEAD_FRAMES as usize + 4410..LEAD_FRAMES as usize + 22_050;
        (rms(&left[window.clone()]), rms(&right[window]))
    }

    #[test]
    fn pan_moves_the_stereo_image() {
        if engine_with_soundfont().is_none() {
            return;
        }
        let (l_hard_left, r_hard_left) = render_panned_flute(0);
        let (l_hard_right, r_hard_right) = render_panned_flute(127);
        assert!(r_hard_right > 0.01, "no audio rendered");
        assert!(
            l_hard_left > l_hard_right * 1.5 && r_hard_right > r_hard_left * 1.5,
            "pan had no effect: pan0=({l_hard_left},{r_hard_left}) pan127=({l_hard_right},{r_hard_right})"
        );
    }

    #[test]
    fn stop_resets_programs_and_channel_9_stays_a_drum_kit() {
        let Some((mut engine, _handle)) = engine_with_soundfont() else { return };
        engine.apply(play_events(flute(0, 0.5, None), PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        assert_eq!(engine.synth().unwrap().program(0).unwrap().2, 73);

        engine.apply(EngineCommand::Stop);
        assert_eq!(engine.synth().unwrap().program(0).unwrap().2, 0, "SystemReset restores program 0");

        engine.apply(play_events(
            vec![
                (0, EventKind::ProgramChange { channel: 9, program: 0 }),
                (0, EventKind::NoteOn { channel: 9, key: 36, velocity: 110 }),
                (4410, EventKind::NoteOff { channel: 9, key: 36 }),
            ],
            PlayMode::Layer,
        ));
        let (left, _) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        assert!(rms(&left[LEAD_FRAMES as usize..]) > 0.001, "kick did not sound");
        let synth = engine.synth().unwrap();
        let kit = synth.channel_preset(9).unwrap();
        let piano = synth.channel_preset(0).unwrap();
        assert_ne!(kit.name(), piano.name(), "channel 9 must draw from the percussion bank");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse midi::engine 2>&1 | tail -20`
Expected: compile error until Step 1 is done; after Step 1 the tests should pass. If `a_note_scheduled_at_n...` fails on "sound before the scheduled frame", `render_unclipped` is applying events at chunk start rather than splitting the render at `until_next`.

- [ ] **Step 4: Run, then commit**

```bash
cargo test --bin mcp-muse midi:: 2>&1 | tail -20
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/engine.rs src/midi/player.rs
git commit -m "feat(engine): load the SoundFont once; sample-accurate MIDI events

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 4: `EngineSource`, the rodio wrapper

**Files:**
- Modify: `src/midi/engine.rs`

**Interfaces:**
- Produces: `pub struct EngineSource`, `EngineSource::new(MidiEngine) -> EngineSource`, implements `rodio::Source` (stereo, 44.1 kHz, infinite).

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn engine_source_is_stereo_interleaved_and_never_ends() {
        use rodio::Source;
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, vec![0.5; 100])], PlayMode::Replace));
        let mut source = EngineSource::new(engine);
        assert_eq!(source.channels(), 2);
        assert_eq!(source.sample_rate(), SAMPLE_RATE);
        assert_eq!(source.total_duration(), None);
        let samples: Vec<f32> = source.by_ref().take(2 * (LEAD_FRAMES as usize + 50)).collect();
        assert_eq!(samples[2 * LEAD_FRAMES as usize], 0.5, "left of the first buffer frame");
        assert_eq!(samples[2 * LEAD_FRAMES as usize + 1], 0.5, "right of the first buffer frame");
        assert!(source.by_ref().take(10 * CHUNK_FRAMES).count() == 10 * CHUNK_FRAMES, "must not end");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --bin mcp-muse midi::engine::tests::engine_source 2>&1 | tail -5`
Expected: compile error, `EngineSource` not found.

- [ ] **Step 3: Implement**

Add after `impl MidiEngine`:

```rust
/// Adapts the engine to rodio: renders a chunk at a time and hands out
/// interleaved stereo samples forever (silence when idle).
pub struct EngineSource {
    engine: MidiEngine,
    left: Vec<f32>,
    right: Vec<f32>,
    frame: usize,
    pending_right: Option<f32>,
}

impl EngineSource {
    pub fn new(engine: MidiEngine) -> Self {
        Self {
            engine,
            left: vec![0.0; CHUNK_FRAMES],
            right: vec![0.0; CHUNK_FRAMES],
            frame: CHUNK_FRAMES, // force a render on the first pull
            pending_right: None,
        }
    }
}

impl Iterator for EngineSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.pending_right.take() {
            return Some(right);
        }
        if self.frame >= CHUNK_FRAMES {
            self.engine.render(&mut self.left, &mut self.right);
            self.frame = 0;
        }
        let (left, right) = (self.left[self.frame], self.right[self.frame]);
        self.frame += 1;
        self.pending_right = Some(right);
        Some(left)
    }
}

impl rodio::Source for EngineSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
```

- [ ] **Step 4: Run, lint, commit**

```bash
cargo test --bin mcp-muse midi::engine 2>&1 | tail -20
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/engine.rs
git commit -m "feat(engine): EngineSource adapts MidiEngine to the rodio mixer

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 5: Translator — MIDI note events

**Files:**
- Modify: `src/midi/translate.rs`

**Interfaces:**
- Produces: `pub(crate) fn midi_events(notes: &[MidiNote]) -> Vec<(u64, EventKind)>`.
- Consumes: `crate::midi::parser::MidiNote` (fields `note, velocity, channel, start_time, duration, instrument, reverb, chorus, volume, pan, balance, expression, sustain`), `EventKind`, `seconds_to_frames`.

- [ ] **Step 1: Write the failing tests**

Replace the contents of `src/midi/translate.rs` with:

```rust
//! Turns a `SimpleSequence` into a `PlayCommand`: presets, musical time,
//! pre-rendered R2D2/synthesis buffers and a time-ordered MIDI event list.
#![allow(dead_code)]

use crate::midi::engine::{EventKind, seconds_to_frames};
use crate::midi::parser::MidiNote;
use std::collections::HashMap;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: f64, dur: f64, instrument: Option<u8>, reverb: Option<u8>) -> MidiNote {
        MidiNote {
            note: 60,
            velocity: 100,
            channel: 0,
            start_time: Duration::from_secs_f64(start),
            duration: Duration::from_secs_f64(dur),
            instrument,
            reverb,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
        }
    }

    #[test]
    fn setup_events_precede_note_on_and_are_deduplicated_per_channel() {
        let events = midi_events(&[
            note(0.25, 0.5, None, Some(40)),
            note(0.0, 0.5, Some(73), Some(40)),
        ]);
        assert_eq!(
            events,
            vec![
                (0, EventKind::ProgramChange { channel: 0, program: 73 }),
                (0, EventKind::ControlChange { channel: 0, controller: 91, value: 40 }),
                (0, EventKind::NoteOn { channel: 0, key: 60, velocity: 100 }),
                (11_025, EventKind::NoteOn { channel: 0, key: 60, velocity: 100 }),
                (22_050, EventKind::NoteOff { channel: 0, key: 60 }),
                (33_075, EventKind::NoteOff { channel: 0, key: 60 }),
            ]
        );
    }

    #[test]
    fn an_unspecified_instrument_means_program_0_on_first_use_only() {
        let events = midi_events(&[note(0.0, 0.1, None, None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(programs, vec![&(0, EventKind::ProgramChange { channel: 0, program: 0 })]);

        let events = midi_events(&[note(0.0, 0.1, Some(48), None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(programs.len(), 1, "a later note without instrument keeps the channel's program");
    }

    #[test]
    fn a_zero_length_note_still_gets_its_note_off_after_note_on() {
        let events = midi_events(&[note(0.0, 0.0, Some(0), None)]);
        assert_eq!(events[1], (0, EventKind::NoteOn { channel: 0, key: 60, velocity: 100 }));
        assert_eq!(events[2], (1, EventKind::NoteOff { channel: 0, key: 60 }));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse midi::translate 2>&1 | tail -10`
Expected: compile error, `midi_events` not found.

- [ ] **Step 3: Implement**

Insert above `#[cfg(test)]`:

```rust
/// GM controller numbers the note schema exposes, in the order they are sent.
const CONTROLLERS: [(u8, fn(&MidiNote) -> Option<u8>); 7] = [
    (91, |n| n.reverb),
    (93, |n| n.chorus),
    (7, |n| n.volume),
    (10, |n| n.pan),
    (8, |n| n.balance),
    (11, |n| n.expression),
    (64, |n| n.sustain),
];

/// Time-ordered MIDI events for one call. Per channel, the first note sends a
/// program change (its instrument, or 0 so an unspecified instrument is still
/// piano when layering); controllers are sent only when a note specifies a
/// value that differs from what this call last sent. Channel 9 needs no bank
/// select: OxiSynth treats it as the drum channel and resolves bank 128.
pub(crate) fn midi_events(notes: &[MidiNote]) -> Vec<(u64, EventKind)> {
    let mut sorted: Vec<&MidiNote> = notes.iter().collect();
    sorted.sort_by_key(|n| n.start_time);

    let mut program: HashMap<u8, u8> = HashMap::new();
    let mut controller: HashMap<(u8, u8), u8> = HashMap::new();
    let mut events = Vec::with_capacity(notes.len() * 3);

    for note in sorted {
        let at = seconds_to_frames(note.start_time);
        let channel = note.channel;
        let wanted = match note.instrument {
            Some(p) => Some(p),
            None if !program.contains_key(&channel) => Some(0),
            None => None,
        };
        if let Some(p) = wanted
            && program.get(&channel) != Some(&p)
        {
            events.push((at, EventKind::ProgramChange { channel, program: p }));
            program.insert(channel, p);
        }
        for (number, get) in CONTROLLERS {
            if let Some(value) = get(note)
                && controller.get(&(channel, number)) != Some(&value)
            {
                events.push((at, EventKind::ControlChange { channel, controller: number, value }));
                controller.insert((channel, number), value);
            }
        }
        events.push((at, EventKind::NoteOn { channel, key: note.note, velocity: note.velocity }));
        let off = at + seconds_to_frames(note.duration).max(1);
        events.push((off, EventKind::NoteOff { channel, key: note.note }));
    }

    // Stable: keeps setup-before-note-on and off-before-next-on at equal frames.
    events.sort_by_key(|(at, _)| *at);
    events
}
```

- [ ] **Step 4: Run, lint, commit**

```bash
cargo test --bin mcp-muse midi::translate 2>&1 | tail -10
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/translate.rs
git commit -m "feat(translate): time-ordered MIDI events with per-call channel state

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 6: Translator — whole sequence to `PlayCommand`

Moves the sequence-processing code out of `player.rs` unchanged in behaviour, and pre-renders R2D2/synthesis buffers where `EnhancedHybridAudioSource::new` used to.

**Files:**
- Modify: `src/midi/translate.rs`
- Reference (move from): `src/midi/player.rs:62-128` (`calculate_tail_time`), `:130-279` (`apply_preset_to_note`), `:281-500` (body of `play_enhanced_mixed` up to `let total_time`), `:1090-1150` (R2D2 and synthesis pre-render loops in `EnhancedHybridAudioSource::new`), `:1167-1392` (`convert_simple_note_to_synth_params`).

**Interfaces:**
- Produces: `pub struct Translator`, `Translator::new(midi_available: Result<(), String>) -> Translator`, `Translator::translate(&self, SimpleSequence, PlayMode) -> Result<Translation, String>`, `pub struct Translation { pub command: PlayCommand, pub duration: Duration }`.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `translate.rs`:

```rust
    use crate::midi::engine::{PlayMode, SYNTH_BUS_GAIN};
    use crate::midi::{SimpleNote, SimpleSequence};

    fn seq(notes: Vec<SimpleNote>) -> SimpleSequence {
        SimpleSequence {
            notes,
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    #[test]
    fn synthesis_notes_become_buffers_at_bus_level() {
        let translator = Translator::new(Ok(()));
        let t = translator
            .translate(
                seq(vec![SimpleNote {
                    synth_type: Some("sine".into()),
                    synth_frequency: Some(440.0),
                    synth_amplitude: Some(0.8),
                    start_time: Some(0.5),
                    duration: Some(0.2),
                    ..Default::default()
                }]),
                PlayMode::Layer,
            )
            .unwrap();
        assert_eq!(t.command.mode, PlayMode::Layer);
        assert!(t.command.events.is_empty());
        assert_eq!(t.command.buffers.len(), 1);
        let (offset, samples) = &t.command.buffers[0];
        assert_eq!(*offset, 22_050);
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.8 * SYNTH_BUS_GAIN + 0.01, "bus gain not applied: peak {peak}");
        assert!(t.duration >= Duration::from_secs_f64(0.7), "duration must include the tail");
    }

    #[test]
    fn midi_notes_need_a_soundfont_but_synthesis_does_not() {
        let translator = Translator::new(Err("SoundFont not found".into()));
        let midi = seq(vec![SimpleNote {
            note: Some(60),
            duration: Some(0.1),
            ..Default::default()
        }]);
        let err = translator.translate(midi, PlayMode::Replace).unwrap_err();
        assert!(err.contains("SoundFont not found"), "{err}");

        let synth = seq(vec![SimpleNote {
            synth_type: Some("sine".into()),
            synth_frequency: Some(440.0),
            duration: Some(0.1),
            ..Default::default()
        }]);
        assert!(translator.translate(synth, PlayMode::Replace).is_ok());
    }

    #[test]
    fn presets_are_applied_and_unknown_presets_are_errors() {
        let translator = Translator::new(Ok(()));
        let ok = translator.translate(
            seq(vec![SimpleNote {
                preset_name: Some("Minimoog Bass".into()),
                note: Some(36),
                duration: Some(0.2),
                ..Default::default()
            }]),
            PlayMode::Replace,
        );
        assert_eq!(ok.unwrap().command.buffers.len(), 1);

        let err = translator
            .translate(
                seq(vec![SimpleNote {
                    preset_name: Some("Definitely Not A Preset".into()),
                    note: Some(36),
                    ..Default::default()
                }]),
                PlayMode::Replace,
            )
            .unwrap_err();
        assert!(err.contains("Definitely Not A Preset"), "{err}");
    }

    #[test]
    fn musical_time_is_converted_with_the_sequence_tempo() {
        let translator = Translator::new(Ok(()));
        let mut s = seq(vec![SimpleNote {
            note: Some(60),
            musical_time: Some(crate::midi::MusicalTime { bar: 2, beat: 1, tick: 0 }),
            musical_duration: Some(crate::midi::MusicalDuration::Bars(1.0)),
            ..Default::default()
        }]);
        s.tempo = 60;
        s.beats_per_bar = 4;
        let t = translator.translate(s, PlayMode::Replace).unwrap();
        let on = t.command.events.iter().find(|(_, e)| matches!(e, EventKind::NoteOn { .. })).unwrap();
        assert_eq!(on.0, 4 * 44_100, "bar 2 at 60 BPM in 4/4 starts at 4 s");
        let off = t.command.events.iter().find(|(_, e)| matches!(e, EventKind::NoteOff { .. })).unwrap();
        assert_eq!(off.0, 8 * 44_100);
    }

    #[test]
    fn an_empty_sequence_translates_to_nothing() {
        let t = Translator::new(Ok(())).translate(seq(vec![]), PlayMode::Replace).unwrap();
        assert!(t.command.events.is_empty() && t.command.buffers.is_empty());
        assert_eq!(t.duration, Duration::ZERO);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin mcp-muse midi::translate 2>&1 | tail -10`
Expected: compile error, `Translator` not found.

- [ ] **Step 3: Implement `Translator`**

Add these imports at the top of `translate.rs`:

```rust
use crate::expressive::{
    EffectsChain, EffectsPresetLibrary, ExpressiveSynth, PresetLibrary, R2D2Emotion,
    R2D2Expression, R2D2Voice,
};
use crate::midi::engine::{PlayCommand, PlayMode, SAMPLE_RATE, SYNTH_BUS_GAIN};
use crate::midi::{EffectConfig, SimpleNote, SimpleSequence};
```

Then add:

```rust
/// Result of translating one call.
pub struct Translation {
    pub command: PlayCommand,
    /// Time until the last note plus its effect tail; reported to the caller.
    pub duration: Duration,
}

pub struct Translator {
    preset_library: PresetLibrary,
    effects_library: EffectsPresetLibrary,
    /// `Err(reason)` when no SoundFont is loaded; MIDI notes then fail here.
    midi_available: Result<(), String>,
}

impl Translator {
    pub fn new(midi_available: Result<(), String>) -> Self {
        Self {
            preset_library: PresetLibrary::new(),
            effects_library: EffectsPresetLibrary::new(),
            midi_available,
        }
    }

    pub fn translate(
        &self,
        sequence: SimpleSequence,
        mode: PlayMode,
    ) -> Result<Translation, String> {
        if sequence.notes.is_empty() {
            return Ok(Translation {
                command: PlayCommand { mode, ..Default::default() },
                duration: Duration::ZERO,
            });
        }

        // === moved from player.rs:293-343 (preset application, effects
        // validation, musical_time / musical_duration conversion) ===
        let mut processed_notes = Vec::new();
        for (i, mut note) in sequence.notes.into_iter().enumerate() {
            self.apply_preset_to_note(&mut note)
                .map_err(|e| format!("Note {}: {}", i + 1, e))?;
            if let Err(e) = note.validate_effects() {
                tracing::warn!("Invalid effects on note: {}", e);
                note.effects = None;
                note.effects_preset = None;
            }
            if note.start_time.is_none()
                && let Some(musical_time) = &note.musical_time
            {
                note.start_time =
                    Some(musical_time.to_seconds(sequence.tempo, sequence.beats_per_bar, 480));
            }
            if note.duration.is_none()
                && let Some(ref musical_duration) = note.musical_duration
            {
                note.duration =
                    Some(musical_duration.to_seconds(sequence.tempo, sequence.beats_per_bar));
            }
            processed_notes.push(note);
        }

        // MIDI bus chain: first MIDI note with a non-empty effects list.
        let midi_effects: Option<Vec<EffectConfig>> = processed_notes
            .iter()
            .filter(|n| n.note_type != "r2d2" && !n.is_synthesis())
            .find_map(|n| n.effects.clone().filter(|e| !e.is_empty()));

        let mut midi_notes: Vec<MidiNote> = Vec::new();
        let mut buffers: Vec<(u64, Vec<f32>)> = Vec::new();
        let mut note_end = Duration::ZERO;
        let expressive_synth = ExpressiveSynth::new();
        let r2d2_voice = R2D2Voice::new();

        for note in processed_notes {
            let start = Duration::from_secs_f64(note.start_time.unwrap_or(0.0));
            if note.note_type == "r2d2" {
                // === moved from player.rs:378-425 (R2D2 expression) and
                // :1090-1120 (R2D2 pre-render) ===
                note.validate_r2d2().map_err(|e| format!("Invalid R2D2 note: {}", e))?;
                let emotion = parse_emotion(note.r2d2_emotion.as_deref().ok_or("R2D2 emotion is required")?)?;
                let expression = R2D2Expression {
                    emotion,
                    intensity: note.r2d2_intensity.unwrap_or(0.7),
                    duration: note.duration.unwrap_or(1.0) as f32,
                    phrase_complexity: note.r2d2_complexity.unwrap_or(2),
                    pitch_range: match &note.r2d2_pitch_range {
                        Some(range) if range.len() == 2 => (range[0], range[1]),
                        _ => (200.0, 800.0),
                    },
                    context: note.r2d2_context,
                };
                let params = r2d2_voice
                    .generate_expression_params(&expression)
                    .ok_or("Failed to generate R2D2 synthesis parameters")?;
                let mut samples = expressive_synth.generate_r2d2_samples_with_contour(
                    params.base_freq,
                    expression.intensity,
                    params.duration,
                    &params.pitch_contour,
                );
                let mut chain =
                    EffectsChain::new(SAMPLE_RATE as f32, note.effects.as_deref().unwrap_or(&[]));
                if !chain.is_empty() {
                    samples.resize(samples.len() + SAMPLE_RATE as usize, 0.0);
                    chain.process_buffer(&mut samples);
                }
                note_end = note_end.max(start + Duration::from_secs_f32(expression.duration));
                buffers.push((seconds_to_frames(start), samples));
            } else if note.is_synthesis() {
                // === moved from player.rs:426-436 and :1123-1145 ===
                note.validate_synthesis().map_err(|e| format!("Invalid synthesis note: {}", e))?;
                let params = Self::convert_simple_note_to_synth_params(&note)?;
                let mut samples = expressive_synth
                    .generate_synthesized_samples(&params)
                    .map_err(|e| format!("Failed to generate synthesis samples: {}", e))?;
                for s in &mut samples {
                    *s *= SYNTH_BUS_GAIN;
                }
                note_end = note_end.max(start + Duration::from_secs_f64(note.duration.unwrap_or(1.0)));
                buffers.push((seconds_to_frames(start), samples));
            } else if let Some(key) = note.note {
                // === moved from player.rs:437-470 ===
                let duration = Duration::from_secs_f64(note.duration.unwrap_or(1.0));
                note_end = note_end.max(start + duration);
                midi_notes.push(MidiNote {
                    note: key,
                    velocity: note.velocity.unwrap_or(80),
                    channel: note.channel,
                    start_time: start,
                    duration,
                    instrument: note.instrument,
                    reverb: note.reverb,
                    chorus: note.chorus,
                    volume: note.volume,
                    pan: note.pan,
                    balance: note.balance,
                    expression: note.expression,
                    sustain: note.sustain,
                });
            }
        }

        if !midi_notes.is_empty()
            && let Err(reason) = &self.midi_available
        {
            return Err(format!("MIDI notes need a SoundFont: {}", reason));
        }

        let duration = if midi_notes.is_empty() && buffers.is_empty() {
            Duration::ZERO
        } else {
            note_end + calculate_tail_time(&midi_notes)
        };
        tracing::info!(
            "Translated {} MIDI notes and {} buffers ({} mode), {:.2}s including tail",
            midi_notes.len(),
            buffers.len(),
            mode.as_str(),
            duration.as_secs_f64()
        );

        Ok(Translation {
            command: PlayCommand {
                events: midi_events(&midi_notes),
                buffers,
                midi_effects,
                mode,
            },
            duration,
        })
    }
}

fn parse_emotion(name: &str) -> Result<R2D2Emotion, String> {
    Ok(match name {
        "Happy" => R2D2Emotion::Happy,
        "Sad" => R2D2Emotion::Sad,
        "Excited" => R2D2Emotion::Excited,
        "Worried" => R2D2Emotion::Worried,
        "Curious" => R2D2Emotion::Curious,
        "Affirmative" => R2D2Emotion::Affirmative,
        "Negative" => R2D2Emotion::Negative,
        "Surprised" => R2D2Emotion::Surprised,
        "Thoughtful" => R2D2Emotion::Thoughtful,
        _ => return Err(format!("Unknown R2D2 emotion: {}", name)),
    })
}
```

Then move, verbatim, into `translate.rs`:
- `fn calculate_tail_time(notes: &[MidiNote]) -> Duration` from `player.rs:62-128` as a free function (drop the `Self::`).
- `fn apply_preset_to_note(&self, note: &mut SimpleNote) -> Result<(), String>` from `player.rs:130-279` into `impl Translator` (it uses `self.preset_library` and `self.effects_library`, both present).
- `fn convert_simple_note_to_synth_params(note: &SimpleNote) -> Result<SynthParams, String>` from `player.rs:1167-1392` into `impl Translator` as an associated function.

Leave the originals in `player.rs` for now (Task 8 deletes the file's old body wholesale); duplicate private code compiles fine.

- [ ] **Step 4: Run, lint, commit**

```bash
cargo test --bin mcp-muse midi::translate 2>&1 | tail -15
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/translate.rs
git commit -m "feat(translate): sequence to PlayCommand with pre-rendered buffers

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 7: Headroom tests through the new pipeline

Moves `level_tests` from `player.rs` onto translator + engine so they no longer need an audio device, before the old code is deleted.

**Files:**
- Modify: `src/midi/player.rs` (replace `mod level_tests`, lines 1584-1740)

**Interfaces:**
- Consumes: `Translator`, `MidiEngine::apply`, `MidiEngine::render_unclipped`, `engine::tests::engine_with_soundfont` (make it `pub(crate)`; it already is).

- [ ] **Step 1: Rewrite `measure`**

Replace the `measure` function inside `mod level_tests` with:

```rust
    /// Peak and fraction of samples above the clipper knee, before clipping.
    fn measure(seq: SimpleSequence) -> (f32, f32) {
        use crate::midi::engine::{CHUNK_FRAMES, EngineCommand, LEAD_FRAMES, PlayMode};
        use crate::midi::translate::Translator;
        let (mut engine, _handle) =
            crate::midi::engine::tests::engine_with_soundfont().expect("caller checked");
        let translation = Translator::new(Ok(()))
            .translate(seq, PlayMode::Replace)
            .unwrap();
        engine.apply(EngineCommand::Play(translation.command));

        let total = LEAD_FRAMES as usize + 66_150; // 1.5 s of material
        let (mut l, mut r) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
        let (mut peak, mut over, mut count) = (0.0f32, 0usize, 0usize);
        let mut rendered = 0;
        while rendered < total {
            engine.render_unclipped(&mut l, &mut r);
            rendered += CHUNK_FRAMES;
            for v in l.iter().chain(r.iter()) {
                peak = peak.max(v.abs());
                if v.abs() > 0.8 {
                    over += 1;
                }
                count += 1;
            }
        }
        (peak, over as f32 / count as f32)
    }
```

Delete the old `measure` body's `MidiPlayer::new()`, `apply_preset_to_note`, `SynthEvent`, `MidiNote` and `EnhancedHybridAudioSource` usage entirely. Replace `find_soundfont().is_err()` guards with `crate::midi::engine::find_soundfont().is_err()`. The `midi(...)` and `preset(...)` helpers and the two `#[test]` functions stay as they are.

- [ ] **Step 2: Run the level tests**

Run: `cargo test --bin mcp-muse level_tests -- --nocapture 2>&1 | grep -E "LEVEL|test result"`
Expected: same LEVEL lines as before and `test result: ok`. The thresholds (0.6 / 0.8 / 1.0 peaks, <1 % over the knee, >0.15 floor) are unchanged; if `drums` moves, confirm channel 9 still resolves the kit (Task 3 test) before touching thresholds.

- [ ] **Step 3: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/player.rs
git commit -m "test(player): headroom checks render through Translator + MidiEngine

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 8: Rewrite `MidiPlayer` on the engine; delete the old pipeline

**Files:**
- Modify: `src/midi/player.rs` (everything above `mod level_tests` is replaced)
- Modify: `src/demos.rs` (36 call sites)
- Modify: `src/midi/engine.rs`, `src/midi/translate.rs` (remove `#![allow(dead_code)]`)

**Interfaces:**
- Produces: `MidiPlayer::new() -> Result<MidiPlayer, String>`, `MidiPlayer::play(&mut self, SimpleSequence, PlayMode) -> Result<Duration, String>`, `MidiPlayer::stop_all(&mut self) -> usize`, `MidiPlayer::active_playbacks(&mut self) -> usize`, `pub(crate) fn count_active(ends: &[u64], now: u64) -> usize`.
- Removes: `play_enhanced_mixed`, `OxiSynthSource`, `EnhancedHybridAudioSource`, `ChannelProcessor`, `R2D2Event`, `SynthEvent`, `MIDI_GAIN`.

- [ ] **Step 1: Write the failing test**

Add a new test module to `player.rs` (keep `level_tests` below it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_active_counts_playbacks_whose_end_is_in_the_future() {
        assert_eq!(count_active(&[], 10), 0);
        assert_eq!(count_active(&[5, 10, 11, 500], 10), 2);
    }
}
```

- [ ] **Step 2: Replace the top of `player.rs`**

Replace everything from line 1 to the line before `#[cfg(test)]\nmod level_tests` with:

```rust
//! Owns the audio output for the process and the one `MidiEngine` running on
//! it. Tool calls translate sequences into engine commands; playback state
//! lives on the audio thread.
use crate::midi::SimpleSequence;
use crate::midi::engine::{
    EngineCommand, EngineHandle, EngineSource, LEAD_FRAMES, MidiEngine, PlayMode, find_soundfont,
    load_synth, seconds_to_frames,
};
use crate::midi::translate::{Translation, Translator};
use rodio::OutputStream;
use std::time::Duration;

pub struct MidiPlayer {
    /// Kept alive for the process; dropping it closes the device.
    _stream: OutputStream,
    engine: EngineHandle,
    translator: Translator,
    /// Engine frame at which each started playback ends (including tail).
    playback_ends: Vec<u64>,
}

impl MidiPlayer {
    /// Open the output device, load the SoundFont once, and attach the engine
    /// to the mixer. Without a SoundFont, synthesis and R2D2 still work.
    pub fn new() -> Result<Self, String> {
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|e| format!("Failed to create audio output stream: {}", e))?;

        let (synth, midi_available) = match find_soundfont().and_then(|p| load_synth(&p)) {
            Ok(synth) => (Some(synth), Ok(())),
            Err(reason) => {
                tracing::warn!("MIDI unavailable: {}", reason);
                (None, Err(reason))
            }
        };
        let (engine, handle) = MidiEngine::new(synth);
        stream.mixer().add(EngineSource::new(engine));

        Ok(MidiPlayer {
            _stream: stream,
            engine: handle,
            translator: Translator::new(midi_available),
            playback_ends: Vec::new(),
        })
    }

    /// Schedule a sequence. Returns the time until it finishes, including
    /// effect tails. `Replace` cuts whatever is playing first.
    pub fn play(&mut self, sequence: SimpleSequence, mode: PlayMode) -> Result<Duration, String> {
        let Translation { command, duration } = self.translator.translate(sequence, mode)?;
        if command.events.is_empty() && command.buffers.is_empty() {
            tracing::warn!("Nothing to play");
            return Ok(Duration::ZERO);
        }
        let now = self.engine.clock();
        if mode == PlayMode::Replace {
            self.playback_ends.clear();
        }
        self.engine.send(EngineCommand::Play(command))?;
        self.playback_ends
            .push(now + LEAD_FRAMES + seconds_to_frames(duration));
        tracing::info!(
            "Playback started ({}) - duration: {:.2}s, active playbacks: {}",
            mode.as_str(),
            duration.as_secs_f64(),
            self.playback_ends.len()
        );
        Ok(duration)
    }

    /// Stop everything. Returns how many started playbacks had not finished.
    pub fn stop_all(&mut self) -> usize {
        let active = count_active(&self.playback_ends, self.engine.clock());
        self.playback_ends.clear();
        if let Err(e) = self.engine.send(EngineCommand::Stop) {
            tracing::warn!("Stop failed: {}", e);
        }
        tracing::info!("Stopped {} active playbacks", active);
        active
    }

    /// Number of started playbacks that have not reached their end frame.
    #[allow(dead_code)]
    pub fn active_playbacks(&mut self) -> usize {
        let now = self.engine.clock();
        self.playback_ends.retain(|&end| end > now);
        self.playback_ends.len()
    }
}

pub(crate) fn count_active(ends: &[u64], now: u64) -> usize {
    ends.iter().filter(|&&end| end > now).count()
}
```

- [ ] **Step 3: Update the demos**

```bash
sed -i '' 's/player\.play_enhanced_mixed(\([A-Za-z0-9_]*\))/player.play(\1, PlayMode::Layer)/g' src/demos.rs
sed -i '' 's/^use crate::midi::{MidiPlayer, SimpleNote, SimpleSequence};/use crate::midi::{MidiPlayer, PlayMode, SimpleNote, SimpleSequence};/' src/demos.rs
grep -c "play_enhanced_mixed" src/demos.rs   # expect 0
```

`Layer` reproduces the previous demo behaviour (overlapping calls mix).

- [ ] **Step 4: Remove the dead-code allowances**

Delete the `#![allow(dead_code)]` line from `src/midi/engine.rs` and `src/midi/translate.rs`. The old `find_soundfont` import in `player.rs` from Task 3 is superseded by the new `use` block.

`src/server/mcp.rs:872` still calls `play_enhanced_mixed`. Change that one line to

```rust
    match player.play(sequence, crate::midi::PlayMode::Replace) {
```

so the server builds with the default mode; Task 9 replaces this with the parsed `mode`.

- [ ] **Step 5: Build, test, lint**

```bash
cargo build 2>&1 | tail -20
cargo test --bin mcp-muse 2>&1 | tail -20
cargo fmt && cargo clippy --all-targets -- -D warnings
```
Expected: no references to `OxiSynthSource`, `EnhancedHybridAudioSource`, `MIDI_GAIN` remain (`grep -rn "OxiSynthSource\|EnhancedHybridAudioSource\|MIDI_GAIN\|play_enhanced_mixed" src` prints nothing); all unit tests pass; clippy clean, including no `dead_code` warnings.

- [ ] **Step 6: Commit**

```bash
git add src/midi src/demos.rs src/server/mcp.rs
git commit -m "feat(player): one long-lived MidiEngine per process; delete per-call sources

Every play call used to build its own OxiSynthSource (150 MB SoundFont copy)
and Sink, and overlapping calls ran N synthesizers in the audio callback
(crackling reported in #95). The player now translates sequences into engine
commands; replace/layer semantics and a real stop live on the engine.

Closes #95

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 9: MCP tool surface: `mode` argument, schema, texts

**Files:**
- Modify: `src/server/mcp.rs:4` (imports), `:700-732` (play_notes schema), `:640-660` (play_sequence schema), `:851-909` (`playback_started_text`, `start_playback`, `handle_play_notes`), `:962-1010` (`handle_play_sequence`)
- Test: `tests/integration/mcp_protocol.rs`

**Interfaces:**
- Consumes: `MidiPlayer::play(sequence, mode)`, `PlayMode`.

- [ ] **Step 1: Write the failing integration tests**

Append to `tests/integration/mcp_protocol.rs`:

```rust
#[test]
fn play_mode_is_in_both_schemas_and_validated() {
    let mut server = TestServer::start();
    let tools = server.call(json!({"jsonrpc": "2.0", "id": 30, "method": "tools/list"}));
    for name in ["play_notes", "play_sequence"] {
        let tool = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        let mode = &tool["inputSchema"]["properties"]["mode"];
        assert_eq!(mode["type"], "string", "{name}: {mode}");
        assert_eq!(mode["enum"], json!(["replace", "layer"]), "{name}");
        assert_eq!(mode["default"], "replace", "{name}");
    }

    let bad = server.call(json!({
        "jsonrpc": "2.0", "id": 31, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {
            "mode": "queue",
            "notes": [{"note": 60, "duration": 0.1}]
        }}
    }));
    assert_eq!(bad["error"]["code"], -32602, "{bad}");
    assert!(bad["error"]["message"].as_str().unwrap().contains("queue"));
}

#[test]
fn consecutive_plays_layer_or_replace_and_stop_reports_the_count() {
    let mut server = TestServer::start();
    let note = json!([{"synth_type": "sine", "synth_frequency": 440, "duration": 3.0}]);
    let first = server.call(json!({
        "jsonrpc": "2.0", "id": 32, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": note}}
    }));
    if first["result"]["isError"] == true {
        eprintln!("skipping: {}", first["result"]["content"][0]["text"]);
        return; // no audio device (CI)
    }
    let text = first["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("replace"), "{text}");

    let second = server.call(json!({
        "jsonrpc": "2.0", "id": 33, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"mode": "layer", "notes": note}}
    }));
    let text = second["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("layer"), "{text}");

    let stop = server.call(json!({
        "jsonrpc": "2.0", "id": 34, "method": "tools/call",
        "params": {"name": "stop_playback", "arguments": {}}
    }));
    let text = stop["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Stopped 2"), "{text}");

    let third = server.call(json!({
        "jsonrpc": "2.0", "id": 35, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": note}}
    }));
    assert!(third["result"]["isError"].is_null(), "{third}");
    let stop = server.call(json!({
        "jsonrpc": "2.0", "id": 36, "method": "tools/call",
        "params": {"name": "stop_playback", "arguments": {}}
    }));
    let text = stop["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Stopped 1"), "replace should have dropped the earlier count: {text}");
}
```

- [ ] **Step 2: Implement in `mcp.rs`**

Import: change line 4 to
```rust
use crate::midi::{
    ExtendedSequence, MidiPlayer, PlayMode, SequencePattern, SimpleNote, SimpleSequence,
};
```

Add above `playback_started_text`:

```rust
/// `mode` is read separately from the sequence structs so patterns and the
/// demos keep their plain data shapes; unknown values are a parameter error.
fn parse_mode(arguments: &Value) -> Result<PlayMode, String> {
    match arguments.get("mode") {
        None | Some(Value::Null) => Ok(PlayMode::default()),
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| {
            format!(
                "Invalid mode {}: expected \"replace\" or \"layer\"",
                value
            )
        }),
    }
}
```

Replace `playback_started_text` and `start_playback`:

```rust
fn playback_started_text(summary: String, duration: Duration, mode: PlayMode) -> String {
    let how = match mode {
        PlayMode::Replace => "replaced what was playing",
        PlayMode::Layer => "layered over the current playback",
    };
    format!(
        "🎵 Playback started ({}; {}). It will finish in about {:.1} seconds including effect tails; call stop_playback to cut it short.",
        summary,
        how,
        duration.as_secs_f64()
    )
}

fn start_playback(
    state: &mut ServerState,
    sequence: SimpleSequence,
    mode: PlayMode,
    id: Option<Value>,
    summary: String,
) -> JsonRpcResponse {
    let player = match state.player() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Audio output unavailable: {}", e);
            return JsonRpcResponse::tool_error(id, format!("Audio output unavailable: {}", e));
        }
    };
    match player.play(sequence, mode) {
        Ok(duration) => {
            JsonRpcResponse::tool_text(id, playback_started_text(summary, duration, mode))
        }
        Err(e) => {
            tracing::error!("Playback failed: {}", e);
            JsonRpcResponse::tool_error(id, format!("Playback failed: {}", e))
        }
    }
}
```

In `handle_play_notes` and `handle_play_sequence`, before the `serde_json::from_value(arguments)` line, add:

```rust
    let mode = match parse_mode(&arguments) {
        Ok(mode) => mode,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
```

and change the final calls to `start_playback(state, sequence, mode, id, summary)` and `start_playback(state, resolved, mode, id, summary)`.

Schema: in the `play_notes` tool's `properties` (after `beats_per_bar`) and in the `play_sequence` tool's `properties` (after `beats_per_bar`), add:

```json
"mode": {
    "type": "string",
    "enum": ["replace", "layer"],
    "default": "replace",
    "description": "replace (default) stops whatever is playing before this starts; layer mixes this on top of the current playback"
}
```

Also update the `play_notes` description's trailing sentence to mention it: append `\n\nPass \"mode\": \"layer\" to play over what is already sounding; the default replaces it.` to the description string.

- [ ] **Step 3: Run the integration tests**

Run: `cargo test --test integration_tests play_mode consecutive_plays 2>&1 | tail -15`
Expected: both pass locally (the second one exercises the real device; on CI it prints "skipping").

- [ ] **Step 4: Full verification and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -25
git add src/server/mcp.rs tests/integration/mcp_protocol.rs
git commit -m "feat(mcp): mode argument (replace|layer) on play_notes and play_sequence

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

---

### Task 10: Docs, spec deviations, listen-by-ear check

**Files:**
- Modify: `CLAUDE.md:49-62`, `README.md:498-505`, `docs/superpowers/specs/2026-09-05-single-midi-engine-design.md`

- [ ] **Step 1: CLAUDE.md**

Replace the "Audio pipeline" section (from `### Audio pipeline` to the "Known limitation" paragraph inclusive) with:

```markdown
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
```

Also in "Important Architectural Decisions" change `**No audio stream per call**: \`ServerState\` owns one \`MidiPlayer\` for the process.` to `**One engine per process**: \`ServerState\` owns one \`MidiPlayer\`, which owns the stream and the single \`MidiEngine\`; never create a synthesizer per call.`

- [ ] **Step 2: README**

In the Tools list change the `play_notes` and `play_sequence` bullets to mention the mode, e.g. after the `stop_playback` bullet add:

```markdown
- Both play tools take `"mode": "replace"` (default: stop what is playing first) or `"mode": "layer"` (mix on top). One synthesizer serves the whole session, so overlapping calls do not multiply CPU or memory.
```

- [ ] **Step 3: Spec deviations**

Append to the spec's "Tool surface" section:

```markdown
Implementation notes (2026-09-05): `mode` is parsed from the tool arguments
by `parse_mode` in `mcp.rs` rather than stored on the sequence structs, so
the 40+ struct literals in demos and tests are untouched. The engine picks
`start = clock + LEAD_FRAMES` itself when it applies a command, so the tool
thread never schedules against a stale clock. The channel-9 bank-select
sequence was dropped: OxiSynth's `drums_channel_active` (default on) makes
program changes on channel 9 resolve in bank 128.
```

- [ ] **Step 4: Listen-by-ear**

```bash
cargo build --release
cargo run --release -- test-presets     # presets still sound right, no clicks between demos
cargo run --release -- test-drums
```

Then the issue scenario through the MCP server (three overlapping calls, replace then layer): start `cargo run --release` and paste, one per line:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"play_notes","arguments":{"notes":[{"note":60,"instrument":48,"duration":6},{"note":64,"instrument":48,"duration":6},{"note":67,"instrument":48,"duration":6}]}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"play_notes","arguments":{"mode":"layer","notes":[{"note":36,"channel":9,"duration":0.2},{"note":38,"channel":9,"start_time":0.5,"duration":0.2},{"note":36,"channel":9,"start_time":1.0,"duration":0.2}]}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"play_notes","arguments":{"notes":[{"note":72,"instrument":73,"duration":2}]}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"stop_playback","arguments":{}}}
```

Expect: strings chord, drums layered over it, then the flute replaces both with a short fade and no click, and stop silences it. Check memory: `ps -o rss= -p $(pgrep -f "target/release/mcp-muse")` should stay around one SoundFont (~200 MB) across all calls.

- [ ] **Step 5: Final verification and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -5
git add CLAUDE.md README.md docs/superpowers/specs/2026-09-05-single-midi-engine-design.md
git commit -m "docs: describe the single MIDI engine and play modes

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018oJbeDaEHJQ8sLgeA1qYQs"
```

Then follow `superpowers:finishing-a-development-branch` (PR to `main`, body ends with the generated-with footer and `Closes #95`).
