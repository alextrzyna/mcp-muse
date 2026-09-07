//! One `Voice` per note per enabled engine. The renderer sums voices, so a
//! voice only has to produce its own signal and report when it has finished.

use crate::expressive::LfoTarget;

pub mod fm;
pub mod percussion;
pub mod subtractive;
pub mod wavetable;

#[allow(unused_imports)]
pub use fm::{FM_MOD_DEPTH, FmVoice};
#[allow(unused_imports)]
pub use percussion::{MIN_HIT_SECONDS, PercussionVoice};
#[allow(unused_imports)]
pub use subtractive::SubtractiveVoice;
#[allow(unused_imports)]
pub use wavetable::WavetableVoice;

/// Per-sample modulation inputs shared by all voices; the renderer derives
/// them from the patch's LFO each sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modulation {
    pub pitch_ratio: f32,
    pub cutoff_ratio: f32,
    pub amplitude: f32,
    /// Added to a wavetable voice's morph position, then clamped to 0..1.
    pub morph_offset: f32,
    /// Multiplies a granular voice's grain rate.
    pub grain_density_ratio: f32,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            cutoff_ratio: 1.0,
            amplitude: 1.0,
            morph_offset: 0.0,
            grain_density_ratio: 1.0,
        }
    }
}

impl Modulation {
    /// Map an LFO sample (`value` in -1..1) at `depth` onto `target` with the
    /// spec's ranges: cutoff ±2 octaves, pitch ±2 semitones, amplitude down to
    /// silence, morph ±0.5, grain density 0.5x..2x.
    pub fn from_lfo(target: LfoTarget, depth: f32, value: f32) -> Self {
        let d = depth.clamp(0.0, 1.0);
        let v = value.clamp(-1.0, 1.0);
        let mut m = Self::default();
        if d == 0.0 {
            return m;
        }
        match target {
            LfoTarget::Off => {}
            LfoTarget::Cutoff => m.cutoff_ratio = 2f32.powf(2.0 * v * d),
            LfoTarget::Pitch => m.pitch_ratio = 2f32.powf(2.0 * v * d / 12.0),
            LfoTarget::Amplitude => m.amplitude = 1.0 - d * (1.0 - v) / 2.0,
            LfoTarget::Morph => m.morph_offset = v * d / 2.0,
            LfoTarget::GrainDensity => m.grain_density_ratio = 2f32.powf(v * d),
        }
        m
    }
}

#[allow(dead_code)]
pub trait Voice {
    /// The note ended; start the release (percussion ignores this).
    fn gate_off(&mut self);
    /// False once the voice has nothing more to output.
    fn is_active(&self) -> bool;
    /// One stereo sample, already scaled by the engine's `level`.
    fn tick(&mut self, mods: &Modulation) -> (f32, f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::LfoTarget;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn modulation_from_lfo_maps_each_target_with_the_spec_ranges() {
        let d = Modulation::default();
        assert!(close(d.morph_offset, 0.0) && close(d.grain_density_ratio, 1.0));

        let m = Modulation::from_lfo(LfoTarget::Cutoff, 1.0, 1.0);
        assert!(close(m.cutoff_ratio, 4.0), "two octaves up at full depth");
        let m = Modulation::from_lfo(LfoTarget::Cutoff, 1.0, -1.0);
        assert!(close(m.cutoff_ratio, 0.25));

        let m = Modulation::from_lfo(LfoTarget::Pitch, 1.0, 1.0);
        assert!(close(m.pitch_ratio, 2f32.powf(2.0 / 12.0)), "two semitones");
        let m = Modulation::from_lfo(LfoTarget::Pitch, 0.5, 1.0);
        assert!(close(m.pitch_ratio, 2f32.powf(1.0 / 12.0)));

        let m = Modulation::from_lfo(LfoTarget::Amplitude, 1.0, -1.0);
        assert!(close(m.amplitude, 0.0), "full tremolo reaches silence");
        let m = Modulation::from_lfo(LfoTarget::Amplitude, 1.0, 1.0);
        assert!(close(m.amplitude, 1.0));
        let m = Modulation::from_lfo(LfoTarget::Amplitude, 0.5, -1.0);
        assert!(close(m.amplitude, 0.5));

        let m = Modulation::from_lfo(LfoTarget::Morph, 1.0, 1.0);
        assert!(close(m.morph_offset, 0.5));
        let m = Modulation::from_lfo(LfoTarget::GrainDensity, 1.0, 1.0);
        assert!(close(m.grain_density_ratio, 2.0));
        let m = Modulation::from_lfo(LfoTarget::GrainDensity, 1.0, -1.0);
        assert!(close(m.grain_density_ratio, 0.5));

        let m = Modulation::from_lfo(LfoTarget::Off, 1.0, 1.0);
        assert_eq!(m, Modulation::default());
        let m = Modulation::from_lfo(LfoTarget::Pitch, 0.0, 1.0);
        assert_eq!(m, Modulation::default(), "zero depth is identity");
    }
}
