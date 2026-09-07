//! Gate-driven ADSR: attack, decay and sustain while the gate is open,
//! release once it closes. Segments are linear.

use serde::{Deserialize, Serialize};

/// Envelope times in seconds (0 to 10) and sustain 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
#[allow(dead_code)]
pub struct Adsr {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.3,
        }
    }
}

impl Adsr {
    /// Field-path error for an out-of-range value; `path` is e.g. "subtractive.env".
    #[allow(dead_code)]
    pub fn validate(&self, path: &str) -> Result<(), String> {
        for (name, value) in [
            ("attack", self.attack),
            ("decay", self.decay),
            ("release", self.release),
        ] {
            if !(0.0..=10.0).contains(&value) {
                return Err(format!(
                    "{path}.{name} must be 0 to 10 seconds, got {value}"
                ));
            }
        }
        if !(0.0..=1.0).contains(&self.sustain) {
            return Err(format!(
                "{path}.sustain must be 0 to 1, got {}",
                self.sustain
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Idle,
}

/// Shortest segment we step through, so a zero-length stage completes in one sample.
#[allow(dead_code)]
const MIN_SEGMENT_SECONDS: f32 = 0.0005;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GateEnvelope {
    attack_inc: f32,
    decay_dec: f32,
    sustain: f32,
    release_samples: f32,
    stage: Stage,
    level: f32,
    release_from: f32,
    release_pos: f32,
}

impl GateEnvelope {
    #[allow(dead_code)]
    pub fn new(params: &Adsr, sample_rate: f32) -> Self {
        let seg = |seconds: f32| seconds.max(MIN_SEGMENT_SECONDS) * sample_rate;
        let sustain = params.sustain.clamp(0.0, 1.0);
        Self {
            attack_inc: 1.0 / seg(params.attack),
            decay_dec: (1.0 - sustain) / seg(params.decay),
            sustain,
            release_samples: seg(params.release),
            stage: Stage::Attack,
            level: 0.0,
            release_from: 0.0,
            release_pos: 0.0,
        }
    }

    #[allow(dead_code)]
    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.release_from = self.level;
            self.release_pos = 0.0;
            self.stage = Stage::Release;
        }
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    #[inline]
    #[allow(dead_code)]
    pub fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= self.decay_dec;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.release_pos += 1.0;
                self.level = self.release_from * (1.0 - self.release_pos / self.release_samples);
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {}
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 1000.0;

    fn env(a: f32, d: f32, s: f32, r: f32) -> GateEnvelope {
        GateEnvelope::new(
            &Adsr {
                attack: a,
                decay: d,
                sustain: s,
                release: r,
            },
            SR,
        )
    }

    #[test]
    fn default_adsr_matches_the_spec() {
        let d = Adsr::default();
        assert_eq!(
            (d.attack, d.decay, d.sustain, d.release),
            (0.01, 0.1, 0.8, 0.3)
        );
    }

    #[test]
    fn attack_reaches_full_level_then_decays_to_sustain() {
        let mut e = env(0.1, 0.1, 0.5, 0.1);
        let attack: Vec<f32> = (0..100).map(|_| e.next()).collect();
        assert!(attack[0] < 0.05, "starts near zero: {}", attack[0]);
        assert!(
            (attack[99] - 1.0).abs() < 0.02,
            "peaks at 1: {}",
            attack[99]
        );
        for _ in 0..100 {
            e.next();
        }
        let sustain = e.next();
        assert!((sustain - 0.5).abs() < 0.02, "sustains at 0.5: {}", sustain);
        assert!(e.is_active());
    }

    #[test]
    fn release_runs_from_the_current_level_and_then_deactivates() {
        let mut e = env(0.01, 0.01, 0.6, 0.1);
        for _ in 0..50 {
            e.next();
        }
        e.gate_off();
        let first = e.next();
        assert!(
            first < 0.6 && first > 0.55,
            "release starts from sustain: {first}"
        );
        for _ in 0..100 {
            e.next();
        }
        assert_eq!(e.next(), 0.0);
        assert!(!e.is_active());
    }

    #[test]
    fn gate_off_during_attack_releases_from_the_partial_level() {
        let mut e = env(1.0, 0.1, 0.8, 0.1);
        for _ in 0..500 {
            e.next();
        }
        e.gate_off();
        let level = e.next();
        assert!(level < 0.5 && level > 0.45, "released from ~0.5: {level}");
    }

    #[test]
    fn zero_length_segments_do_not_divide_by_zero() {
        let mut e = env(0.0, 0.0, 1.0, 0.0);
        assert!((e.next() - 1.0).abs() < 0.01);
        e.gate_off();
        e.next();
        assert!(!e.is_active());
    }
}
