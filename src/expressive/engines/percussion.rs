//! A percussive hit pre-rendered at note-on; ignores gate-off.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{Percussion, percussion};

/// Shortest hit a percussion voice renders regardless of gate.
pub const MIN_HIT_SECONDS: f32 = 0.05;

/// Raised-cosine fade applied to the end of a rendered hit. The buffer is cut
/// at the gate, so without it a still-ringing cymbal stops dead and clicks.
pub const HIT_FADE_SECONDS: f32 = 0.005;

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
        fade_out(&mut samples, (HIT_FADE_SECONDS * sample_rate) as usize);
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
        assert!(peak > 0.1 && peak <= 0.5 * 1.5, "level applied: {peak}");
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
