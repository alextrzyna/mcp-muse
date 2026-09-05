//! Stateful audio effects.
//!
//! Every effect here keeps its own delay lines / filter memory across calls, so
//! it can be driven one sample at a time from the real-time mixer. Build an
//! [`EffectsChain`] once per channel from the user's [`EffectConfig`] list and
//! call [`EffectsChain::process`] for each sample.

use crate::midi::{EffectConfig, EffectType, FilterType};

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
        let len = self.buffer.len() as f32;
        let delay = delay.clamp(1.0, len - 1.0);
        let pos = (self.write as f32 - delay).rem_euclid(len);
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1) % self.buffer.len();
        let frac = pos - i0 as f32;
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

/// Feedback delay with one-pole damping in the loop.
#[derive(Debug, Clone)]
pub struct Delay {
    line: DelayLine,
    feedback: f32,
    lp: f32,
    wet: f32,
    dry: f32,
}

impl Delay {
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
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let delayed = self.line.read();
        self.lp += 0.5 * (delayed - self.lp);
        self.line.write(x + self.lp * self.feedback);
        (x * self.dry + self.lp * self.wet).clamp(-1.0, 1.0)
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
    pub fn from_config(sample_rate: f32, config: &EffectConfig) -> Self {
        let intensity = config.intensity.clamp(0.0, 1.0);
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
                sync_tempo: _,
            } => EffectNode::Delay(Delay::new(
                sample_rate,
                *delay_time,
                *feedback,
                *wet_level,
                intensity,
            )),
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
    pub fn new(sample_rate: f32, configs: &[EffectConfig]) -> Self {
        Self {
            nodes: configs
                .iter()
                .filter(|c| c.enabled)
                .map(|c| EffectNode::from_config(sample_rate, c))
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
    use crate::expressive::test_util::{db, db_amp, goertzel_power, sine, white_noise};

    const SR: f32 = 44100.0;

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
