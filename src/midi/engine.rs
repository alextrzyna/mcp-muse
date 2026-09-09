//! One long-lived MIDI/synthesis engine for the process (GitHub issue #95).
//!
//! `MidiEngine` owns the single OxiSynth instance, a queue of MIDI events keyed
//! to its sample clock, and the pre-rendered R2D2/synthesis buffers scheduled
//! on a shared mono bus. Tool calls send `EngineCommand`s over a channel; the
//! engine drains them at chunk boundaries and applies events at their exact
//! sample.

use crate::expressive::{DEFAULT_TEMPO, EffectsChain};
use crate::midi::EffectConfig;
use oxisynth::{MidiEvent, SoundFont, Synth};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::env;
use std::num::NonZero;
use std::path::{Path, PathBuf};
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
/// Voice cap. Idle voices cost nothing in the render loop, so this is left
/// at OxiSynth's own default rather than trimmed; a lower cap caused
/// audible note stealing with layered, sustained FluidR3 material, since
/// stereo-layered presets use two or more voices per note.
const POLYPHONY: u16 = 256;
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
#[derive(Debug, Clone)]
pub struct PlayCommand {
    /// MIDI events in time order (stable: setup before note-on at equal offsets).
    pub events: Vec<(u64, EventKind)>,
    /// Pre-rendered stereo buffers (R2D2, synthesis) already at bus level.
    pub buffers: Vec<(u64, Vec<[f32; 2]>)>,
    /// MIDI bus effects chain for this call, if any note specified one.
    pub midi_effects: Option<Vec<EffectConfig>>,
    pub mode: PlayMode,
    /// The sequence's tempo, reaching the MIDI bus chain's tempo-synced effects.
    pub tempo: u32,
}

impl Default for PlayCommand {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            buffers: Vec::new(),
            midi_effects: None,
            mode: PlayMode::default(),
            tempo: DEFAULT_TEMPO,
        }
    }
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

pub fn find_soundfont() -> Result<PathBuf, String> {
    // First check if there's a custom soundfont path configured
    if let Ok(config) = crate::setup::config::SetupConfig::load()
        && let Some(custom_path) = config.soundfont_path
    {
        let path = PathBuf::from(custom_path);
        if path.exists() {
            tracing::info!("Using custom SoundFont from config: {:?}", path);
            return Ok(path);
        } else {
            tracing::warn!("Configured custom SoundFont not found: {:?}", path);
        }
    }

    // Try to find the SoundFont in various locations
    let exe_path = env::current_exe().map_err(|e| format!("Cannot find executable: {}", e))?;
    let exe_dir = exe_path
        .parent()
        .ok_or("Cannot find executable directory")?;

    let possible_paths = vec![
        exe_dir.join("../assets/FluidR3_GM.sf2"), // Development
        exe_dir.join("assets/FluidR3_GM.sf2"),    // Installed
        PathBuf::from("assets/FluidR3_GM.sf2"),   // Current directory
        PathBuf::from("FluidR3_GM.sf2"),          // Current directory
        // Also check target/debug/assets for development
        PathBuf::from("target/debug/assets/FluidR3_GM.sf2"),
        PathBuf::from("target/release/assets/FluidR3_GM.sf2"),
        // Fallback to old soundfont if it exists
        exe_dir.join("../assets/TimGM6mb.sf2"), // Development (old)
        exe_dir.join("assets/TimGM6mb.sf2"),    // Installed (old)
        PathBuf::from("assets/TimGM6mb.sf2"),   // Current directory (old)
        PathBuf::from("TimGM6mb.sf2"),          // Current directory (old)
        PathBuf::from("target/debug/assets/TimGM6mb.sf2"), // Development (old)
        PathBuf::from("target/release/assets/TimGM6mb.sf2"), // Release (old)
    ];

    for path in possible_paths {
        if path.exists() {
            tracing::info!("Found SoundFont at: {:?}", path);
            return Ok(path);
        }
    }

    Err("SoundFont not found. Please run 'mcp-muse setup' to download it.".to_string())
}

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
    samples: Vec<[f32; 2]>,
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
                self.set_bus_effects(play.tempo, play.midi_effects.as_deref().unwrap_or(&[]));
            }
            PlayMode::Layer => {
                if let Some(effects) = &play.midi_effects {
                    self.set_bus_effects(play.tempo, effects);
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
        tracing::debug!(
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

    fn set_bus_effects(&mut self, tempo: u32, effects: &[EffectConfig]) {
        self.bus_left = EffectsChain::with_tempo(SAMPLE_RATE as f32, tempo, effects);
        self.bus_right = EffectsChain::with_tempo(SAMPLE_RATE as f32, tempo, effects);
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
        self.set_bus_effects(DEFAULT_TEMPO, &[]);
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
                *l += s[0];
                *r += s[1];
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

    fn channels(&self) -> rodio::ChannelCount {
        rodio::nz!(2)
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        const { NonZero::new(SAMPLE_RATE).unwrap() }
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::expressive::test_util::rms;
    use crate::midi::EffectConfig;

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
                buffers: vec![(0, vec![[0.25, 0.25]; 10])],
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
                buffers: vec![(0, vec![[0.5, 0.5]; SAMPLE_RATE as usize])],
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
    fn a_default_play_command_carries_the_default_tempo() {
        assert_eq!(PlayCommand::default().tempo, DEFAULT_TEMPO);
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

    fn reverb() -> EffectConfig {
        serde_json::from_value(serde_json::json!({
            "type": "reverb", "room_size": 0.6, "intensity": 0.4
        }))
        .unwrap()
    }

    fn play(buffers: Vec<(u64, Vec<[f32; 2]>)>, mode: PlayMode) -> EngineCommand {
        EngineCommand::Play(PlayCommand {
            buffers,
            mode,
            ..Default::default()
        })
    }

    #[test]
    fn a_buffer_scheduled_at_t_is_mixed_from_t_on_both_sides() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(10, vec![[0.5, 0.5]; 1000])], PlayMode::Replace));
        let (left, right) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        let start = LEAD_FRAMES as usize + 10;
        assert_eq!(left[start - 1], 0.0);
        assert_eq!(left[start], 0.5);
        assert_eq!(left[start + 999], 0.5);
        assert_eq!(left[start + 1000], 0.0);
        assert_eq!(right[start], 0.5, "both sides carry the buffer");
        assert!(engine.buffers.is_empty(), "exhausted buffers are dropped");
    }

    #[test]
    fn stereo_buffers_keep_their_channels() {
        let (mut engine, _h) = MidiEngine::new(None);
        engine.apply(play(
            vec![(0, vec![[0.25, -0.5]; CHUNK_FRAMES])],
            PlayMode::Replace,
        ));
        let (mut l, mut r) = (vec![0.0; CHUNK_FRAMES], vec![0.0; CHUNK_FRAMES]);
        // Skip the lead-in.
        let mut rendered = 0;
        while rendered < LEAD_FRAMES as usize {
            engine.render_unclipped(&mut l, &mut r);
            rendered += CHUNK_FRAMES;
        }
        engine.render_unclipped(&mut l, &mut r);
        assert!(l.iter().any(|&x| (x - 0.25).abs() < 1e-6));
        assert!(r.iter().any(|&x| (x + 0.5).abs() < 1e-6));
    }

    #[test]
    fn layer_keeps_the_current_buffer_and_replace_cuts_it() {
        let a = vec![[0.25f32, 0.25]; 3 * SAMPLE_RATE as usize];
        let b = vec![[0.5f32, 0.5]; 3 * SAMPLE_RATE as usize];

        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, a.clone())], PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES); // A is now sounding
        engine.apply(play(vec![(0, b.clone())], PlayMode::Layer));
        let (left, _) = render_all(&mut engine, 8 * CHUNK_FRAMES);
        assert_eq!(left[0], 0.25, "A still alone before B starts");
        // B starts at LEAD_FRAMES into this render; well past that both are active.
        assert!(
            left[4 * CHUNK_FRAMES..].iter().all(|&s| s == 0.75),
            "layer must mix A and B"
        );

        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, a)], PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES);
        engine.apply(play(vec![(0, b)], PlayMode::Replace));
        let (left, _) = render_all(&mut engine, 8 * CHUNK_FRAMES);
        assert!(
            left[FADE_FRAMES..LEAD_FRAMES as usize]
                .iter()
                .all(|&s| s == 0.0),
            "A cut and faded, B not started yet"
        );
        assert!(
            left[4 * CHUNK_FRAMES..].iter().all(|&s| s == 0.5),
            "replace must leave only B"
        );
    }

    #[test]
    fn replace_and_stop_fade_within_one_chunk() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, vec![[0.5, 0.5]; 44_100])], PlayMode::Replace));
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
        assert!(
            engine.bus_has_effects(),
            "replace installs the call's chain"
        );
        engine.apply(with(None, PlayMode::Layer));
        assert!(engine.bus_has_effects(), "layer without a chain keeps it");
        engine.apply(with(Some(Vec::new()), PlayMode::Layer));
        assert!(
            !engine.bus_has_effects(),
            "layer with an explicit chain replaces it"
        );
        engine.apply(with(Some(vec![reverb()]), PlayMode::Layer));
        engine.apply(with(None, PlayMode::Replace));
        assert!(
            !engine.bus_has_effects(),
            "replace without a chain clears it"
        );
        engine.apply(EngineCommand::Stop);
        assert!(!engine.bus_has_effects());
    }

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
            EventKind::ProgramChange {
                channel: 0,
                program: 73,
            },
        )];
        if let Some(pan) = pan {
            events.push((
                offset,
                EventKind::ControlChange {
                    channel: 0,
                    controller: 10,
                    value: pan,
                },
            ));
        }
        events.push((
            offset,
            EventKind::NoteOn {
                channel: 0,
                key: 76,
                velocity: 100,
            },
        ));
        events.push((
            offset + seconds_to_frames(Duration::from_secs_f64(seconds)),
            EventKind::NoteOff {
                channel: 0,
                key: 76,
            },
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
        let Some((mut engine, _handle)) = engine_with_soundfont() else {
            return;
        };
        engine.apply(play_events(flute(100, 0.5, None), PlayMode::Replace));
        // 5 chunks: the brief's 4 (4096 frames) is short of start+2000 (4148).
        let (left, _) = render_all(&mut engine, 5 * CHUNK_FRAMES);
        let start = LEAD_FRAMES as usize + 100;
        // OxiSynth's built-in reverb idles at a ~1e-7 anti-denormal offset even
        // with no voice active; a real note-on is orders of magnitude louder.
        assert!(
            left[..start].iter().all(|s| s.abs() < 1e-6),
            "sound before the scheduled frame"
        );
        assert!(
            rms(&left[start..start + 2000]) > 0.001,
            "no sound after the scheduled frame"
        );
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
        let Some((mut engine, _handle)) = engine_with_soundfont() else {
            return;
        };
        engine.apply(play_events(flute(0, 0.5, None), PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        assert_eq!(engine.synth().unwrap().program(0).unwrap().2, 73);

        engine.apply(EngineCommand::Stop);
        assert_eq!(
            engine.synth().unwrap().program(0).unwrap().2,
            0,
            "SystemReset restores program 0"
        );

        engine.apply(play_events(
            vec![
                (
                    0,
                    EventKind::ProgramChange {
                        channel: 9,
                        program: 0,
                    },
                ),
                (
                    0,
                    EventKind::NoteOn {
                        channel: 9,
                        key: 36,
                        velocity: 110,
                    },
                ),
                (
                    4410,
                    EventKind::NoteOff {
                        channel: 9,
                        key: 36,
                    },
                ),
            ],
            PlayMode::Layer,
        ));
        let (left, _) = render_all(&mut engine, 4 * CHUNK_FRAMES);
        assert!(
            rms(&left[LEAD_FRAMES as usize..]) > 0.001,
            "kick did not sound"
        );
        let synth = engine.synth().unwrap();
        let kit = synth.channel_preset(9).unwrap();
        let piano = synth.channel_preset(0).unwrap();
        assert_ne!(
            kit.name(),
            piano.name(),
            "channel 9 must draw from the percussion bank"
        );
    }

    #[test]
    fn engine_source_is_stereo_interleaved_and_never_ends() {
        use rodio::Source;
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play(vec![(0, vec![[0.5, 0.5]; 100])], PlayMode::Replace));
        let mut source = EngineSource::new(engine);
        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), SAMPLE_RATE);
        assert_eq!(source.total_duration(), None);
        let samples: Vec<f32> = source
            .by_ref()
            .take(2 * (LEAD_FRAMES as usize + 50))
            .collect();
        assert_eq!(
            samples[2 * LEAD_FRAMES as usize],
            0.5,
            "left of the first buffer frame"
        );
        assert_eq!(
            samples[2 * LEAD_FRAMES as usize + 1],
            0.5,
            "right of the first buffer frame"
        );
        assert!(
            source.by_ref().take(10 * CHUNK_FRAMES).count() == 10 * CHUNK_FRAMES,
            "must not end"
        );
    }
}
