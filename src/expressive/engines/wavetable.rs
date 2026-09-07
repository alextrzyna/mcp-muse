//! Plays one band-limited wavetable (or a morph between two neighbours)
//! through an amplitude envelope.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::wavetables::{mip_level, sample};
use crate::expressive::{GateEnvelope, TableName, Wavetable};

pub struct WavetableVoice {
    level: f32,
    table: TableName,
    next: TableName,
    morph: f32,
    mip: usize,
    frequency: f32,
    phase: PhaseAccumulator,
    env: GateEnvelope,
}

impl WavetableVoice {
    pub fn new(cfg: &Wavetable, frequency: f32, sample_rate: f32) -> Self {
        Self {
            level: cfg.level,
            table: cfg.table,
            next: cfg.table.next(),
            morph: cfg.morph.clamp(0.0, 1.0),
            // The level is chosen from the note's pitch; LFO vibrato is far
            // smaller than an octave, so it stays valid for the whole note.
            mip: mip_level(frequency),
            frequency,
            phase: PhaseAccumulator::new(sample_rate),
            env: GateEnvelope::new(&cfg.env, sample_rate),
        }
    }
}

impl Voice for WavetableVoice {
    fn gate_off(&mut self) {
        self.env.gate_off();
    }

    fn is_active(&self) -> bool {
        self.env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let p = self.phase.next_unit(self.frequency * mods.pitch_ratio);
        let a = sample(self.table, self.mip, p);
        let s = if self.morph > 0.0 {
            let b = sample(self.next, self.mip, p);
            a * (1.0 - self.morph) + b * self.morph
        } else {
            a
        };
        let out = s * self.env.next() * self.level * mods.amplitude;
        (out, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::Adsr;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn cfg(table: TableName, morph: f32) -> Wavetable {
        Wavetable {
            level: 1.0,
            table,
            morph,
            env: fast(),
        }
    }

    fn render(c: &Wavetable, freq: f32, seconds: f32) -> Vec<f32> {
        let mut v = WavetableVoice::new(c, freq, SR);
        let mods = Modulation::default();
        (0..(seconds * SR) as usize)
            .map(|_| v.tick(&mods).0)
            .collect()
    }

    #[test]
    fn plays_at_the_note_frequency() {
        let s = render(&cfg(TableName::Basic, 0.0), 330.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 660.0).abs() < 12.0);
        assert!(rms(&s[441..]) > 0.3);
    }

    #[test]
    fn morph_moves_the_spectrum_between_neighbouring_tables() {
        // basic (weak 3rd harmonic) -> warm (strong 3rd harmonic).
        let h3 = |m: f32| {
            let s = render(&cfg(TableName::Basic, m), 220.0, 1.0);
            db(goertzel_power(&s, 660.0, SR) / goertzel_power(&s, 220.0, SR))
        };
        let (a, b, c) = (h3(0.0), h3(0.5), h3(1.0));
        assert!(a < b && b < c, "3rd harmonic grows with morph: {a} {b} {c}");
    }

    #[test]
    fn morph_wraps_from_noise_to_basic() {
        let noise_to_basic = render(&cfg(TableName::Noise, 1.0), 220.0, 1.0);
        let basic = render(&cfg(TableName::Basic, 0.0), 220.0, 1.0);
        let h20 = |s: &[f32]| goertzel_power(s, 220.0 * 20.0, SR);
        assert!(
            db(h20(&noise_to_basic) / h20(&basic)).abs() < 3.0,
            "fully morphed = next table"
        );
    }

    #[test]
    fn high_notes_use_a_band_limited_level() {
        let s = render(&cfg(TableName::Bright, 0.0), 5000.0, 0.5);
        // 6th harmonic (30 kHz) would fold to 14.1 kHz; the level chosen at 5 kHz excludes it.
        let alias = goertzel_power(&s, 14100.0, SR);
        let fundamental = goertzel_power(&s, 5000.0, SR);
        assert!(db(alias / fundamental) < -30.0);
    }

    #[test]
    fn release_then_silence_and_modulation_apply() {
        let mut c = cfg(TableName::Basic, 0.0);
        c.env.release = 0.4;
        c.level = 0.5;
        let mut v = WavetableVoice::new(&c, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
            ..Modulation::default()
        };
        let s: Vec<f32> = (0..22050).map(|_| v.tick(&mods).0).collect();
        // The organ table's extra harmonics push the zero-crossing rate to
        // ~1758 Hz here, far outside a loose ±40 Hz bound around 880 Hz (see
        // task-5-report.md); the basic (near-sinusoidal) table keeps this
        // assertion meaningful as a pitch-tracking check.
        assert!(
            (zero_crossing_rate(&s, SR) - 880.0).abs() < 40.0,
            "extra crossings; loose bound"
        );
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            peak <= 0.26 && peak > 0.15,
            "level x amplitude = 0.25 on a peak-1 table: {peak}"
        );
        v.gate_off();
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
    }
}
