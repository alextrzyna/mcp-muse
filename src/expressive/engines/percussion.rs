//! A percussive hit pre-rendered at note-on; ignores gate-off.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{Percussion, percussion};

/// Shortest hit a percussion voice renders regardless of gate.
pub const MIN_HIT_SECONDS: f32 = 0.05;

/// Raised-cosine fade applied to the end of a rendered hit. The buffer is cut
/// at the gate, so without it a still-ringing cymbal stops dead and clicks.
pub const HIT_FADE_SECONDS: f32 = 0.005;

/// Every hit is normalised to this peak before `level` applies, so `level`
/// means the same loudness for a kick as for a cymbal.
pub const PERCUSSION_PEAK: f32 = 0.9;

/// Scales `samples` in place so its absolute peak becomes `target`. A silent
/// buffer (peak near zero) is left untouched.
fn normalise_peak(samples: &mut [f32], target: f32) {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 1e-6 {
        let g = target / peak;
        for s in samples.iter_mut() {
            *s *= g;
        }
    }
}

pub struct PercussionVoice {
    samples: Vec<f32>,
    pos: usize,
    level: f32,
}

impl PercussionVoice {
    /// Renders `gate_seconds` of the hit (the hit's own envelope shapes it).
    pub fn new(cfg: &Percussion, gate_seconds: f32, sample_rate: f32) -> Self {
        let count = ((gate_seconds.max(MIN_HIT_SECONDS)) * sample_rate) as usize;
        let mut samples = percussion::render(sample_rate, cfg, count);
        // Fade first, then normalise the faded buffer: some kinds (e.g. a
        // rising `swoosh`) peak in the last few milliseconds, which is
        // exactly the region the fade attenuates. Normalising afterwards
        // guarantees the *played* peak lands on PERCUSSION_PEAK regardless
        // of where in the envelope the loudest sample falls.
        fade_out(&mut samples, (HIT_FADE_SECONDS * sample_rate) as usize);
        normalise_peak(&mut samples, PERCUSSION_PEAK);
        Self {
            samples,
            pos: 0,
            level: cfg.level,
        }
    }
}

/// Raised-cosine fade over the last `fade` samples, so a hit that is still
/// ringing when the buffer ends does not click.
fn fade_out(samples: &mut [f32], fade: usize) {
    if fade == 0 || samples.len() <= fade {
        return;
    }
    let start = samples.len() - fade;
    for (i, s) in samples[start..].iter_mut().enumerate() {
        let x = (i + 1) as f32 / fade as f32;
        *s *= 0.5 * (1.0 + (std::f32::consts::PI * x).cos());
    }
}

impl Voice for PercussionVoice {
    fn gate_off(&mut self) {}

    fn is_active(&self) -> bool {
        self.pos < self.samples.len()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let s = self.samples.get(self.pos).copied().unwrap_or(0.0) * self.level * mods.amplitude;
        self.pos += 1;
        (s, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::PercussionKind;

    #[test]
    fn plays_out_regardless_of_gate_off_and_then_deactivates() {
        let cfg: Percussion = serde_json::from_str(r#"{"kind": "kick", "level": 0.5}"#).unwrap();
        let mut v = PercussionVoice::new(&cfg, 0.2, 44100.0);
        assert_eq!(cfg.kind, PercussionKind::Kick);
        v.gate_off();
        let mods = Modulation::default();
        let mut peak = 0.0f32;
        for _ in 0..(0.2 * 44100.0) as usize {
            assert!(v.is_active());
            peak = peak.max(v.tick(&mods).0.abs());
        }
        assert!(!v.is_active());
        assert!(
            peak > 0.1 && peak <= 0.5 * PERCUSSION_PEAK + 1e-4,
            "level applied: {peak}"
        );
    }

    #[test]
    fn every_kind_is_normalised_to_the_same_peak() {
        for kind in [
            "kick", "snare", "hihat", "cymbal", "zap", "swoosh", "chime", "burst",
        ] {
            let cfg: Percussion =
                serde_json::from_str(&format!(r#"{{"kind": "{kind}", "level": 1.0}}"#)).unwrap();
            let mut v = PercussionVoice::new(&cfg, 0.5, 44100.0);
            let mods = Modulation::default();
            let mut peak = 0.0f32;
            while v.is_active() {
                peak = peak.max(v.tick(&mods).0.abs());
            }
            assert!(
                (peak - PERCUSSION_PEAK).abs() < 0.02,
                "{kind} peaks at {peak}"
            );
        }
    }

    #[test]
    fn a_hit_fades_out_instead_of_stopping_dead() {
        use crate::expressive::test_util::rms;
        const SR: f32 = 44100.0;
        let cfg: Percussion = serde_json::from_str(r#"{"kind": "cymbal"}"#).unwrap();
        let v = PercussionVoice::new(&cfg, 0.3, SR);
        let s = &v.samples;
        let fade = (HIT_FADE_SECONDS * SR) as usize;
        assert!(
            s.last().unwrap().abs() < 1e-3,
            "ends at zero, got {}",
            s.last().unwrap()
        );
        assert!(
            rms(&s[s.len() - fade..]) < rms(&s[s.len() - 2 * fade..s.len() - fade]),
            "the last 5 ms is quieter than the 5 ms before it"
        );
    }
}
