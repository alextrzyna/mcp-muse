//! Synthesis engine: R2D2 vocalizations and the general-purpose synthesizer.
//!
//! Everything renders to a sample buffer up front (`Vec<f32>` at 44.1 kHz); the
//! player then schedules those buffers. The pipeline for a musical note is:
//! oscillator → ADSR → filter → effects → amplitude. Percussive types skip the
//! ADSR because their generators already shape the sound.

use crate::expressive::effects::{EffectsChain, Svf, SvfMode};
use crate::expressive::percussion;
use crate::midi::EffectConfig;
use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Core expressive synthesizer for R2D2-style vocalizations and music synthesis.
pub struct ExpressiveSynth {
    sample_rate: f32,
}

impl Default for ExpressiveSynth {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters for one synthesized note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthParams {
    pub synth_type: SynthType,
    pub frequency: f32,
    pub amplitude: f32,
    pub duration: f32,
    pub envelope: EnvelopeParams,
    pub filter: Option<FilterParams>,
    /// Effects applied to this note after the filter.
    pub effects: Vec<EffectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SynthType {
    // Basic oscillator synthesis
    Sine,
    Square {
        pulse_width: f32,
    },
    Sawtooth,
    Triangle,
    Noise {
        color: NoiseColor,
    },

    // Advanced synthesis techniques
    FM {
        modulator_freq: f32,
        modulation_index: f32,
    },
    // DX7-style 6-operator FM synthesis
    DX7FM {
        algorithm: u8,               // 1-32 (DX7 algorithms)
        operators: [DX7Operator; 6], // 6 operators like real DX7
    },
    Granular {
        grain_size: f32,
        overlap: f32,
        density: f32,
    },
    Wavetable {
        position: f32,
        morph_speed: f32,
    },

    // Percussion synthesis
    Kick {
        punch: f32,
        sustain: f32,
        click_freq: f32,
        body_freq: f32,
    },
    Snare {
        snap: f32,
        buzz: f32,
        tone_freq: f32,
        noise_amount: f32,
    },
    HiHat {
        metallic: f32,
        decay: f32,
        brightness: f32,
    },
    Cymbal {
        size: f32,
        metallic: f32,
        strike_intensity: f32,
    },

    // Sound effects synthesis
    Swoosh {
        direction: f32, // -1.0 to 1.0 for left-to-right sweep
        intensity: f32,
        frequency_sweep: (f32, f32), // (start_freq, end_freq)
    },
    Zap {
        energy: f32,
        decay: f32,
        harmonic_content: f32,
    },
    Chime {
        fundamental: f32,
        harmonic_count: u8,
        decay: f32,
        inharmonicity: f32,
    },
    Burst {
        center_freq: f32,
        bandwidth: f32,
        intensity: f32,
        shape: f32, // 0.0=sharp, 1.0=smooth
    },

    // Ambient textures
    Pad {
        warmth: f32,
        movement: f32,
        space: f32,
        harmonic_evolution: f32,
    },
    Texture {
        roughness: f32,
        evolution: f32,
        spectral_tilt: f32,
        modulation_depth: f32,
    },
    Drone {
        fundamental: f32,
        overtone_spread: f32,
        modulation: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoiseColor {
    White,
    #[allow(dead_code)]
    Pink,
    #[allow(dead_code)]
    Brown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl EnvelopeParams {
    /// ADSR level at time `t` for a note of `duration` seconds.
    /// The release starts `release` seconds before the end (or as soon as
    /// attack+decay finish if the note is shorter) and never goes negative.
    pub fn level_at(&self, t: f32, duration: f32) -> f32 {
        let a = self.attack.max(0.0);
        let d = self.decay.max(0.0);
        let r = self.release.max(0.0);
        let s = self.sustain.clamp(0.0, 1.0);

        let pre_release = |t: f32| -> f32 {
            if t < a {
                if a > 0.0 { t / a } else { 1.0 }
            } else if t < a + d {
                let p = if d > 0.0 { (t - a) / d } else { 1.0 };
                1.0 - p * (1.0 - s)
            } else {
                s
            }
        };

        let release_start = (duration - r).max(0.0);
        if t < release_start {
            pre_release(t)
        } else {
            let p = if r > 0.0 {
                (t - release_start) / r
            } else {
                1.0
            };
            pre_release(release_start) * (1.0 - p).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterParams {
    pub cutoff: f32,
    /// 0.0 (flat) to 1.0 (self-oscillating edge)
    pub resonance: f32,
    pub filter_type: FilterType,
}

impl FilterParams {
    fn to_svf(&self, sample_rate: f32) -> Svf {
        let q = 0.5 + self.resonance.clamp(0.0, 1.0) * 9.5;
        let mode = match self.filter_type {
            FilterType::LowPass => SvfMode::LowPass,
            FilterType::HighPass => SvfMode::HighPass,
            FilterType::BandPass => SvfMode::BandPass,
        };
        Svf::new(sample_rate, self.cutoff, q, mode)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DX7Operator {
    pub frequency_ratio: f32,     // Frequency ratio (0.5, 1.0, 2.0, etc.)
    pub output_level: f32,        // Operator output level (0.0-1.0)
    pub detune: f32,              // Fine detune in cents
    pub envelope: EnvelopeParams, // Individual operator envelope
}

impl Default for DX7Operator {
    fn default() -> Self {
        Self {
            frequency_ratio: 1.0,
            output_level: 0.0, // silent unless a preset switches it on
            detune: 0.0,
            envelope: EnvelopeParams {
                attack: 0.01,
                decay: 0.3,
                sustain: 0.7,
                release: 0.5,
            },
        }
    }
}

/// Sine oscillator that integrates instantaneous frequency, so it stays
/// correct when the frequency changes every sample.
#[derive(Debug, Clone, Copy)]
pub struct PhaseAccumulator {
    phase: f32,
    sample_rate: f32,
}

impl PhaseAccumulator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
        }
    }

    /// Advance by one sample at `freq` Hz and return the phase *before* advancing.
    #[inline]
    pub fn next_phase(&mut self, freq: f32) -> f32 {
        let current = self.phase;
        self.phase = (self.phase + TAU * freq / self.sample_rate).rem_euclid(TAU);
        current
    }

    /// Advance by one sample and return `sin(phase)`.
    #[inline]
    pub fn next(&mut self, freq: f32) -> f32 {
        self.next_phase(freq).sin()
    }

    /// Advance and return phase normalized to 0..1 (for non-sine waveforms).
    #[inline]
    fn next_unit(&mut self, freq: f32) -> f32 {
        self.next_phase(freq) / TAU
    }
}

/// Polynomial band-limited step correction for saw/square discontinuities.
#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// DX7 algorithm as a modulation graph over operators 0..6 (op1 = index 0).
struct Dx7Algorithm {
    /// For each operator, the operators that modulate it.
    modulators: [&'static [usize]; 6],
    carriers: &'static [usize],
}

fn dx7_algorithm(number: u8) -> Dx7Algorithm {
    match number {
        // Two stacks: 2→1, 6→5→4→3
        1 | 2 => Dx7Algorithm {
            modulators: [&[1], &[], &[3], &[4], &[5], &[]],
            carriers: &[0, 2],
        },
        // Three stacks: 2→1, 4→3, 6→5
        5 | 6 => Dx7Algorithm {
            modulators: [&[1], &[], &[3], &[], &[5], &[]],
            carriers: &[0, 2, 4],
        },
        // One carrier: 1 ← 2, 3, 5; 3 ← 4; 5 ← 6
        16 | 17 => Dx7Algorithm {
            modulators: [&[1, 2, 4], &[], &[3], &[], &[5], &[]],
            carriers: &[0],
        },
        // All carriers (additive)
        32 => Dx7Algorithm {
            modulators: [&[], &[], &[], &[], &[], &[]],
            carriers: &[0, 1, 2, 3, 4, 5],
        },
        // Fallback: three two-operator stacks, the most common DX7 shape.
        _ => Dx7Algorithm {
            modulators: [&[1], &[], &[3], &[], &[5], &[]],
            carriers: &[0, 2, 4],
        },
    }
}

/// Phase deviation in radians for a modulator at full output level.
const DX7_MOD_DEPTH: f32 = 4.0;

/// Per-render oscillator state for the non-percussive synthesis types.
struct Oscillator {
    sample_rate: f32,
    main: PhaseAccumulator,
    aux: PhaseAccumulator,
    dx7: [PhaseAccumulator; 6],
    pink: [f32; 3],
    brown: f32,
    rng: rand::rngs::ThreadRng,
}

impl Oscillator {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            main: PhaseAccumulator::new(sample_rate),
            aux: PhaseAccumulator::new(sample_rate),
            dx7: [PhaseAccumulator::new(sample_rate); 6],
            pink: [0.0; 3],
            brown: 0.0,
            rng: rand::rng(),
        }
    }

    fn white(&mut self) -> f32 {
        self.rng.random::<f32>() * 2.0 - 1.0
    }

    /// One sample of the raw (unenveloped) waveform at time `t`.
    fn sample(&mut self, params: &SynthParams, t: f32) -> f32 {
        let freq = params.frequency;
        let dt = freq / self.sample_rate;

        match &params.synth_type {
            SynthType::Sine => self.main.next(freq),
            SynthType::Square { pulse_width } => {
                let pw = pulse_width.clamp(0.05, 0.95);
                let p = self.main.next_unit(freq);
                let naive = if p < pw { 1.0 } else { -1.0 };
                naive + poly_blep(p, dt) - poly_blep((p + 1.0 - pw).rem_euclid(1.0), dt)
            }
            SynthType::Sawtooth => {
                let p = self.main.next_unit(freq);
                2.0 * p - 1.0 - poly_blep(p, dt)
            }
            SynthType::Triangle => {
                let p = self.main.next_unit(freq);
                if p < 0.5 {
                    4.0 * p - 1.0
                } else {
                    3.0 - 4.0 * p
                }
            }
            SynthType::Noise { color } => {
                let white = self.white();
                match color {
                    NoiseColor::White => white,
                    NoiseColor::Pink => {
                        // Paul Kellet's economy pink filter.
                        self.pink[0] = 0.99765 * self.pink[0] + white * 0.0990460;
                        self.pink[1] = 0.96300 * self.pink[1] + white * 0.2965164;
                        self.pink[2] = 0.57000 * self.pink[2] + white * 1.0526913;
                        (self.pink[0] + self.pink[1] + self.pink[2] + white * 0.1848) * 0.25
                    }
                    NoiseColor::Brown => {
                        self.brown = (self.brown + white * 0.02).clamp(-1.0, 1.0);
                        self.brown * 3.5
                    }
                }
            }
            SynthType::FM {
                modulator_freq,
                modulation_index,
            } => {
                let modulator = self.aux.next(*modulator_freq);
                (self.main.next_phase(freq) + modulation_index * modulator).sin()
            }
            SynthType::DX7FM {
                algorithm,
                operators,
            } => {
                let algo = dx7_algorithm(*algorithm);
                let mut outputs = [0.0f32; 6];
                // Modulators have higher indices than the operators they
                // modulate in every table above, so a reverse pass resolves
                // each operator after its modulators.
                for i in (0..6).rev() {
                    let op = &operators[i];
                    if op.output_level <= 0.0 {
                        continue;
                    }
                    let op_freq = freq * op.frequency_ratio * 2f32.powf(op.detune / 1200.0);
                    let mut phase = self.dx7[i].next_phase(op_freq);
                    for &m in algo.modulators[i] {
                        phase += outputs[m] * DX7_MOD_DEPTH;
                    }
                    let env = op.envelope.level_at(t, params.duration);
                    outputs[i] = phase.sin() * op.output_level * env;
                }
                let sum: f32 = algo.carriers.iter().map(|&c| outputs[c]).sum();
                sum / (algo.carriers.len() as f32).sqrt()
            }
            SynthType::Granular {
                grain_size,
                overlap,
                density,
            } => {
                // Pitched granular cloud: overlapping Hann-windowed grains that
                // mostly follow the fundamental with a little spread.
                let grain_period = (grain_size * (1.0 - overlap.clamp(0.0, 0.95))).max(0.002);
                let grain_index = (t / grain_period) as i32;
                let grains = (density * 4.0).clamp(1.0, 16.0) as i32;
                let mut output = 0.0;
                for i in 0..grains {
                    let grain_start = (grain_index - i) as f32 * grain_period;
                    let local = t - grain_start;
                    if local < 0.0 || local > *grain_size {
                        continue;
                    }
                    let window = 0.5 * (1.0 - (TAU * local / grain_size).cos());
                    // Deterministic per-grain detune keeps each grain stable.
                    let seed = ((grain_index - i) as f32 * 12.9898).sin() * 43758.547;
                    let detune = 1.0 + (seed.fract() - 0.5) * 0.06;
                    let tonal = (TAU * freq * detune * local).sin() * 0.7;
                    let breath = self.white() * 0.15;
                    output += (tonal + breath) * window * 0.25 * density;
                }
                output.clamp(-1.0, 1.0)
            }
            SynthType::Wavetable {
                position,
                morph_speed,
            } => {
                let p = self.main.next_unit(freq);
                let sine = (p * TAU).sin();
                let triangle = if p < 0.5 {
                    4.0 * p - 1.0
                } else {
                    3.0 - 4.0 * p
                };
                let saw = 2.0 * p - 1.0 - poly_blep(p, dt);
                let square = (if p < 0.5 { 1.0 } else { -1.0 }) + poly_blep(p, dt)
                    - poly_blep((p + 0.5).rem_euclid(1.0), dt);

                let morph = (t * morph_speed).sin() * 0.5 + 0.5;
                let pos = (position + morph).rem_euclid(1.0) * 4.0;
                let (a, b, blend) = match pos {
                    x if x < 1.0 => (sine, triangle, x),
                    x if x < 2.0 => (triangle, saw, x - 1.0),
                    x if x < 3.0 => (saw, square, x - 2.0),
                    x => (square, sine, x - 3.0),
                };
                a * (1.0 - blend) + b * blend
            }
            SynthType::Chime {
                fundamental,
                harmonic_count,
                decay,
                inharmonicity,
            } => {
                let count = (*harmonic_count).max(1) as usize;
                let mut output = 0.0;
                for i in 1..=count {
                    let partial =
                        fundamental * i as f32 * (1.0 + inharmonicity * (i as f32 - 1.0) * 0.01);
                    let env = (-t * decay * (1.0 + i as f32 * 0.1)).exp();
                    output += (TAU * partial * t).sin() * env / i as f32;
                }
                output / (count as f32).sqrt()
            }
            SynthType::Burst {
                center_freq,
                bandwidth,
                intensity,
                shape,
            } => {
                let progress = t / params.duration.max(0.1);
                let env = if *shape < 0.5 {
                    (-progress * 8.0).exp()
                } else {
                    (-(progress * 3.0).powi(2)).exp()
                };
                let tone = self.main.next(*center_freq) * 0.3;
                // Noise shaped by a one-pole tracking the centre; crude but cheap.
                let alpha = (TAU * (center_freq + bandwidth) / self.sample_rate).min(0.99);
                let white = self.white();
                self.brown += alpha * (white - self.brown);
                (self.brown * 1.5 + tone) * env * intensity
            }
            SynthType::Pad {
                warmth,
                movement,
                space,
                harmonic_evolution,
            } => {
                let base = self.main.next_phase(freq);
                let mut output = 0.0;
                for i in 1..=8 {
                    let amp = 1.0 / (i as f32).powf(0.7 + warmth * 0.5);
                    let evolve = 1.0 + (t * harmonic_evolution + i as f32).sin() * 0.1;
                    output += (base * i as f32).sin() * amp * evolve;
                }
                let lfo = 1.0 + (TAU * 0.2 * t).sin() * movement * 0.1;
                output * lfo * (0.5 + space * 0.2) * 0.3
            }
            SynthType::Texture {
                roughness,
                evolution,
                spectral_tilt,
                modulation_depth,
            } => {
                let osc = self.main.next(freq);
                let modulator = self.aux.next(freq * 0.618) * modulation_depth;
                let evolve = (t * evolution * 0.5).sin() * 0.5 + 0.5;
                let tilt = if *spectral_tilt > 0.5 {
                    1.0 + (freq / 1000.0) * (spectral_tilt - 0.5) * 2.0
                } else {
                    1.0 + (1000.0 / freq.max(100.0)) * (0.5 - spectral_tilt) * 2.0
                };
                let white = self.white();
                let mix = osc * (1.0 - roughness) + white * roughness;
                mix * (1.0 + modulator) * evolve * tilt.min(3.0) * 0.7
            }
            SynthType::Drone {
                fundamental,
                overtone_spread,
                modulation,
            } => {
                let base = self.main.next_phase(*fundamental);
                let mut output = base.sin() * 0.4;
                for i in 2..=6 {
                    let ratio = i as f32 * (1.0 + overtone_spread * 0.1 * (i as f32 - 1.0));
                    let wobble = 1.0 + (t * modulation * 0.1 + i as f32).sin() * 0.05;
                    output += (base * ratio).sin() * (0.3 / i as f32) * wobble;
                }
                output * ((TAU * 0.1 * t).sin() * 0.05 + 1.0) * 0.8
            }
            // Percussive types are rendered by the percussion module.
            SynthType::Kick { .. }
            | SynthType::Snare { .. }
            | SynthType::HiHat { .. }
            | SynthType::Cymbal { .. }
            | SynthType::Zap { .. }
            | SynthType::Swoosh { .. } => 0.0,
        }
    }
}

/// Extra silence rendered through the effects chain so reverb/delay tails
/// are not cut off at the note boundary.
const EFFECT_TAIL_SECONDS: f32 = 1.0;

impl ExpressiveSynth {
    pub const SAMPLE_RATE: f32 = 44100.0;

    pub fn new() -> Self {
        ExpressiveSynth {
            sample_rate: Self::SAMPLE_RATE,
        }
    }

    /// Render a note: oscillator → ADSR → filter → effects → amplitude.
    pub fn generate_synthesized_samples(&self, params: &SynthParams) -> Result<Vec<f32>> {
        let sample_count = (self.sample_rate * params.duration.max(0.0)) as usize;
        let sr = self.sample_rate;

        let mut samples =
            match percussion::render(sr, &params.synth_type, params.frequency, sample_count) {
                Some(rendered) => rendered,
                None => {
                    let mut osc = Oscillator::new(sr);
                    let shape_with_adsr = !matches!(
                        params.synth_type,
                        SynthType::Chime { .. } | SynthType::Burst { .. }
                    );
                    (0..sample_count)
                        .map(|i| {
                            let t = i as f32 / sr;
                            let raw = osc.sample(params, t);
                            if shape_with_adsr {
                                raw * params.envelope.level_at(t, params.duration)
                            } else {
                                raw
                            }
                        })
                        .collect()
                }
            };

        if let Some(filter) = &params.filter {
            let mut svf = filter.to_svf(sr);
            for s in &mut samples {
                *s = svf.process(*s);
            }
        }

        let mut chain = EffectsChain::new(sr, &params.effects);
        if !chain.is_empty() {
            samples.resize(sample_count + (EFFECT_TAIL_SECONDS * sr) as usize, 0.0);
            chain.process_buffer(&mut samples);
        }

        for s in &mut samples {
            *s *= params.amplitude;
        }
        Ok(samples)
    }

    /// Ben Burtt-style ring-modulated vocalization following a pitch contour.
    pub fn generate_r2d2_samples_with_contour(
        &self,
        base_freq: f32,
        emotion_intensity: f32,
        duration: f32,
        pitch_contour: &[f32],
    ) -> Vec<f32> {
        let sample_count = (self.sample_rate * duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);

        let mut carrier = PhaseAccumulator::new(self.sample_rate);
        let mut modulator = PhaseAccumulator::new(self.sample_rate);
        let mut harmonic = PhaseAccumulator::new(self.sample_rate);

        // Subtle vibrato that preserves the contour.
        let vibrato_rate = 1.8;
        let vibrato_depth = 0.008;

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let progress = t / duration;

            let pitch_multiplier =
                Self::interpolate_pitch_contour(progress, pitch_contour, emotion_intensity);
            let vibrato = (TAU * vibrato_rate * t).sin() * vibrato_depth;
            let carrier_freq = base_freq * pitch_multiplier * (1.0 + vibrato);
            // Golden-ratio modulator for an inharmonic, organic timbre.
            let mod_freq = carrier_freq * 0.618 * (1.0 + vibrato * 0.2);

            let ring_mod = carrier.next(carrier_freq) * modulator.next(mod_freq);
            let overtone = if pitch_multiplier > 1.5 {
                harmonic.next(carrier_freq * 1.1) * 0.05
            } else {
                harmonic.next(carrier_freq * 1.05) * 0.02
            };

            let voice = ring_mod * 0.75 + overtone;
            let envelope =
                Self::calculate_emotion_envelope(t, duration, emotion_intensity, pitch_contour);
            samples.push(Self::tube_saturation(voice) * envelope * 0.28);
        }

        samples
    }

    /// Map contour progress (0..1) to a frequency multiplier.
    fn interpolate_pitch_contour(progress: f32, pitch_contour: &[f32], intensity: f32) -> f32 {
        if pitch_contour.is_empty() {
            return 1.0;
        }
        if pitch_contour.len() == 1 {
            return 1.0 + pitch_contour[0] * intensity;
        }

        let scaled = progress.clamp(0.0, 1.0) * (pitch_contour.len() - 1) as f32;
        let index = (scaled.floor() as usize).min(pitch_contour.len() - 1);
        let fraction = scaled - index as f32;
        let current = pitch_contour[index];
        let next = pitch_contour.get(index + 1).copied().unwrap_or(current);
        let value = current + (next - current) * fraction;

        // Contour values are 0..1; spread them over a dramatic pitch range.
        (0.4 + value * intensity * 2.0).clamp(0.2, 3.0)
    }

    /// Envelope shape chosen from the contour's overall direction.
    fn calculate_emotion_envelope(
        t: f32,
        duration: f32,
        emotion_intensity: f32,
        pitch_contour: &[f32],
    ) -> f32 {
        let progress = t / duration;

        let envelope = if pitch_contour.len() >= 3 {
            let start = pitch_contour[0];
            let end = pitch_contour[pitch_contour.len() - 1];

            if end > start + 0.4 {
                // Rising (Curious, Surprised): quick attack, sustained
                if progress < 0.1 {
                    progress * 10.0
                } else if progress < 0.8 {
                    1.0
                } else {
                    (1.0 - progress) * 5.0
                }
            } else if start > end + 0.4 {
                // Falling (Sad, Negative): slower attack, gradual fade
                if progress < 0.2 {
                    progress * 5.0
                } else {
                    (1.0 - progress) * 1.25
                }
            } else {
                // Bouncy (Happy, Excited): punchy with rhythmic pulses
                let bounce = (progress * std::f32::consts::PI * 3.0).sin().abs();
                if progress < 0.1 {
                    progress * 10.0
                } else if progress < 0.9 {
                    0.8 + bounce * 0.2
                } else {
                    (1.0 - progress) * 10.0
                }
            }
        } else {
            let attack = 0.02 + emotion_intensity * 0.03;
            let decay = 0.05 + emotion_intensity * 0.05;
            if t < attack {
                t / attack
            } else if t < duration - decay {
                1.0 - (t - attack) * 0.1 / (duration - attack - decay).max(0.001)
            } else {
                (duration - t) / decay
            }
        };

        envelope.clamp(0.0, 1.0)
    }

    /// Gentle saturation above ±0.5 for warmth.
    fn tube_saturation(x: f32) -> f32 {
        if x.abs() < 0.5 {
            x
        } else {
            x.signum() * (0.5 + (x.abs() - 0.5) * 0.6)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn params(synth_type: SynthType, frequency: f32, duration: f32) -> SynthParams {
        SynthParams {
            synth_type,
            frequency,
            amplitude: 0.8,
            duration,
            envelope: EnvelopeParams {
                attack: 0.01,
                decay: 0.05,
                sustain: 0.8,
                release: 0.05,
            },
            filter: None,
            effects: Vec::new(),
        }
    }

    #[test]
    fn envelope_never_goes_negative_on_short_notes() {
        let env = EnvelopeParams {
            attack: 0.5,
            decay: 0.5,
            sustain: 0.6,
            release: 3.0,
        };
        for i in 0..=100 {
            let t = i as f32 * 0.02;
            let v = env.level_at(t, 1.0);
            assert!((0.0..=1.0).contains(&v), "t={t} level={v}");
        }
        assert_eq!(env.level_at(1.0, 1.0), 0.0);
    }

    #[test]
    fn lowpass_filter_removes_high_harmonics_of_sawtooth() {
        let synth = ExpressiveSynth::new();
        let mut p = params(SynthType::Sawtooth, 110.0, 1.0);
        let dry = synth.generate_synthesized_samples(&p).unwrap();
        p.filter = Some(FilterParams {
            cutoff: 500.0,
            resonance: 0.1,
            filter_type: FilterType::LowPass,
        });
        let wet = synth.generate_synthesized_samples(&p).unwrap();

        let ratio_hi = goertzel_power(&wet, 7920.0, SR) / goertzel_power(&dry, 7920.0, SR);
        let ratio_lo = goertzel_power(&wet, 110.0, SR) / goertzel_power(&dry, 110.0, SR);
        assert!(
            db(ratio_hi) < -20.0,
            "72nd harmonic only dropped {} dB",
            db(ratio_hi)
        );
        assert!(
            db(ratio_lo) > -3.0,
            "fundamental dropped {} dB",
            db(ratio_lo)
        );
    }

    #[test]
    fn descending_r2d2_contour_keeps_descending() {
        let synth = ExpressiveSynth::new();
        let contour = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.0];
        let samples = synth.generate_r2d2_samples_with_contour(300.0, 0.7, 1.0, &contour);
        let n = samples.len();
        let first = zero_crossing_rate(&samples[n / 10..n / 5], SR);
        let middle = zero_crossing_rate(&samples[n / 2..n / 2 + n / 10], SR);
        let last = zero_crossing_rate(&samples[n * 4 / 5..n * 9 / 10], SR);
        assert!(
            first > middle && middle > last,
            "pitch not monotonic: {first} > {middle} > {last}"
        );
    }

    #[test]
    fn pad_respects_slow_attack() {
        let synth = ExpressiveSynth::new();
        let mut p = params(
            SynthType::Pad {
                warmth: 0.8,
                movement: 0.4,
                space: 0.6,
                harmonic_evolution: 0.3,
            },
            220.0,
            2.0,
        );
        p.envelope = EnvelopeParams {
            attack: 0.8,
            decay: 0.3,
            sustain: 0.8,
            release: 0.5,
        };
        let s = synth.generate_synthesized_samples(&p).unwrap();
        let early = rms(&s[..4410]);
        let late = rms(&s[39690..44100]);
        assert!(
            early < late * 0.25,
            "attack ignored: early={early} late={late}"
        );
    }

    #[test]
    fn kick_pitch_sweeps_downward() {
        let synth = ExpressiveSynth::new();
        let p = params(
            SynthType::Kick {
                punch: 0.8,
                sustain: 0.6,
                click_freq: 800.0,
                body_freq: 60.0,
            },
            60.0,
            0.6,
        );
        let s = synth.generate_synthesized_samples(&p).unwrap();
        let early = zero_crossing_rate(&s[441..2205], SR);
        let late = zero_crossing_rate(&s[13230..22050], SR);
        assert!(
            late < early,
            "kick should drop in pitch: early={early} late={late}"
        );
        assert!(
            late < 200.0,
            "kick body should settle near {} Hz, zcr={late}",
            60.0
        );
    }

    #[test]
    fn dx7_modulation_creates_sidebands() {
        let synth = ExpressiveSynth::new();
        let mut ops = [
            DX7Operator::default(),
            DX7Operator::default(),
            DX7Operator::default(),
            DX7Operator::default(),
            DX7Operator::default(),
            DX7Operator::default(),
        ];
        ops[0].output_level = 1.0;
        ops[1].output_level = 0.6;
        ops[1].frequency_ratio = 2.0;
        let p = params(
            SynthType::DX7FM {
                algorithm: 16,
                operators: ops,
            },
            200.0,
            1.0,
        );
        let s = synth.generate_synthesized_samples(&p).unwrap();
        let pure = synth
            .generate_synthesized_samples(&params(SynthType::Sine, 200.0, 1.0))
            .unwrap();
        // Carrier 200 Hz modulated at 400 Hz → sideband at 600 Hz.
        let sideband = goertzel_power(&s, 600.0, SR) / goertzel_power(&s, 200.0, SR);
        let pure_sideband = goertzel_power(&pure, 600.0, SR) / goertzel_power(&pure, 200.0, SR);
        assert!(db(sideband) > -30.0, "no FM sideband: {} dB", db(sideband));
        assert!(db(pure_sideband) < -60.0);
    }

    #[test]
    fn polyblep_saw_aliases_less_than_naive() {
        let synth = ExpressiveSynth::new();
        let p = params(SynthType::Sawtooth, 4000.0, 1.0);
        let blep = synth.generate_synthesized_samples(&p).unwrap();
        let naive: Vec<f32> = (0..blep.len())
            .map(|i| 2.0 * ((4000.0 * i as f32 / SR) % 1.0) - 1.0)
            .collect();
        // 4 kHz saw: harmonic 6 = 24 kHz folds to 20.1 kHz; that alias should be weaker.
        let alias_blep = goertzel_power(&blep, 20100.0, SR) / goertzel_power(&blep, 4000.0, SR);
        let alias_naive = goertzel_power(&naive, 20100.0, SR) / goertzel_power(&naive, 4000.0, SR);
        assert!(
            alias_blep < alias_naive * 0.5,
            "blep={alias_blep} naive={alias_naive}"
        );
    }

    #[test]
    fn effects_extend_buffer_for_tail() {
        let synth = ExpressiveSynth::new();
        let mut p = params(SynthType::Sine, 440.0, 0.5);
        p.effects = vec![EffectConfig {
            effect: crate::midi::EffectType::Reverb {
                room_size: 0.7,
                dampening: 0.3,
                wet_level: 0.5,
                pre_delay: 0.02,
            },
            intensity: 0.8,
            enabled: true,
        }];
        let s = synth.generate_synthesized_samples(&p).unwrap();
        assert_eq!(s.len(), (1.5 * SR) as usize);
        assert!(
            rms(&s[(0.6 * SR) as usize..(0.7 * SR) as usize]) > 1e-4,
            "no tail"
        );
    }
}
