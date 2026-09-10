//! One long-lived MIDI/synthesis engine for the process (GitHub issue #95).
//!
//! `MidiEngine` owns the single OxiSynth instance, a queue of MIDI events keyed
//! to its sample clock, and the pre-rendered R2D2/synthesis buffers scheduled
//! on a shared mono bus. Tool calls send `EngineCommand`s over a channel; the
//! engine drains them at chunk boundaries and applies events at their exact
//! sample. Each playback's MIDI channels are remapped onto physical channels
//! no other active playback owns, so layered calls keep their own programs
//! and controllers (GitHub issue #98).

use crate::expressive::{DEFAULT_TEMPO, EffectsChain};
use crate::midi::EffectConfig;
use crate::midi::external::{ExternalMessage, ExternalSender, PortId, encode};
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
/// OxiSynth's percussion channel. It is never remapped: the kit is fixed, so
/// two playbacks sharing it is harmless.
const DRUM_CHANNEL: u8 = 9;
const MIDI_CHANNELS: usize = 16;

/// Which playback holds a physical MIDI channel, and the engine frame of its
/// last scheduled event on it. Once that frame has passed the channel may be
/// handed to another playback (GitHub issue #98).
#[derive(Debug, Clone, Copy)]
struct ChannelOwner {
    playback: u64,
    last_event: u64,
}

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

impl EventKind {
    fn channel(&self) -> u8 {
        match self {
            EventKind::NoteOn { channel, .. }
            | EventKind::NoteOff { channel, .. }
            | EventKind::ControlChange { channel, .. }
            | EventKind::ProgramChange { channel, .. } => *channel,
        }
    }

    fn set_channel(&mut self, physical: u8) {
        match self {
            EventKind::NoteOn { channel, .. }
            | EventKind::NoteOff { channel, .. }
            | EventKind::ControlChange { channel, .. }
            | EventKind::ProgramChange { channel, .. } => *channel = physical,
        }
    }
}

/// Everything one play call schedules. Offsets are frames after the
/// command's start; the engine picks the start as `clock + LEAD_FRAMES`.
#[derive(Debug, Clone)]
pub struct PlayCommand {
    /// MIDI events in time order (stable: setup before note-on at equal offsets).
    pub events: Vec<(u64, EventKind)>,
    /// MIDI events for ports on this machine, keyed by the opened port.
    /// Their channels reach the port as given (a DAW routes by channel).
    pub external: Vec<(u64, PortId, EventKind)>,
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
            external: Vec::new(),
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

/// Where a scheduled event goes when its frame comes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The internal OxiSynth.
    Synth,
    /// An opened external port, through the sender thread.
    External(PortId),
}

struct ScheduledEvent {
    at: u64,
    seq: u64,
    kind: EventKind,
    target: Target,
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
    /// Physical channel ownership; `None` is pristine since the last reset.
    channels: [Option<ChannelOwner>; MIDI_CHANNELS],
    /// Id handed to the next scheduled playback.
    next_playback: u64,
    /// Handle to the external MIDI sender thread, when the process has one.
    external: Option<ExternalSender>,
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
            channels: [None; MIDI_CHANNELS],
            next_playback: 0,
            external: None,
        };
        (engine, EngineHandle { clock, sender })
    }

    /// Forward external events (and all-notes-off on every reset) to the
    /// sender thread. Without this, external events are dropped.
    pub fn set_external(&mut self, sender: ExternalSender) {
        self.external = Some(sender);
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
        let playback = self.next_playback;
        self.next_playback += 1;
        let mut events = play.events;
        self.allocate_channels(playback, start, &mut events);
        for (offset, kind) in events {
            self.push_event(start + offset, kind, Target::Synth);
        }
        for (offset, port, kind) in play.external {
            self.push_event(start + offset, kind, Target::External(port));
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

    fn push_event(&mut self, at: u64, kind: EventKind, target: Target) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push(ScheduledEvent {
            at,
            seq,
            kind,
            target,
        });
    }

    fn set_bus_effects(&mut self, tempo: u32, effects: &[EffectConfig]) {
        self.bus_left = EffectsChain::with_tempo(SAMPLE_RATE as f32, tempo, effects);
        self.bus_right = EffectsChain::with_tempo(SAMPLE_RATE as f32, tempo, effects);
    }

    /// Move a playback's logical channels onto physical channels no active
    /// playback owns, so layered calls keep their own programs and
    /// controllers. Events are rewritten in place; a channel taken over from
    /// a finished playback gets its controllers reset first (queued at
    /// `start`, ahead of the call's own events). Channel 9 stays channel 9.
    fn allocate_channels(&mut self, playback: u64, start: u64, events: &mut [(u64, EventKind)]) {
        let mut last_offset: [Option<u64>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
        for (offset, kind) in events.iter() {
            let slot = &mut last_offset[kind.channel() as usize];
            *slot = Some(slot.map_or(*offset, |l| l.max(*offset)));
        }
        let mut map: [u8; MIDI_CHANNELS] = std::array::from_fn(|c| c as u8);
        for logical in 0..MIDI_CHANNELS as u8 {
            let Some(offset) = last_offset[logical as usize] else {
                continue;
            };
            if logical == DRUM_CHANNEL {
                continue;
            }
            let physical = self.claim_channel(logical, start);
            map[logical as usize] = physical;
            let last_event = start + offset;
            let slot = &mut self.channels[physical as usize];
            match *slot {
                // Pristine since the last reset: nothing to undo.
                None => {
                    *slot = Some(ChannelOwner {
                        playback,
                        last_event,
                    })
                }
                // A finished playback left its controllers behind.
                Some(previous) if previous.last_event < start => {
                    self.reset_controllers(physical, start);
                    self.channels[physical as usize] = Some(ChannelOwner {
                        playback,
                        last_event,
                    });
                }
                // Sharing fallback: the channel frees when the later of the two ends.
                Some(previous) => {
                    tracing::info!(
                        "All melodic MIDI channels are busy; playback {} shares channel {} \
                         with playback {} (its program and controllers apply to both)",
                        playback,
                        physical,
                        previous.playback
                    );
                    self.channels[physical as usize] = Some(ChannelOwner {
                        playback,
                        last_event: last_event.max(previous.last_event),
                    });
                }
            }
        }
        for (logical, physical) in map.iter().enumerate() {
            if *physical != logical as u8 {
                tracing::debug!(
                    "Playback {}: MIDI channel {} plays on physical channel {}",
                    playback,
                    logical,
                    physical
                );
            }
        }
        for (_, kind) in events.iter_mut() {
            let logical = kind.channel() as usize;
            if map[logical] != logical as u8 {
                kind.set_channel(map[logical]);
            }
        }
    }

    /// The physical channel for `logical`: itself when free, else the lowest
    /// free melodic channel, else (all fifteen busy) the one that frees soonest.
    fn claim_channel(&self, logical: u8, start: u64) -> u8 {
        let is_free = |c: u8| self.channels[c as usize].is_none_or(|o| o.last_event < start);
        if is_free(logical) {
            return logical;
        }
        let melodic = || (0..MIDI_CHANNELS as u8).filter(|c| *c != DRUM_CHANNEL);
        if let Some(free) = melodic().find(|c| is_free(*c)) {
            return free;
        }
        melodic()
            .min_by_key(|c| self.channels[*c as usize].map_or(0, |o| o.last_event))
            .expect("fifteen melodic channels")
    }

    /// Return a reassigned channel to the state a SystemReset leaves: every
    /// controller and the pitch bend cleared, volume 100, pan centred, no
    /// reverb or chorus send. Sounding voices keep their preset.
    fn reset_controllers(&mut self, channel: u8, at: u64) {
        const RESET_ALL_CONTROLLERS: u8 = 121;
        for (controller, value) in [
            (RESET_ALL_CONTROLLERS, 0),
            (7, 100),
            (10, 64),
            (91, 0),
            (93, 0),
        ] {
            self.push_event(
                at,
                EventKind::ControlChange {
                    channel,
                    controller,
                    value,
                },
                Target::Synth,
            );
        }
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
        self.channels = [None; MIDI_CHANNELS];
        self.set_bus_effects(DEFAULT_TEMPO, &[]);
        if let Some(external) = &self.external {
            external.send(ExternalMessage::AllNotesOff);
        }
    }

    fn apply_event(&mut self, kind: EventKind, target: Target) {
        if let Target::External(port) = target {
            if let Some(external) = &self.external {
                let (bytes, len) = encode(&kind);
                external.send(ExternalMessage::Send { port, bytes, len });
            }
            return;
        }
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
                self.apply_event(event.kind, event.target);
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

    /// Program change, note-on and note-off for one note on `channel`.
    fn voice(channel: u8, program: u8, key: u8, at: u64, frames: u64) -> Vec<(u64, EventKind)> {
        vec![
            (at, EventKind::ProgramChange { channel, program }),
            (
                at,
                EventKind::NoteOn {
                    channel,
                    key,
                    velocity: 100,
                },
            ),
            (at + frames, EventKind::NoteOff { channel, key }),
        ]
    }

    /// Note-ons still queued, as (frame, channel, key).
    fn queued_note_ons(engine: &MidiEngine) -> Vec<(u64, u8, u8)> {
        let mut ons: Vec<(u64, u8, u8)> = engine
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { channel, key, .. } => Some((e.at, channel, key)),
                _ => None,
            })
            .collect();
        ons.sort();
        ons
    }

    fn queued_program_changes(engine: &MidiEngine) -> Vec<(u8, u8)> {
        engine
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::ProgramChange { channel, program } => Some((channel, program)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_layered_call_on_a_busy_logical_channel_moves_to_a_free_physical_channel() {
        let (mut engine, _handle) = MidiEngine::new(None);
        // A: two notes on channel 2, the second a second in.
        let mut a = voice(2, 80, 60, 0, 44_100);
        a.extend(voice(2, 80, 62, 44_100, 44_100));
        engine.apply(play_events(a, PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES); // A's first note has started
        // B lands on logical channel 2 while A still owns it.
        engine.apply(play_events(voice(2, 60, 64, 0, 22_050), PlayMode::Layer));

        let ons = queued_note_ons(&engine);
        let a_later = ons
            .iter()
            .find(|(_, _, key)| *key == 62)
            .expect("A's second note");
        let b = ons.iter().find(|(_, _, key)| *key == 64).expect("B's note");
        assert_eq!(a_later.1, 2, "A keeps its channel");
        assert_ne!(b.1, 2, "B must not share A's channel");
        assert_ne!(b.1, 9, "B must not land on the drum channel");
        assert!(
            queued_program_changes(&engine).contains(&(b.1, 60)),
            "B's program change follows it to the new channel"
        );
    }

    #[test]
    fn a_replace_call_keeps_its_logical_channels() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play_events(voice(2, 80, 60, 0, 44_100), PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        engine.apply(play_events(voice(2, 60, 64, 0, 22_050), PlayMode::Replace));
        assert_eq!(
            queued_note_ons(&engine)[0].1,
            2,
            "replace starts from a clear map"
        );
    }

    #[test]
    fn channel_9_is_never_remapped() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play_events(voice(9, 0, 36, 0, 44_100), PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        engine.apply(play_events(voice(9, 0, 38, 0, 44_100), PlayMode::Layer));
        assert_eq!(queued_note_ons(&engine)[0].1, 9);
    }

    #[test]
    fn a_layered_call_after_the_owner_finishes_reuses_the_channel() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(play_events(voice(2, 80, 60, 0, 1000), PlayMode::Replace));
        render_all(&mut engine, 4 * CHUNK_FRAMES); // past A's note-off
        engine.apply(play_events(voice(2, 60, 64, 0, 1000), PlayMode::Layer));
        assert_eq!(
            queued_note_ons(&engine)[0].1,
            2,
            "A's channel is free again"
        );
    }

    #[test]
    fn sharing_is_the_fallback_when_every_melodic_channel_is_busy() {
        let (mut engine, _handle) = MidiEngine::new(None);
        let mut all: Vec<(u64, EventKind)> = Vec::new();
        for channel in (0..16u8).filter(|c| *c != 9) {
            all.extend(voice(channel, 1, 60, 0, 88_200));
        }
        engine.apply(play_events(all, PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        engine.apply(play_events(voice(0, 60, 64, 0, 1000), PlayMode::Layer));
        let ons = queued_note_ons(&engine);
        assert_eq!(ons.len(), 1, "the call is still scheduled");
        assert!(ons[0].1 < 16 && ons[0].1 != 9, "on a real melodic channel");
    }

    #[test]
    fn layered_programs_survive_on_the_synth() {
        let Some((mut engine, _handle)) = engine_with_soundfont() else {
            return;
        };
        engine.apply(play_events(flute(0, 2.0, None), PlayMode::Replace));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        engine.apply(play_events(voice(0, 56, 60, 0, 4410), PlayMode::Layer));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        let synth = engine.synth().unwrap();
        assert_eq!(synth.program(0).unwrap().2, 73, "the flute keeps channel 0");
        assert!(
            (0..16u8)
                .filter(|c| *c != 9)
                .any(|c| synth.program(c).unwrap().2 == 56),
            "the trumpet plays on its own channel"
        );
    }

    #[test]
    fn a_reassigned_channel_starts_from_default_controllers() {
        let Some((mut engine, _handle)) = engine_with_soundfont() else {
            return;
        };
        // A: flute panned hard left, sustain on, over in 0.1 s.
        let mut a = flute(0, 0.1, Some(0));
        a.insert(
            1,
            (
                0,
                EventKind::ControlChange {
                    channel: 0,
                    controller: 64,
                    value: 127,
                },
            ),
        );
        engine.apply(play_events(a, PlayMode::Replace));
        render_all(&mut engine, 8 * CHUNK_FRAMES);
        assert_eq!(
            engine.synth().unwrap().cc(0, 10).unwrap(),
            0,
            "A panned left"
        );
        // B takes channel 0 over without saying anything about pan or sustain.
        engine.apply(play_events(voice(0, 56, 60, 0, 4410), PlayMode::Layer));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        let synth = engine.synth().unwrap();
        assert_eq!(synth.program(0).unwrap().2, 56);
        assert_eq!(synth.cc(0, 10).unwrap(), 64, "pan back to centre");
        assert_eq!(synth.cc(0, 64).unwrap(), 0, "sustain released");
        assert_eq!(synth.cc(0, 7).unwrap(), 100, "volume back to default");
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

#[cfg(test)]
mod external_tests {
    //! External events leave through the sender at their exact frame and
    //! are never remapped; every reset silences the ports.
    use super::tests::render_all;
    use super::*;
    use std::sync::mpsc::Receiver;

    fn engine_with_sender() -> (MidiEngine, Receiver<ExternalMessage>) {
        let (mut engine, _handle) = MidiEngine::new(None);
        let (sender, rx) = ExternalSender::for_test();
        engine.set_external(sender);
        (engine, rx)
    }

    fn note(port: u16, channel: u8, at: u64) -> Vec<(u64, PortId, EventKind)> {
        vec![
            (
                at,
                PortId(port),
                EventKind::NoteOn {
                    channel,
                    key: 60,
                    velocity: 100,
                },
            ),
            (
                at + 100,
                PortId(port),
                EventKind::NoteOff { channel, key: 60 },
            ),
        ]
    }

    /// Every `Send` received so far, as (port, bytes).
    fn sent(rx: &Receiver<ExternalMessage>) -> Vec<(u16, Vec<u8>)> {
        rx.try_iter()
            .filter_map(|m| match m {
                ExternalMessage::Send { port, bytes, len } => Some((port.0, bytes[..len].to_vec())),
                _ => None,
            })
            .collect()
    }

    fn all_notes_off_count(rx: &Receiver<ExternalMessage>) -> usize {
        rx.try_iter()
            .filter(|m| matches!(m, ExternalMessage::AllNotesOff))
            .count()
    }

    #[test]
    fn an_external_event_is_sent_when_its_frame_is_rendered_not_before() {
        let (mut engine, rx) = engine_with_sender();
        // Layer: no reset, so the channel only ever carries the note's bytes.
        engine.apply(EngineCommand::Play(PlayCommand {
            external: note(3, 2, 3000),
            mode: PlayMode::Layer,
            ..Default::default()
        }));
        // LEAD_FRAMES + 3000 = 5048: chunks 0..4 (frames 0-5119) cover the
        // note-on but not the note-off at 5148.
        render_all(&mut engine, 4 * CHUNK_FRAMES);
        assert!(sent(&rx).is_empty(), "nothing before the scheduled frame");
        render_all(&mut engine, CHUNK_FRAMES);
        assert_eq!(sent(&rx), vec![(3, vec![0x92, 60, 100])]);
        render_all(&mut engine, CHUNK_FRAMES);
        assert_eq!(sent(&rx), vec![(3, vec![0x82, 60, 0])]);
    }

    #[test]
    fn replace_and_stop_send_all_notes_off_and_clear_pending_external_events() {
        let (mut engine, rx) = engine_with_sender();
        engine.apply(EngineCommand::Play(PlayCommand {
            external: note(0, 0, 44_100),
            mode: PlayMode::Replace,
            ..Default::default()
        }));
        assert_eq!(all_notes_off_count(&rx), 1);
        engine.apply(EngineCommand::Stop);
        assert_eq!(all_notes_off_count(&rx), 1);
        render_all(&mut engine, 50 * CHUNK_FRAMES);
        assert!(sent(&rx).is_empty(), "the stopped note never went out");
    }

    #[test]
    fn a_layered_external_note_keeps_its_channel_while_a_synth_note_is_remapped() {
        let (mut engine, rx) = engine_with_sender();
        // A: a synth note on channel 2 that is still sounding.
        engine.apply(EngineCommand::Play(PlayCommand {
            events: vec![
                (
                    0,
                    EventKind::NoteOn {
                        channel: 2,
                        key: 60,
                        velocity: 100,
                    },
                ),
                (
                    88_200,
                    EventKind::NoteOff {
                        channel: 2,
                        key: 60,
                    },
                ),
            ],
            mode: PlayMode::Replace,
            ..Default::default()
        }));
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        // B: a synth note and an external note, both on logical channel 2.
        engine.apply(EngineCommand::Play(PlayCommand {
            events: vec![(
                0,
                EventKind::NoteOn {
                    channel: 2,
                    key: 64,
                    velocity: 100,
                },
            )],
            external: note(0, 2, 0),
            mode: PlayMode::Layer,
            ..Default::default()
        }));
        let synth_channel = engine
            .events
            .iter()
            .find_map(|e| match (e.target, &e.kind) {
                (
                    Target::Synth,
                    EventKind::NoteOn {
                        channel, key: 64, ..
                    },
                ) => Some(*channel),
                _ => None,
            })
            .expect("B's synth note");
        assert_ne!(synth_channel, 2, "the synth note moves off A's channel");
        render_all(&mut engine, 3 * CHUNK_FRAMES);
        let first = sent(&rx).into_iter().next().expect("external note-on");
        assert_eq!(
            first,
            (0, vec![0x92, 60, 100]),
            "external channel untouched"
        );
    }

    #[test]
    fn without_a_sender_external_events_are_dropped_and_rendering_continues() {
        let (mut engine, _handle) = MidiEngine::new(None);
        engine.apply(EngineCommand::Play(PlayCommand {
            external: note(0, 0, 0),
            mode: PlayMode::Replace,
            ..Default::default()
        }));
        render_all(&mut engine, 4 * CHUNK_FRAMES);
        assert!(engine.events.is_empty());
    }
}
