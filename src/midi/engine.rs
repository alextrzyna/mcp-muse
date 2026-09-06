//! One long-lived MIDI/synthesis engine for the process (GitHub issue #95).
//!
//! `MidiEngine` owns the single OxiSynth instance, a queue of MIDI events keyed
//! to its sample clock, and the pre-rendered R2D2/synthesis buffers scheduled
//! on a shared mono bus. Tool calls send `EngineCommand`s over a channel; the
//! engine drains them at chunk boundaries and applies events at their exact
//! sample.
#![allow(dead_code)]

use crate::expressive::EffectsChain;
use crate::midi::EffectConfig;
use oxisynth::{MidiEvent, Synth};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc::{Receiver, Sender, channel};
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
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
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
        // Stops on both `Empty` and `Disconnected`: a dropped handle simply
        // means no more commands will arrive, and rendering carries on.
        while let Ok(command) = self.commands.try_recv() {
            self.apply(command);
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
            "Scheduled {} at frame {}: {} MIDI events, {} buffers queued",
            play.mode.as_str(),
            start,
            self.events.len(),
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
        let mut tail: Vec<(f32, f32)> = left
            .iter()
            .zip(&right)
            .enumerate()
            .map(|(i, (&l, &r))| {
                let gain = 1.0 - (i + 1) as f32 / FADE_FRAMES as f32;
                (l * gain, r * gain)
            })
            .collect();
        // A previous fade may still be playing out - two resets in one command
        // drain, or a replace during an earlier fade. Sum the unconsumed
        // remainder into the new tail instead of discarding it; in the
        // same-drain case the new tail is silent and only the old one is heard.
        for (new, &(old_l, old_r)) in tail
            .iter_mut()
            .zip(self.fade_tail.iter().skip(self.fade_pos))
        {
            new.0 += old_l;
            new.1 += old_r;
        }
        self.fade_tail = tail;
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
            EventKind::NoteOn {
                channel,
                key,
                velocity,
            } => MidiEvent::NoteOn {
                channel,
                key,
                vel: velocity,
            },
            EventKind::NoteOff { channel, key } => MidiEvent::NoteOff { channel, key },
            EventKind::ControlChange {
                channel,
                controller,
                value,
            } => MidiEvent::ControlChange {
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
            let samples = &buffer.samples[buffer.pos..buffer.pos + count];
            for ((l, r), s) in left[offset..offset + count]
                .iter_mut()
                .zip(right[offset..offset + count].iter_mut())
                .zip(samples)
            {
                *l += *s;
                *r += *s;
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
            let Some(&(fl, fr)) = self.fade_tail.get(self.fade_pos) else {
                break;
            };
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
    fn a_second_reset_in_the_same_drain_keeps_the_first_fade_tail() {
        let (mut engine, handle) = MidiEngine::new(None);
        handle
            .send(EngineCommand::Play(PlayCommand {
                buffers: vec![(0, vec![0.5; SAMPLE_RATE as usize])],
                ..Default::default()
            }))
            .unwrap();
        // Four chunks: the buffer (scheduled at LEAD_FRAMES) is sounding.
        let (sounding, _) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        assert!(sounding[4 * CHUNK_FRAMES - 1] > 0.4, "buffer is audible");

        // Two resetting commands land in the same drain. The second one sees an
        // already-silent engine, so it must not throw away the first one's tail.
        handle.send(EngineCommand::Stop).unwrap();
        handle
            .send(EngineCommand::Play(PlayCommand {
                mode: PlayMode::Replace,
                ..Default::default()
            }))
            .unwrap();

        let (mut l, mut r) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
        engine.render(&mut l, &mut r);
        assert!(l[0] > 0.4, "fade starts from the audio that was playing");
        assert!(
            l[FADE_FRAMES / 2] > 0.0 && l[FADE_FRAMES / 2] < l[0],
            "fade decays instead of hard-cutting"
        );
        assert!(
            l[FADE_FRAMES..].iter().all(|s| *s == 0.0),
            "silence once the fade is over"
        );
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
