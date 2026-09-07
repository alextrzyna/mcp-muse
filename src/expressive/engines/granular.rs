//! Granular synthesis: a cloud of short Hann-windowed grains read from a
//! single-cycle source waveform at the note's pitch, with random start
//! positions and random stereo placement.
//!
//! `GranularVoice` is not wired into the renderer until the next task, so
//! most of this module is unused for now.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{GateEnvelope, GrainSource, Granular};
use rand::{Rng, RngExt};
use std::f32::consts::{FRAC_PI_2, TAU};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::Adsr;
    use crate::expressive::test_util::{goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn cfg() -> Granular {
        Granular {
            level: 1.0,
            source: GrainSource::Harmonics,
            grain_ms: 50.0,
            density: 20.0,
            pitch_semitones: 0.0,
            randomness: 0.0,
            stereo_width: 0.0,
            env: fast(),
        }
    }

    fn render(c: &Granular, freq: f32, seconds: f32, mods: &Modulation) -> (Vec<f32>, Vec<f32>) {
        let mut v = GranularVoice::new(c, freq, SR);
        let mut l = Vec::new();
        let mut r = Vec::new();
        for _ in 0..(seconds * SR) as usize {
            let (a, b) = v.tick(mods);
            l.push(a);
            r.push(b);
        }
        (l, r)
    }

    #[test]
    fn every_source_is_a_normalised_single_cycle() {
        let mut rng = rand::rng();
        for source in GrainSource::ALL {
            let cycle = source_cycle(source, &mut rng);
            assert_eq!(cycle.len(), SOURCE_SAMPLES);
            let peak = cycle.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((peak - 1.0).abs() < 1e-3, "{source:?} peak {peak}");
            assert!(cycle.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn a_dense_cloud_of_harmonic_grains_is_pitched_at_the_note() {
        let (l, _) = render(&cfg(), 220.0, 1.0, &Modulation::default());
        assert!(rms(&l[4410..]) > 0.1, "audible");
        let f = goertzel_power(&l[4410..], 220.0, SR);
        let off = goertzel_power(&l[4410..], 330.0, SR);
        assert!(f > off * 10.0, "fundamental dominates a non-harmonic bin");
    }

    #[test]
    fn pitch_semitones_transposes_the_grains() {
        let mut up = cfg();
        up.pitch_semitones = 12.0;
        up.grain_ms = 200.0;
        let (a, _) = render(&cfg(), 220.0, 1.0, &Modulation::default());
        let (b, _) = render(&up, 220.0, 1.0, &Modulation::default());
        let za = zero_crossing_rate(&a[8820..], SR);
        let zb = zero_crossing_rate(&b[8820..], SR);
        assert!(
            zb > za * 1.6,
            "an octave up doubles the crossing rate: {za} -> {zb}"
        );
    }

    #[test]
    fn stereo_width_zero_is_mono_and_width_one_is_not() {
        let (l, r) = render(&cfg(), 220.0, 0.5, &Modulation::default());
        assert!(
            l.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-6),
            "width 0 is centred"
        );
        let mut wide = cfg();
        wide.stereo_width = 1.0;
        wide.randomness = 0.5;
        let (l, r) = render(&wide, 220.0, 0.5, &Modulation::default());
        let diff: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.05, "width 1 spreads grains across the field");
    }

    #[test]
    fn higher_density_is_louder_and_the_lfo_ratio_scales_it() {
        let mut sparse = cfg();
        sparse.density = 2.0;
        sparse.grain_ms = 20.0;
        let mut dense = sparse.clone();
        dense.density = 40.0;
        let (a, _) = render(&sparse, 220.0, 1.0, &Modulation::default());
        let (b, _) = render(&dense, 220.0, 1.0, &Modulation::default());
        assert!(rms(&b) > rms(&a) * 1.5, "{} vs {}", rms(&a), rms(&b));
        let doubled = Modulation {
            grain_density_ratio: 2.0,
            ..Modulation::default()
        };
        let (c, _) = render(&sparse, 220.0, 1.0, &doubled);
        assert!(
            rms(&c) > rms(&a) * 1.15,
            "density ratio 2 adds grains: {} vs {}",
            rms(&a),
            rms(&c)
        );
    }

    #[test]
    fn release_then_silence_and_level_apply() {
        let mut c = cfg();
        c.level = 0.5;
        c.env.release = 0.3;
        let mut v = GranularVoice::new(&c, 220.0, SR);
        let mods = Modulation::default();
        let s: Vec<f32> = (0..22050).map(|_| v.tick(&mods).0).collect();
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            peak <= 0.5 + 1e-3 && peak > 0.1,
            "level bounds the output: {peak}"
        );
        v.gate_off();
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
        assert!(v.tick(&mods).0 == 0.0);
    }

    #[test]
    fn never_exceeds_the_grain_cap_or_produces_nan() {
        let mut c = cfg();
        c.density = 50.0;
        c.grain_ms = 500.0;
        c.randomness = 1.0;
        c.stereo_width = 1.0;
        let (l, r) = render(&c, 55.0, 2.0, &Modulation::default());
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.5));
    }
}

pub const MAX_GRAINS: usize = 32;
pub const SOURCE_SAMPLES: usize = 4096;

/// One cycle of the source waveform, peak-normalised to 1. `noise` is a
/// fresh random cycle per voice, so it buzzes at the note's pitch.
pub fn source_cycle(source: GrainSource, rng: &mut impl Rng) -> Vec<f32> {
    let mut cycle = vec![0.0f32; SOURCE_SAMPLES];
    for (i, s) in cycle.iter_mut().enumerate() {
        let phase = i as f32 / SOURCE_SAMPLES as f32;
        let h = |n: f32| (TAU * n * phase).sin();
        *s = match source {
            GrainSource::Harmonics => 0.5 * h(1.0) + 0.25 * h(2.0) + 0.15 * h(3.0) + 0.1 * h(5.0),
            GrainSource::Noise => rng.random::<f32>() * 2.0 - 1.0,
            GrainSource::Formant => {
                0.3 * h(1.0) + 0.5 * h(3.0) + 0.4 * h(5.0) + 0.2 * h(7.0) + 0.1 * h(11.0)
            }
            GrainSource::Inharmonic => 0.4 * h(1.0) + 0.3 * h(1.41) + 0.2 * h(2.76) + 0.1 * h(3.53),
        };
    }
    let peak = cycle.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        for s in &mut cycle {
            *s /= peak;
        }
    }
    cycle
}

#[derive(Clone, Copy, Default)]
struct Grain {
    active: bool,
    /// Read position in the source cycle, in cycles (fractional).
    pos: f32,
    /// Cycles advanced per sample.
    pos_inc: f32,
    /// 0..1 across the grain's life.
    life: f32,
    life_inc: f32,
    left_gain: f32,
    right_gain: f32,
}

pub struct GranularVoice {
    cfg: Granular,
    sample_rate: f32,
    frequency: f32,
    source: Vec<f32>,
    grains: [Grain; MAX_GRAINS],
    /// Samples since the last grain started.
    since_spawn: f32,
    env: GateEnvelope,
    rng: rand::rngs::ThreadRng,
}

impl GranularVoice {
    pub fn new(cfg: &Granular, frequency: f32, sample_rate: f32) -> Self {
        let mut rng = rand::rng();
        let source = source_cycle(cfg.source, &mut rng);
        Self {
            cfg: cfg.clone(),
            sample_rate,
            frequency,
            source,
            grains: [Grain::default(); MAX_GRAINS],
            // Start with a grain due immediately so the attack is not delayed.
            since_spawn: f32::MAX,
            env: GateEnvelope::new(&cfg.env, sample_rate),
            rng,
        }
    }

    fn spawn(&mut self, pitch_ratio: f32) {
        let Some(slot) = self.grains.iter().position(|g| !g.active) else {
            return;
        };
        let grain_samples = (self.cfg.grain_ms.max(1.0) * 0.001 * self.sample_rate).max(1.0);
        let semitone_ratio = 2f32.powf(self.cfg.pitch_semitones / 12.0);
        let pan = 0.5 + (self.rng.random::<f32>() - 0.5) * self.cfg.stereo_width;
        let (left_gain, right_gain) = if self.cfg.stereo_width == 0.0 {
            let g = (0.5 * FRAC_PI_2).cos();
            (g, g)
        } else {
            ((pan * FRAC_PI_2).cos(), (pan * FRAC_PI_2).sin())
        };
        self.grains[slot] = Grain {
            active: true,
            pos: self.cfg.randomness * self.rng.random::<f32>(),
            pos_inc: self.frequency * semitone_ratio * pitch_ratio / self.sample_rate,
            life: 0.0,
            life_inc: 1.0 / grain_samples,
            left_gain,
            right_gain,
        };
    }
}

impl Voice for GranularVoice {
    fn gate_off(&mut self) {
        self.env.gate_off();
    }

    fn is_active(&self) -> bool {
        self.env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let env = self.env.next();
        if !self.env.is_active() {
            return (0.0, 0.0);
        }
        let density = (self.cfg.density * mods.grain_density_ratio).max(0.01);
        let interval = self.sample_rate / density;
        if self.since_spawn >= interval {
            self.since_spawn = 0.0;
            self.spawn(mods.pitch_ratio);
        }
        self.since_spawn += 1.0;

        let (mut l, mut r) = (0.0f32, 0.0f32);
        let mut active = 0usize;
        for g in &mut self.grains {
            if !g.active {
                continue;
            }
            let window = 0.5 * (1.0 - (TAU * g.life).cos());
            let s = {
                let p = g.pos.rem_euclid(1.0) * SOURCE_SAMPLES as f32;
                let i0 = p as usize % SOURCE_SAMPLES;
                let i1 = (i0 + 1) % SOURCE_SAMPLES;
                let frac = p - p.floor();
                self.source[i0] * (1.0 - frac) + self.source[i1] * frac
            } * window;
            l += s * g.left_gain;
            r += s * g.right_gain;
            active += 1;
            g.pos += g.pos_inc;
            g.life += g.life_inc;
            if g.life >= 1.0 {
                g.active = false;
            }
        }
        if active > 1 {
            let norm = 1.0 / (active as f32).sqrt();
            l *= norm;
            r *= norm;
        }
        let gain = env * self.cfg.level * mods.amplitude;
        (l * gain, r * gain)
    }
}
