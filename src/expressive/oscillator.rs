//! Phase accumulation and band-limited basic waveforms shared by the engines.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

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
    pub fn next_unit(&mut self, freq: f32) -> f32 {
        self.next_phase(freq) / TAU
    }
}

/// Polynomial band-limited step correction for saw/square discontinuities.
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
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

/// Basic oscillator shapes. `noise` ignores pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Wave {
    Sine,
    #[default]
    Saw,
    Square,
    Triangle,
    Noise,
}

/// One sample of `wave` at unit phase `phase` (0..1). `dt` is the phase
/// increment per sample (`freq / sample_rate`) for the PolyBLEP correction.
#[inline]
#[allow(dead_code)]
pub fn wave_sample(wave: Wave, phase: f32, dt: f32, pulse_width: f32, rng: &mut impl Rng) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Saw => 2.0 * phase - 1.0 - poly_blep(phase, dt),
        Wave::Square => {
            let pw = pulse_width.clamp(0.05, 0.95);
            let naive = if phase < pw { 1.0 } else { -1.0 };
            naive + poly_blep(phase, dt) - poly_blep((phase + 1.0 - pw).rem_euclid(1.0), dt)
        }
        Wave::Triangle => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }
        Wave::Noise => rng.random::<f32>() * 2.0 - 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn render(wave: Wave, freq: f32, seconds: f32) -> Vec<f32> {
        let mut acc = PhaseAccumulator::new(SR);
        let mut rng = rand::rng();
        let dt = freq / SR;
        (0..(seconds * SR) as usize)
            .map(|_| wave_sample(wave, acc.next_unit(freq), dt, 0.5, &mut rng))
            .collect()
    }

    #[test]
    fn every_pitched_wave_has_the_requested_fundamental() {
        for wave in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle] {
            let s = render(wave, 220.0, 1.0);
            let rate = zero_crossing_rate(&s, SR);
            assert!((rate - 440.0).abs() < 10.0, "{wave:?}: {rate} crossings/s");
        }
    }

    #[test]
    fn square_is_odd_harmonics_only() {
        let s = render(Wave::Square, 200.0, 1.0);
        let odd = goertzel_power(&s, 600.0, SR);
        let even = goertzel_power(&s, 400.0, SR);
        assert!(db(odd / even) > 20.0, "odd/even = {} dB", db(odd / even));
    }

    #[test]
    fn polyblep_saw_aliases_less_than_a_naive_saw() {
        let freq = 4000.0;
        let s = render(Wave::Saw, freq, 1.0);
        let mut acc = PhaseAccumulator::new(SR);
        let naive: Vec<f32> = (0..SR as usize)
            .map(|_| 2.0 * acc.next_unit(freq) - 1.0)
            .collect();
        // 44100 - 12000 = 32100 -> aliases to 12000 - (32100 - 22050) ... measure a
        // known alias: the 6th harmonic (24000) folds to 20100.
        let alias = 20100.0;
        assert!(
            goertzel_power(&s, alias, SR) < goertzel_power(&naive, alias, SR) * 0.5,
            "PolyBLEP should reduce the folded harmonic"
        );
    }

    #[test]
    fn noise_is_broadband_and_bounded() {
        let s = render(Wave::Noise, 440.0, 0.5);
        assert!(s.iter().all(|x| x.abs() <= 1.0));
        // A single Goertzel bin of white noise has high variance; average over
        // several bins per band so the comparison is stable run to run.
        let avg_power = |center: f32| -> f32 {
            let bins: Vec<f32> = (0..10).map(|i| center + i as f32 * 100.0).collect();
            bins.iter().map(|&f| goertzel_power(&s, f, SR)).sum::<f32>() / bins.len() as f32
        };
        let low = avg_power(500.0);
        let high = avg_power(9000.0);
        assert!(db(low / high).abs() < 15.0, "white noise is roughly flat");
    }

    #[test]
    fn wave_names_are_snake_case() {
        let w: Wave = serde_json::from_str("\"saw\"").unwrap();
        assert_eq!(w, Wave::Saw);
        assert!(serde_json::from_str::<Wave>("\"Sawtooth\"").is_err());
    }
}
