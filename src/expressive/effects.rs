//! Stateful audio effects.
//!
//! Every effect here keeps its own delay lines / filter memory across calls, so
//! it can be driven one sample at a time from the real-time mixer. Build an
//! [`EffectsChain`] once per channel from the user's [`EffectConfig`] list and
//! call [`EffectsChain::process`] for each sample.

use crate::midi::{EffectConfig, EffectType, FilterType, PitchMode};
use rand::RngExt;

/// Tempo assumed by [`EffectsChain::new`] when the caller has no sequence tempo.
pub const DEFAULT_TEMPO: u32 = 120;

/// Filter response selected on an [`Svf`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SvfMode {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

impl From<&FilterType> for SvfMode {
    fn from(f: &FilterType) -> Self {
        match f {
            FilterType::LowPass => SvfMode::LowPass,
            FilterType::HighPass => SvfMode::HighPass,
            FilterType::BandPass => SvfMode::BandPass,
            FilterType::Notch => SvfMode::Notch,
            FilterType::Peak => SvfMode::Peak,
            FilterType::LowShelf => SvfMode::LowShelf,
            FilterType::HighShelf => SvfMode::HighShelf,
        }
    }
}

/// Topology-preserving state variable filter (Simper/Cytomic form).
/// Stable for any cutoff below Nyquist, unlike the Chamberlin form.
#[derive(Debug, Clone)]
pub struct Svf {
    sample_rate: f32,
    mode: SvfMode,
    q: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    /// `q` is the resonance quality factor; 0.707 is flat, 10+ is strongly resonant.
    pub fn new(sample_rate: f32, cutoff_hz: f32, q: f32, mode: SvfMode) -> Self {
        let mut svf = Self {
            sample_rate,
            mode,
            q: q.max(0.1),
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        };
        svf.set_cutoff(cutoff_hz);
        svf
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        let nyquist = self.sample_rate * 0.5;
        let cutoff = cutoff_hz.clamp(10.0, nyquist * 0.98);
        self.g = (std::f32::consts::PI * cutoff / self.sample_rate).tan();
        self.k = 1.0 / self.q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        let low = v2;
        let band = v1;
        let high = x - self.k * v1 - v2;
        // A fixed +6 dB shelf; enough character without a second gain parameter.
        const SHELF_GAIN: f32 = 2.0;
        match self.mode {
            SvfMode::LowPass => low,
            SvfMode::HighPass => high,
            SvfMode::BandPass => band,
            SvfMode::Notch => low + high,
            SvfMode::Peak => low - high,
            SvfMode::LowShelf => x + low * (SHELF_GAIN - 1.0),
            SvfMode::HighShelf => x + high * (SHELF_GAIN - 1.0),
        }
    }
}

/// Fixed-length circular delay line.
#[derive(Debug, Clone)]
struct DelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl DelayLine {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)],
            write: 0,
        }
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Sample written `self.len()` samples ago (the oldest sample).
    #[inline]
    fn read(&self) -> f32 {
        self.buffer[self.write]
    }

    /// Linearly interpolated read `delay` samples in the past (0 < delay < len).
    #[inline]
    fn read_fractional(&self, delay: f32) -> f32 {
        let len = self.buffer.len();
        let delay = delay.clamp(1.0, len as f32 - 1.0);
        let whole = delay.floor();
        let frac = delay - whole;
        // Integer wrap-around: floating-point `rem_euclid` can round to
        // exactly `len` for tiny negative inputs and index past the end.
        let i0 = (self.write + len - whole as usize) % len;
        let i1 = (i0 + len - 1) % len;
        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buffer[self.write] = x;
        self.write = (self.write + 1) % self.buffer.len();
    }
}

/// Schroeder reverb: parallel damped comb filters into series allpass diffusers.
#[derive(Debug, Clone)]
pub struct Reverb {
    pre_delay: DelayLine,
    combs: [DelayLine; 4],
    comb_lp: [f32; 4],
    comb_feedback: f32,
    damping: f32,
    allpass: [DelayLine; 2],
    wet: f32,
    dry: f32,
}

impl Reverb {
    pub fn new(
        sample_rate: f32,
        room_size: f32,
        dampening: f32,
        wet_level: f32,
        pre_delay: f32,
        intensity: f32,
    ) -> Self {
        let room = room_size.clamp(0.0, 1.0);
        // Classic Freeverb-style comb lengths scaled by room size.
        let base = [1116.0f32, 1188.0, 1277.0, 1356.0];
        let scale = sample_rate / 44100.0 * (0.6 + room * 0.8);
        let combs = base.map(|n| DelayLine::new((n * scale) as usize));
        let allpass =
            [556.0f32, 441.0].map(|n| DelayLine::new((n * sample_rate / 44100.0) as usize));
        let wet = (wet_level * intensity).clamp(0.0, 1.0);
        Self {
            pre_delay: DelayLine::new((pre_delay.max(0.0) * sample_rate) as usize),
            combs,
            comb_lp: [0.0; 4],
            comb_feedback: 0.7 + room * 0.28,
            damping: dampening.clamp(0.0, 1.0) * 0.4,
            allpass,
            wet,
            dry: 1.0 - wet * 0.5,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let input = if self.pre_delay.len() > 1 {
            let d = self.pre_delay.read();
            self.pre_delay.write(x);
            d
        } else {
            x
        };

        let mut sum = 0.0;
        for i in 0..4 {
            let out = self.combs[i].read();
            // One-pole lowpass in the feedback path (high-frequency damping).
            self.comb_lp[i] = out * (1.0 - self.damping) + self.comb_lp[i] * self.damping;
            self.combs[i].write(input + self.comb_lp[i] * self.comb_feedback);
            sum += out;
        }
        let mut y = sum * 0.25;

        for ap in &mut self.allpass {
            let delayed = ap.read();
            let v = y + delayed * 0.5;
            ap.write(v);
            y = delayed - v * 0.5;
        }

        x * self.dry + y * self.wet
    }
}

/// Longest delay time Time Fracture will honour: 4 beats (the validated
/// maximum) at 30 BPM. Beyond this the requested time is clamped, so an
/// absurdly slow tempo cannot size an unbounded buffer.
const MAX_DELAY_SECONDS: f32 = 8.0;

/// Room above the delay time for a downward-transposed repeat to walk the read
/// position backwards before it restarts. Matches the reference implementation's
/// whole buffer, and outlasts any grain a realistic `random_rate` produces.
const DRIFT_HEADROOM_SECONDS: f32 = 4.0;

/// Time Fracture state: a random delay time gliding between two bounds,
/// and a pitch accumulator that drifts the read position so each repeat
/// plays back transposed.
#[derive(Debug, Clone)]
struct Fracture {
    min_samples: f32,
    max_samples: f32,
    /// Longest delay the line can actually serve; past it `read_fractional`
    /// would clamp and the read would stick, so a repeat restarts instead.
    max_read: f32,
    /// Whether the delay time wanders at all (`random_rate > 0`). The grain
    /// clock also runs for pitch shifting, so this cannot be inferred from
    /// `phase_inc`.
    random_motion: bool,
    /// Cycles per sample of the random-walk phase (0 = never moves).
    phase_inc: f32,
    phase: f32,
    last_random: f32,
    next_random: f32,
    intervals: Vec<f32>,
    mode: PitchMode,
    index: usize,
    going_up: bool,
    pitch_ratio: f32,
    pitch_accumulator: f32,
    /// xorshift32 state, seeded once from `rand::rng()`; keeps `Delay`
    /// `Clone + Debug` (a `ThreadRng` field would not) and allocation-free.
    rng: u32,
}

impl Fracture {
    /// Uniform in [0, 1). xorshift32; the state is never zero because the
    /// seed is or-ed with 1.
    #[inline]
    fn next_unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Delay in samples for the current random-walk phase, before pitch drift.
    fn current_delay(&self) -> f32 {
        let alpha = 0.5 - 0.5 * (std::f32::consts::PI * self.phase).cos();
        let r = self.last_random * (1.0 - alpha) + self.next_random * alpha;
        self.min_samples + (self.max_samples - self.min_samples) * r
    }

    /// One sample of motion; returns the effective read delay in samples.
    #[inline]
    fn advance(&mut self) -> f32 {
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.roll_endpoints();
            if !self.intervals.is_empty() {
                self.pitch_ratio = 2f32.powf(self.next_semitones() / 12.0);
                self.pitch_accumulator = 0.0;
            }
        }
        if !self.intervals.is_empty() {
            self.pitch_accumulator += self.pitch_ratio - 1.0;
        }
        let mut delay = self.current_delay() - self.pitch_accumulator;
        // A transposed repeat that has run out of buffer starts over.
        if !self.intervals.is_empty() && (delay < 1.0 || delay >= self.max_read) {
            self.roll_endpoints();
            self.pitch_accumulator = 0.0;
            self.phase = 0.0;
            self.pitch_ratio = 2f32.powf(self.next_semitones() / 12.0);
            delay = self.current_delay();
        }
        delay.max(1.0)
    }

    /// Take the next pair of random-walk endpoints. Without random motion the
    /// endpoints stay at zero, which pins the delay time to `min_samples`.
    #[inline]
    fn roll_endpoints(&mut self) {
        if self.random_motion && self.max_samples > self.min_samples {
            self.last_random = self.next_random;
            self.next_random = self.next_unit();
        }
    }

    /// The next repeat's transposition, following [`PitchMode`].
    fn next_semitones(&mut self) -> f32 {
        let n = self.intervals.len();
        if n == 0 {
            return 0.0;
        }
        match self.mode {
            PitchMode::Random => {
                let i = ((self.next_unit() * n as f32) as usize).min(n - 1);
                self.intervals[i]
            }
            PitchMode::Up => {
                let s = self.intervals[self.index];
                self.index = (self.index + 1) % n;
                s
            }
            PitchMode::Down => {
                let s = self.intervals[self.index];
                self.index = if self.index == 0 {
                    n - 1
                } else {
                    self.index - 1
                };
                s
            }
            PitchMode::UpDown => {
                let s = self.intervals[self.index];
                if self.going_up {
                    if self.index + 1 >= n {
                        self.index = n.saturating_sub(2);
                        self.going_up = false;
                    } else {
                        self.index += 1;
                    }
                } else if self.index == 0 {
                    self.index = 1.min(n - 1);
                    self.going_up = true;
                } else {
                    self.index -= 1;
                }
                s
            }
        }
    }
}

/// Feedback delay with one-pole damping in the loop; optionally a Time
/// Fracture delay, whose time wanders between two tempo-synced bounds and
/// whose repeats are transposed.
#[derive(Debug, Clone)]
pub struct Delay {
    line: DelayLine,
    feedback: f32,
    lp: f32,
    wet: f32,
    dry: f32,
    /// `None` for the static path, whose delay is the whole line.
    fracture: Option<Fracture>,
}

impl Delay {
    /// Static delay: identical to the pre-Time-Fracture behaviour.
    pub fn new(
        sample_rate: f32,
        delay_time: f32,
        feedback: f32,
        wet_level: f32,
        intensity: f32,
    ) -> Self {
        let wet = (wet_level * intensity).clamp(0.0, 1.0);
        Self {
            line: DelayLine::new((delay_time.max(0.001) * sample_rate) as usize),
            feedback: (feedback * intensity).clamp(0.0, 0.95),
            lp: 0.0,
            wet,
            dry: 1.0 - wet * 0.5,
            fracture: None,
        }
    }

    /// Time Fracture: the delay time glides randomly between `min_beats` and
    /// `max_beats` of `tempo` at `random_rate` Hz, and each new repeat is
    /// transposed by the next of `pitch_intervals` (empty = no transposition).
    #[allow(clippy::too_many_arguments)]
    pub fn time_fracture(
        sample_rate: f32,
        tempo: u32,
        min_beats: f32,
        max_beats: f32,
        random_rate: f32,
        pitch_intervals: &[f32],
        pitch_mode: PitchMode,
        feedback: f32,
        wet_level: f32,
        intensity: f32,
    ) -> Self {
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        let to_samples = |beats: f32| {
            (beats * seconds_per_beat * sample_rate).clamp(1.0, MAX_DELAY_SECONDS * sample_rate)
        };
        let min_samples = to_samples(min_beats);
        let max_samples = to_samples(max_beats).max(min_samples);
        let wet = (wet_level * intensity).clamp(0.0, 1.0);
        // The delay itself plus room for a downward repeat to drift backwards.
        let line =
            DelayLine::new((max_samples + DRIFT_HEADROOM_SECONDS * sample_rate) as usize + 1);
        let random_motion = random_rate > 0.0;
        // With no random motion the delay sits at `min`; with pitch shifting and
        // no random motion, a new repeat (and interval) starts every delay period.
        let phase_inc = if random_motion {
            random_rate / sample_rate
        } else if !pitch_intervals.is_empty() {
            1.0 / min_samples
        } else {
            0.0
        };
        let mut fracture = Fracture {
            min_samples,
            max_samples,
            // `read_fractional` clamps to `len - 1`, so a delay that long would
            // stick rather than track; restart the repeat instead.
            max_read: line.len() as f32 - 1.0,
            random_motion,
            phase_inc,
            phase: 0.0,
            last_random: 0.0,
            next_random: 0.0,
            intervals: pitch_intervals.iter().map(|s| s.round()).collect(),
            mode: pitch_mode,
            index: 0,
            going_up: true,
            // The first repeat plays untransposed; the first interval is taken
            // when the first grain wraps, so the sequence starts at its head.
            pitch_ratio: 1.0,
            pitch_accumulator: 0.0,
            rng: rand::rng().random::<u32>() | 1,
        };
        if random_motion {
            fracture.last_random = fracture.next_unit();
            fracture.next_random = fracture.next_unit();
        }
        Self {
            line,
            feedback: (feedback * intensity).clamp(0.0, 0.95),
            lp: 0.0,
            wet,
            dry: 1.0 - wet * 0.5,
            fracture: Some(fracture),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let delayed = match &mut self.fracture {
            None => self.line.read(),
            Some(f) => {
                let delay = f.advance();
                self.line.read_fractional(delay)
            }
        };
        self.lp += 0.5 * (delayed - self.lp);
        self.line.write(x + self.lp * self.feedback);
        (x * self.dry + self.lp * self.wet).clamp(-1.0, 1.0)
    }

    #[cfg(test)]
    pub(crate) fn current_delay_samples(&self) -> f32 {
        match &self.fracture {
            // The static path reads the oldest sample, one whole line back.
            None => self.line.len() as f32,
            Some(f) => f.current_delay(),
        }
    }

    #[cfg(test)]
    pub(crate) fn next_pitch_semitones(&mut self) -> f32 {
        self.fracture
            .as_mut()
            .map(|f| f.next_semitones())
            .unwrap_or(0.0)
    }
}

/// Three-voice chorus with LFO-modulated, interpolated delay taps.
#[derive(Debug, Clone)]
pub struct Chorus {
    line: DelayLine,
    sample_rate: f32,
    phase: f32,
    rate: f32,
    base_delays: [f32; 3],
    depth_samples: f32,
    feedback: f32,
    wet: f32,
    dry: f32,
}

impl Chorus {
    pub fn new(sample_rate: f32, rate: f32, depth: f32, feedback: f32, intensity: f32) -> Self {
        let ms = sample_rate / 1000.0;
        let depth_samples = depth.clamp(0.0, 1.0) * 8.0 * ms;
        let wet = intensity.clamp(0.0, 1.0) * 0.6;
        Self {
            line: DelayLine::new((40.0 * ms) as usize + 4),
            sample_rate,
            phase: 0.0,
            rate: rate.clamp(0.1, 10.0),
            base_delays: [15.0 * ms, 22.0 * ms, 27.0 * ms],
            depth_samples,
            feedback: feedback.clamp(0.0, 0.9),
            wet,
            dry: 1.0 - wet * 0.5,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        use std::f32::consts::TAU;
        let mut mix = 0.0;
        for (i, base) in self.base_delays.iter().enumerate() {
            let lfo = (self.phase + i as f32 * TAU / 3.0).sin();
            let delay = base + lfo * self.depth_samples * (1.0 - i as f32 * 0.2);
            mix += self.line.read_fractional(delay);
        }
        mix /= 3.0;
        self.line.write(x + mix * self.feedback * 0.3);
        self.phase = (self.phase + TAU * self.rate / self.sample_rate) % TAU;
        x * self.dry + mix * self.wet
    }
}

/// Feed-forward compressor with smoothed gain.
#[derive(Debug, Clone)]
pub struct Compressor {
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    envelope: f32,
    gain: f32,
    intensity: f32,
}

impl Compressor {
    pub fn new(
        sample_rate: f32,
        threshold_db: f32,
        ratio: f32,
        attack: f32,
        release: f32,
        intensity: f32,
    ) -> Self {
        Self {
            threshold: 10f32.powf(threshold_db / 20.0),
            ratio: ratio.max(1.0),
            attack: (-1.0 / (attack.max(0.0001) * sample_rate)).exp(),
            release: (-1.0 / (release.max(0.001) * sample_rate)).exp(),
            envelope: 0.0,
            gain: 1.0,
            intensity: intensity.clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let level = x.abs();
        let coeff = if level > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = level + (self.envelope - level) * coeff;

        let target = if self.envelope > self.threshold {
            let over_db = 20.0 * (self.envelope / self.threshold).log10();
            let reduction_db = over_db * (1.0 - 1.0 / self.ratio);
            10f32.powf(-reduction_db / 20.0)
        } else {
            1.0
        };
        let coeff = if target < self.gain {
            self.attack
        } else {
            self.release
        };
        self.gain = target + (self.gain - target) * coeff;

        let compressed = x * self.gain;
        x * (1.0 - self.intensity) + compressed * self.intensity
    }
}

/// tanh waveshaper with pre-emphasis and a post tone filter.
#[derive(Debug, Clone)]
pub struct Distortion {
    input_gain: f32,
    output_gain: f32,
    pre_lp: f32,
    post_lp: f32,
    post_coeff: f32,
    intensity: f32,
}

impl Distortion {
    pub fn new(drive: f32, tone: f32, output_level: f32, intensity: f32) -> Self {
        Self {
            input_gain: (1.0 + drive * 4.0).clamp(1.0, 21.0),
            output_gain: output_level.clamp(0.1, 2.0),
            pre_lp: 0.0,
            post_lp: 0.0,
            post_coeff: 0.1 + tone.clamp(0.0, 1.0) * 0.85,
            intensity: intensity.clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let pre_hp = x - self.pre_lp;
        self.pre_lp += 0.15 * (x - self.pre_lp);
        let driven = (x + pre_hp * 0.3) * self.input_gain;
        let shaped = driven.tanh();
        self.post_lp += self.post_coeff * (shaped - self.post_lp);
        let processed = self.post_lp * self.output_gain;
        (x * (1.0 - self.intensity) + processed * self.intensity).clamp(-1.0, 1.0)
    }
}

/// Filter effect wrapper: SVF with wet/dry mix.
#[derive(Debug, Clone)]
pub struct FilterEffect {
    svf: Svf,
    intensity: f32,
}

impl FilterEffect {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.svf.process(x);
        x * (1.0 - self.intensity) + y * self.intensity
    }
}

#[derive(Debug, Clone)]
pub enum EffectNode {
    Reverb(Reverb),
    Delay(Delay),
    Chorus(Chorus),
    Filter(FilterEffect),
    Compressor(Compressor),
    Distortion(Distortion),
}

impl EffectNode {
    pub fn from_config(sample_rate: f32, tempo: u32, config: &EffectConfig) -> Self {
        let intensity = config.intensity.clamp(0.0, 1.0);
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        match &config.effect {
            EffectType::Reverb {
                room_size,
                dampening,
                wet_level,
                pre_delay,
            } => EffectNode::Reverb(Reverb::new(
                sample_rate,
                *room_size,
                *dampening,
                *wet_level,
                *pre_delay,
                intensity,
            )),
            EffectType::Delay {
                delay_time,
                feedback,
                wet_level,
                sync_tempo,
                random_beats,
                random_rate,
                pitch_intervals,
                pitch_mode,
            } => {
                if random_beats.is_some() || !pitch_intervals.is_empty() {
                    let [min, max] = random_beats.unwrap_or_else(|| {
                        let beats = if *sync_tempo {
                            *delay_time
                        } else {
                            delay_time / seconds_per_beat
                        };
                        [beats, beats]
                    });
                    EffectNode::Delay(Delay::time_fracture(
                        sample_rate,
                        tempo,
                        min,
                        max,
                        *random_rate,
                        pitch_intervals,
                        *pitch_mode,
                        *feedback,
                        *wet_level,
                        intensity,
                    ))
                } else {
                    let seconds = if *sync_tempo {
                        delay_time * seconds_per_beat
                    } else {
                        *delay_time
                    };
                    EffectNode::Delay(Delay::new(
                        sample_rate,
                        seconds,
                        *feedback,
                        *wet_level,
                        intensity,
                    ))
                }
            }
            EffectType::Chorus {
                rate,
                depth,
                feedback,
                stereo_width: _,
            } => EffectNode::Chorus(Chorus::new(
                sample_rate,
                *rate,
                *depth,
                *feedback,
                intensity,
            )),
            EffectType::Filter {
                filter_type,
                cutoff,
                resonance,
                envelope_amount: _,
            } => EffectNode::Filter(FilterEffect {
                svf: Svf::new(sample_rate, *cutoff, resonance.max(0.1), filter_type.into()),
                intensity,
            }),
            EffectType::Compressor {
                threshold,
                ratio,
                attack,
                release,
            } => EffectNode::Compressor(Compressor::new(
                sample_rate,
                *threshold,
                *ratio,
                *attack,
                *release,
                intensity,
            )),
            EffectType::Distortion {
                drive,
                tone,
                output_level,
            } => EffectNode::Distortion(Distortion::new(*drive, *tone, *output_level, intensity)),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        match self {
            EffectNode::Reverb(e) => e.process(x),
            EffectNode::Delay(e) => e.process(x),
            EffectNode::Chorus(e) => e.process(x),
            EffectNode::Filter(e) => e.process(x),
            EffectNode::Compressor(e) => e.process(x),
            EffectNode::Distortion(e) => e.process(x),
        }
    }
}

/// An ordered list of stateful effects applied in series.
#[derive(Debug, Clone, Default)]
pub struct EffectsChain {
    nodes: Vec<EffectNode>,
}

impl EffectsChain {
    /// Build a chain at [`DEFAULT_TEMPO`], for callers with no sequence tempo.
    pub fn new(sample_rate: f32, configs: &[EffectConfig]) -> Self {
        Self::with_tempo(sample_rate, DEFAULT_TEMPO, configs)
    }

    /// Build a chain whose tempo-synced effects follow the sequence tempo.
    pub fn with_tempo(sample_rate: f32, tempo: u32, configs: &[EffectConfig]) -> Self {
        Self {
            nodes: configs
                .iter()
                .filter(|c| c.enabled)
                .map(|c| EffectNode::from_config(sample_rate, tempo, c))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn process(&mut self, mut x: f32) -> f32 {
        for node in &mut self.nodes {
            x = node.process(x);
        }
        x
    }

    /// Process a whole buffer in place.
    pub fn process_buffer(&mut self, samples: &mut [f32]) {
        if self.nodes.is_empty() {
            return;
        }
        for s in samples.iter_mut() {
            *s = self.process(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{
        db, db_amp, goertzel_power, rms, sine, white_noise, zero_crossing_rate,
    };

    const SR: f32 = 44100.0;

    fn impulse_response(d: &mut Delay, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| d.process(if i == 0 { 1.0 } else { 0.0 }))
            .collect()
    }

    fn first_echo_index(out: &[f32]) -> usize {
        out.iter()
            .enumerate()
            .skip(1)
            .find(|&(_, &x)| x.abs() > 0.05)
            .map(|(i, _)| i)
            .unwrap()
    }

    #[test]
    fn time_fracture_with_zero_rate_is_a_fixed_delay_at_min_beats() {
        // 0.5 beats at 120 BPM = 0.25 s = 11025 samples.
        let mut d = Delay::time_fracture(
            SR,
            120,
            0.5,
            2.0,
            0.0,
            &[],
            PitchMode::Random,
            0.0,
            1.0,
            1.0,
        );
        let out = impulse_response(&mut d, 30000);
        let echo = first_echo_index(&out);
        assert!((echo as i64 - 11025).abs() <= 2, "echo at {echo}");
        assert!(
            out[12000..].iter().all(|x| x.abs() < 1e-3),
            "no second echo without feedback"
        );
    }

    #[test]
    fn the_longest_supported_delay_fits_the_line() {
        // 4 beats (the validated maximum) at 30 BPM is 8 s, the line's length.
        let mut d =
            Delay::time_fracture(SR, 30, 4.0, 4.0, 0.0, &[], PitchMode::Random, 0.0, 1.0, 1.0);
        let out = impulse_response(&mut d, (8.5 * SR) as usize);
        let echo = first_echo_index(&out);
        assert!((echo as i64 - 352800).abs() <= 2, "echo at {echo}");
    }

    #[test]
    fn sync_tempo_puts_delay_time_in_beats() {
        let cfg: EffectConfig = serde_json::from_str(
            r#"{"type": "delay", "delay_time": 1.0, "sync_tempo": true, "feedback": 0.0, "wet_level": 1.0, "intensity": 1.0}"#,
        )
        .unwrap();
        let mut chain = EffectsChain::with_tempo(SR, 60, std::slice::from_ref(&cfg));
        let out: Vec<f32> = (0..60000)
            .map(|i| chain.process(if i == 0 { 1.0 } else { 0.0 }))
            .collect();
        assert!(
            (first_echo_index(&out) as i64 - 44100).abs() <= 2,
            "1 beat at 60 BPM is one second"
        );
    }

    #[test]
    fn static_delay_output_is_unchanged_by_the_tempo_plumbing() {
        let cfg: EffectConfig = serde_json::from_str(
            r#"{"type": "delay", "delay_time": 0.1, "feedback": 0.3, "intensity": 0.6}"#,
        )
        .unwrap();
        let mut a = EffectsChain::new(SR, std::slice::from_ref(&cfg));
        let mut b = EffectsChain::with_tempo(SR, 97, std::slice::from_ref(&cfg));
        let mut reference = Delay::new(SR, 0.1, 0.3, 0.3, 0.6);
        for i in 0..10000 {
            let x = if i % 500 == 0 { 0.8 } else { 0.0 };
            let (ya, yb, yr) = (a.process(x), b.process(x), reference.process(x));
            assert_eq!(ya, yb);
            assert_eq!(ya, yr);
        }
    }

    #[test]
    fn random_rate_moves_the_delay_time_within_the_beat_range() {
        let mut d = Delay::time_fracture(
            SR,
            120,
            0.25,
            1.0,
            3.0,
            &[],
            PitchMode::Random,
            0.0,
            1.0,
            1.0,
        );
        let (min_s, max_s) = (0.125 * SR, 0.5 * SR);
        let mut seen_min = f32::MAX;
        let mut seen_max = 0.0f32;
        for _ in 0..(2.0 * SR) as usize {
            d.process(0.0);
            let cur = d.current_delay_samples();
            assert!(
                cur >= min_s - 1.0 && cur <= max_s + 1.0,
                "{cur} outside [{min_s}, {max_s}]"
            );
            seen_min = seen_min.min(cur);
            seen_max = seen_max.max(cur);
        }
        assert!(
            seen_max - seen_min > 0.1 * (max_s - min_s),
            "delay time actually moves"
        );
    }

    #[test]
    fn zero_random_rate_holds_the_delay_time_even_with_pitch_intervals() {
        // `random_rate: 0` is documented as a static delay time. Pitch shifting
        // still runs the grain clock, so every wrap used to draw a new random
        // endpoint and glide the delay across the whole [min, max] range.
        let mut d = Delay::time_fracture(
            SR,
            120,
            0.25,
            1.0,
            0.0,
            &[12.0],
            PitchMode::Up,
            0.0,
            1.0,
            1.0,
        );
        let min_samples = 0.25 * 0.5 * SR;
        // Eight grains: with `random_rate` 0 the grain is `min_samples` long.
        for _ in 0..(8.0 * min_samples) as usize {
            d.process(0.0);
            let cur = d.current_delay_samples();
            assert!(
                (cur - min_samples).abs() < 1e-3,
                "delay time moved to {cur}, expected a fixed {min_samples}"
            );
        }
    }

    #[test]
    fn a_downward_repeat_restarts_before_the_read_saturates() {
        // 4 beats at 30 BPM is the longest supported delay. A -12 repeat walks
        // the read position backwards, so the restart has to fire on the line's
        // real bound; anything larger leaves `read_fractional` clamping (a stuck
        // read) instead. A slow grain clock keeps the phase from wrapping first.
        let mut d = Delay::time_fracture(
            SR,
            30,
            4.0,
            4.0,
            0.05,
            &[-12.0],
            PitchMode::Up,
            0.0,
            1.0,
            1.0,
        );
        let max_read = d.line.len() as f32 - 1.0;
        let f = d.fracture.as_mut().unwrap();
        let mut restarts = 0;
        let mut previous = 0.0f32;
        for _ in 0..(45.0 * SR) as usize {
            let delay = f.advance();
            assert!(
                (1.0..max_read).contains(&delay),
                "read saturated at {delay} (line bound {max_read})"
            );
            if delay < previous {
                restarts += 1;
            }
            previous = delay;
        }
        assert!(restarts >= 2, "expected repeats to restart, saw {restarts}");
    }

    #[test]
    fn each_restart_draws_a_new_delay_time_when_the_time_is_random() {
        // An octave-up repeat restarts long before the slow grain clock wraps,
        // so without a fresh pair of endpoints every restart would reuse the
        // same delay time.
        let mut d = Delay::time_fracture(
            SR,
            120,
            0.25,
            1.0,
            1.0,
            &[12.0],
            PitchMode::Up,
            0.0,
            1.0,
            1.0,
        );
        let (min_s, max_s) = (0.125 * SR, 0.5 * SR);
        let f = d.fracture.as_mut().unwrap();
        // An upward repeat shortens the delay every sample, so a restart is the
        // one place the delay jumps back up.
        let mut restarts: Vec<f32> = Vec::new();
        let mut previous = f32::MAX;
        for _ in 0..(8.0 * SR) as usize {
            let delay = f.advance();
            if delay > previous + 1.0 {
                restarts.push(delay);
            }
            previous = delay;
        }
        // Eight seconds is eight grains at 1 Hz, and each grain holds two or
        // more repeats, so most restarts happen inside a grain.
        assert!(
            restarts.len() >= 12,
            "expected many restarts, saw {}",
            restarts.len()
        );
        assert!(
            restarts
                .iter()
                .all(|d| *d >= min_s - 1.0 && *d <= max_s + 1.0),
            "a restart landed outside [{min_s}, {max_s}]"
        );
        restarts.sort_by(f32::total_cmp);
        let distinct = 1 + restarts.windows(2).filter(|w| w[1] - w[0] > 1.0).count();
        assert!(
            distinct + 1 >= restarts.len(),
            "restarts reused delay times: {distinct} distinct of {}",
            restarts.len()
        );
    }

    #[test]
    fn pitch_interval_of_twelve_repeats_one_octave_up() {
        // No feedback: after the 0.3 s input burst ends, the output is the
        // pitch-shifted repeat alone.
        let mut d = Delay::time_fracture(
            SR,
            120,
            1.0,
            1.0,
            0.0,
            &[12.0],
            PitchMode::Up,
            0.0,
            1.0,
            1.0,
        );
        let burst = sine(220.0, 0.3, SR, 0.8);
        let total = (1.2 * SR) as usize;
        let out: Vec<f32> = (0..total)
            .map(|i| d.process(if i < burst.len() { burst[i] } else { 0.0 }))
            .collect();
        // Delay is 0.5 s (1 beat at 120); the shifted repeat plays the 0.3 s burst
        // at double speed, so it occupies roughly 0.5..0.65 s.
        let window = &out[(0.52 * SR) as usize..(0.62 * SR) as usize];
        assert!(rms(window) > 0.05, "repeat is audible");
        let zcr = zero_crossing_rate(window, SR);
        assert!(
            (zcr - 880.0).abs() < 60.0,
            "an octave up: {zcr} crossings/s"
        );
        // Nothing wet has arrived yet while the input plays, so the output
        // there is exactly the dry input at the mix law's `1 - wet * 0.5`.
        let (from, to) = ((0.05 * SR) as usize, (0.25 * SR) as usize);
        let dry_rms = rms(&out[from..to]);
        assert!(
            (dry_rms - 0.5 * rms(&burst[from..to])).abs() < 1e-4,
            "dry only before the delay: {dry_rms}"
        );
    }

    #[test]
    fn pitch_modes_visit_intervals_in_order() {
        let mut up = Delay::time_fracture(
            SR,
            120,
            1.0,
            1.0,
            0.0,
            &[0.0, 12.0, -12.0],
            PitchMode::Up,
            0.0,
            1.0,
            1.0,
        );
        assert_eq!(up.next_pitch_semitones(), 0.0);
        assert_eq!(up.next_pitch_semitones(), 12.0);
        assert_eq!(up.next_pitch_semitones(), -12.0);
        assert_eq!(up.next_pitch_semitones(), 0.0);
        let mut down = Delay::time_fracture(
            SR,
            120,
            1.0,
            1.0,
            0.0,
            &[0.0, 12.0, -12.0],
            PitchMode::Down,
            0.0,
            1.0,
            1.0,
        );
        assert_eq!(down.next_pitch_semitones(), 0.0);
        assert_eq!(down.next_pitch_semitones(), -12.0);
        assert_eq!(down.next_pitch_semitones(), 12.0);
        let mut ud = Delay::time_fracture(
            SR,
            120,
            1.0,
            1.0,
            0.0,
            &[0.0, 5.0, 12.0],
            PitchMode::UpDown,
            0.0,
            1.0,
            1.0,
        );
        let seq: Vec<f32> = (0..6).map(|_| ud.next_pitch_semitones()).collect();
        assert_eq!(seq, vec![0.0, 5.0, 12.0, 5.0, 0.0, 5.0]);
        let mut random = Delay::time_fracture(
            SR,
            120,
            1.0,
            1.0,
            0.0,
            &[3.0, 7.0],
            PitchMode::Random,
            0.0,
            1.0,
            1.0,
        );
        for _ in 0..20 {
            let s = random.next_pitch_semitones();
            assert!(s == 3.0 || s == 7.0);
        }
    }

    #[test]
    fn time_fracture_with_feedback_stays_bounded() {
        let mut d = Delay::time_fracture(
            SR,
            120,
            0.25,
            1.0,
            4.0,
            &[7.0, 12.0],
            PitchMode::UpDown,
            0.9,
            1.0,
            1.0,
        );
        let noise = white_noise((3.0 * SR) as usize, 7);
        let mut peak = 0.0f32;
        for x in noise {
            let y = d.process(x * 0.5);
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        assert!(peak <= 1.0, "output is clamped: {peak}");
    }

    #[test]
    fn delay_produces_echo_at_configured_time() {
        let mut d = Delay::new(SR, 0.1, 0.0, 1.0, 1.0);
        let mut out = Vec::new();
        for i in 0..6000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(d.process(x));
        }
        // The one-pole damping smears the echo over a few samples around 4410.
        let echo: f32 = out[4405..4420].iter().map(|v| v.abs()).sum();
        let silence: f32 = out[1000..4000].iter().map(|v| v.abs()).sum();
        assert!(echo > 0.3, "echo energy {echo} too small");
        assert!(
            silence < 1e-3,
            "unexpected signal before the echo: {silence}"
        );
    }

    #[test]
    fn delay_line_fractional_read_never_indexes_past_the_end() {
        // Regression: a delay a hair longer than the write position used to
        // produce a position that rounded to exactly `len`.
        let mut line = DelayLine::new(1764);
        for i in 0..5 {
            line.write(i as f32);
        }
        for delay in [1.0, 4.9999995, 5.0, 5.0000005, 1762.9999, 1763.0, 5000.0] {
            let _ = line.read_fractional(delay);
        }
        // Exact integer delays read back what was written that long ago.
        assert_eq!(line.read_fractional(1.0), 4.0);
        assert_eq!(line.read_fractional(3.0), 2.0);
    }

    #[test]
    fn chorus_survives_a_long_render() {
        let mut c = Chorus::new(SR, 0.8, 1.0, 0.2, 1.0);
        let input = white_noise((SR * 20.0) as usize, 11);
        let max = input
            .iter()
            .map(|&x| c.process(x).abs())
            .fold(0.0, f32::max);
        assert!(max.is_finite() && max < 4.0);
    }

    #[test]
    fn svf_lowpass_attenuates_high_frequencies() {
        let input = white_noise(SR as usize, 7);
        let mut svf = Svf::new(SR, 500.0, 0.707, SvfMode::LowPass);
        let output: Vec<f32> = input.iter().map(|&x| svf.process(x)).collect();

        let hi_in = goertzel_power(&input, 8000.0, SR);
        let hi_out = goertzel_power(&output, 8000.0, SR);
        let lo_in = goertzel_power(&input, 100.0, SR);
        let lo_out = goertzel_power(&output, 100.0, SR);

        assert!(
            db(hi_out / hi_in) < -20.0,
            "8 kHz should drop by >20 dB, got {} dB",
            db(hi_out / hi_in)
        );
        assert!(
            db(lo_out / lo_in) > -3.0,
            "100 Hz should pass, got {} dB",
            db(lo_out / lo_in)
        );
    }

    #[test]
    fn svf_stays_stable_at_high_cutoff() {
        let mut svf = Svf::new(SR, 20000.0, 20.0, SvfMode::LowPass);
        let input = white_noise(SR as usize, 3);
        let max = input
            .iter()
            .map(|&x| svf.process(x).abs())
            .fold(0.0, f32::max);
        assert!(max.is_finite() && max < 50.0, "filter blew up: {max}");
    }

    #[test]
    fn reverb_has_a_tail() {
        let mut r = Reverb::new(SR, 0.7, 0.3, 0.5, 0.0, 1.0);
        let mut out = Vec::new();
        for i in 0..(SR as usize) {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(r.process(x));
        }
        let after_200ms: f32 = out[8820..].iter().map(|v| v.abs()).sum();
        assert!(after_200ms > 0.05, "no reverb tail: {after_200ms}");
    }

    #[test]
    fn compressor_reduces_loud_signal() {
        let input = sine(1000.0, 1.0, SR, 0.5);
        let mut c = Compressor::new(SR, -12.0, 4.0, 0.001, 0.05, 1.0);
        let out: Vec<f32> = input.iter().map(|&x| c.process(x)).collect();
        let peak_in = input[22050..].iter().map(|v| v.abs()).fold(0.0, f32::max);
        let peak_out = out[22050..].iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(
            db_amp(peak_out / peak_in) < -3.0,
            "expected >3 dB reduction, got {} dB",
            db_amp(peak_out / peak_in)
        );
    }

    #[test]
    fn chain_skips_disabled_effects_and_passes_through_when_empty() {
        let configs = vec![EffectConfig {
            effect: EffectType::Distortion {
                drive: 5.0,
                tone: 0.5,
                output_level: 1.0,
            },
            intensity: 1.0,
            enabled: false,
        }];
        let mut chain = EffectsChain::new(SR, &configs);
        assert!(chain.is_empty());
        assert_eq!(chain.process(0.25), 0.25);
    }
}
