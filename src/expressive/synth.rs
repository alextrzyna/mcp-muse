//! R2D2 vocalization voice.
//!
//! Renders a Ben Burtt-style ring-modulated vocalization to a sample buffer
//! (`Vec<f32>` at 44.1 kHz); the translator then schedules that buffer.
//! Musical notes go through the patch engines in `render.rs` instead.

use crate::expressive::oscillator::PhaseAccumulator;
use std::f32::consts::TAU;

/// Core expressive synthesizer for R2D2-style vocalizations.
pub struct ExpressiveSynth {
    sample_rate: f32,
}

impl Default for ExpressiveSynth {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpressiveSynth {
    pub const SAMPLE_RATE: f32 = 44100.0;

    pub fn new() -> Self {
        ExpressiveSynth {
            sample_rate: Self::SAMPLE_RATE,
        }
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
    use crate::expressive::test_util::zero_crossing_rate;

    const SR: f32 = 44100.0;

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
}
