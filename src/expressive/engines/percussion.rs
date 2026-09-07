//! A percussive hit pre-rendered at note-on; ignores gate-off.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{Percussion, percussion};

/// Shortest hit a percussion voice renders regardless of gate.
pub const MIN_HIT_SECONDS: f32 = 0.05;

pub struct PercussionVoice {
    samples: Vec<f32>,
    pos: usize,
    level: f32,
}

impl PercussionVoice {
    /// Renders `gate_seconds` of the hit (the hit's own envelope shapes it).
    pub fn new(cfg: &Percussion, gate_seconds: f32, sample_rate: f32) -> Self {
        let count = ((gate_seconds.max(MIN_HIT_SECONDS)) * sample_rate) as usize;
        Self {
            samples: percussion::render(sample_rate, cfg, count),
            pos: 0,
            level: cfg.level,
        }
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
}
