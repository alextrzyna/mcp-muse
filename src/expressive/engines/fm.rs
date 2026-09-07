//! Four-operator FM. Each operator is a sine with its own envelope; the
//! algorithm decides which operators modulate which, and which are heard.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::{Fm, GateEnvelope};

/// Peak phase deviation in radians when a modulator's level and envelope are both 1.
pub const FM_MOD_DEPTH: f32 = 4.0;

const MAX_OPERATORS: usize = 4;

pub struct FmVoice {
    cfg: Fm,
    /// `cfg.operators.len()`, clamped to `MAX_OPERATORS`; operators beyond
    /// this (an out-of-contract patch bypassing `Fm::validate`) are ignored
    /// rather than indexed.
    count: usize,
    frequency: f32,
    phases: [PhaseAccumulator; MAX_OPERATORS],
    envs: Vec<GateEnvelope>,
    /// Previous-sample output of each operator, for feedback.
    last_out: [f32; MAX_OPERATORS],
    /// 1/sqrt(number of carriers that exist), so layering carriers does not clip.
    carrier_norm: f32,
}

impl FmVoice {
    pub fn new(cfg: &Fm, frequency: f32, sample_rate: f32) -> Self {
        let count = cfg.operators.len().min(MAX_OPERATORS);
        let mut cfg = cfg.clone();
        cfg.operators.truncate(count);
        let envs = cfg
            .operators
            .iter()
            .map(|op| GateEnvelope::new(&op.env, sample_rate))
            .collect();
        let carriers = cfg
            .algorithm
            .carriers()
            .iter()
            .filter(|&&c| c < count)
            .count()
            .max(1);
        Self {
            cfg,
            count,
            frequency,
            phases: [PhaseAccumulator::new(sample_rate); MAX_OPERATORS],
            envs,
            last_out: [0.0; MAX_OPERATORS],
            carrier_norm: 1.0 / (carriers as f32).sqrt(),
        }
    }
}

impl Voice for FmVoice {
    fn gate_off(&mut self) {
        for env in &mut self.envs {
            env.gate_off();
        }
    }

    fn is_active(&self) -> bool {
        self.cfg
            .algorithm
            .carriers()
            .iter()
            .filter_map(|&c| self.envs.get(c))
            .any(|e| e.is_active())
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let base = self.frequency * mods.pitch_ratio;
        let count = self.count;
        let last = count.saturating_sub(1);
        let mut out = [0.0f32; MAX_OPERATORS];

        // Modulators always have higher indices than what they modulate, so a
        // reverse pass has every modulator ready when its carrier needs it.
        for i in (0..count).rev() {
            let op = &self.cfg.operators[i];
            let freq = base * op.ratio * 2f32.powf(op.detune_cents / 1200.0);
            let mut phase = self.phases[i].next_phase(freq);
            for &m in self.cfg.algorithm.modulators(i) {
                if m < count {
                    phase += out[m] * FM_MOD_DEPTH;
                }
            }
            if i == last && self.cfg.feedback > 0.0 {
                phase += self.last_out[i] * self.cfg.feedback;
            }
            let env = self.envs[i].next();
            out[i] = phase.sin() * op.level * env;
        }
        self.last_out = out;

        let sum: f32 = self
            .cfg
            .algorithm
            .carriers()
            .iter()
            .filter(|&&c| c < count)
            .map(|&c| out[c])
            .sum();
        let s = sum * self.carrier_norm * self.cfg.level * mods.amplitude;
        (s, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};
    use crate::expressive::{Adsr, FmAlgorithm, Operator};

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn op(ratio: f32, level: f32) -> Operator {
        Operator {
            ratio,
            level,
            detune_cents: 0.0,
            env: fast(),
        }
    }

    fn render(cfg: &Fm, freq: f32, gate: f32, total: f32) -> Vec<f32> {
        let mut v = FmVoice::new(cfg, freq, SR);
        let mods = Modulation::default();
        let gate_end = (gate * SR) as usize;
        (0..(total * SR) as usize)
            .map(|i| {
                if i == gate_end {
                    v.gate_off();
                }
                v.tick(&mods).0
            })
            .collect()
    }

    fn fm(algorithm: FmAlgorithm, operators: Vec<Operator>) -> Fm {
        Fm {
            level: 1.0,
            algorithm,
            feedback: 0.0,
            operators,
        }
    }

    #[test]
    fn a_single_carrier_is_a_sine_at_the_note_frequency() {
        let s = render(&fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]), 220.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 440.0).abs() < 8.0);
        let h2 = goertzel_power(&s, 440.0, SR);
        let f = goertzel_power(&s, 220.0, SR);
        assert!(db(h2 / f) < -40.0, "no harmonics without a modulator");
    }

    #[test]
    fn a_modulator_creates_sidebands_at_carrier_plus_ratio_multiples() {
        // Carrier 220 Hz, modulator at ratio 2 (440 Hz): sidebands at 660, 1100, ...
        let plain = render(&fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]), 220.0, 1.0, 1.0);
        let modulated = render(
            &fm(FmAlgorithm::Stack, vec![op(1.0, 1.0), op(2.0, 0.5)]),
            220.0,
            1.0,
            1.0,
        );
        let side = goertzel_power(&modulated, 660.0, SR);
        assert!(
            db(side / goertzel_power(&plain, 660.0, SR)) > 30.0,
            "660 Hz sideband appears"
        );
    }

    #[test]
    fn parallel_operators_are_pure_partials_without_sidebands() {
        let s = render(
            &fm(FmAlgorithm::Parallel, vec![op(1.0, 1.0), op(2.0, 1.0)]),
            220.0,
            1.0,
            1.0,
        );
        let f = goertzel_power(&s, 220.0, SR);
        let h2 = goertzel_power(&s, 440.0, SR);
        let h3 = goertzel_power(&s, 660.0, SR);
        assert!(
            db(h2 / f).abs() < 3.0,
            "both partials present at similar level"
        );
        assert!(db(h3 / f) < -30.0, "no sideband at 660 Hz");
    }

    #[test]
    fn pairs_has_two_carriers_and_fan_in_has_one() {
        // pairs: op3 (idx2) modulates op1; op2 (idx1) is a second carrier at ratio 1.5.
        let pairs = render(
            &fm(
                FmAlgorithm::Pairs,
                vec![op(1.0, 1.0), op(1.5, 1.0), op(2.0, 0.4)],
            ),
            200.0,
            1.0,
            1.0,
        );
        assert!(
            db(goertzel_power(&pairs, 300.0, SR) / goertzel_power(&pairs, 200.0, SR)) > -6.0,
            "second carrier at 300 Hz is heard"
        );
        // fan_in: op2 at ratio 1.5 is a modulator, not a carrier; 300 Hz appears only as a weak sideband.
        let fan = render(
            &fm(
                FmAlgorithm::FanIn,
                vec![op(1.0, 1.0), op(1.5, 0.1), op(2.0, 0.1)],
            ),
            200.0,
            1.0,
            1.0,
        );
        assert!(
            db(goertzel_power(&fan, 300.0, SR) / goertzel_power(&fan, 200.0, SR)) < -10.0,
            "ratio-1.5 operator is not heard directly in fan_in"
        );
    }

    #[test]
    fn modulator_envelope_shapes_the_timbre_over_time() {
        let mut modulator = op(3.0, 0.8);
        modulator.env = Adsr {
            attack: 0.001,
            decay: 0.3,
            sustain: 0.0,
            release: 0.1,
        };
        let s = render(
            &fm(FmAlgorithm::Stack, vec![op(1.0, 1.0), modulator]),
            220.0,
            1.5,
            1.5,
        );
        let early = goertzel_power(&s[..4410], 880.0, SR);
        let late = goertzel_power(&s[44100..48510], 880.0, SR);
        assert!(
            db(early / late) > 15.0,
            "sideband fades as the modulator decays"
        );
    }

    #[test]
    fn feedback_adds_harmonics_to_the_last_operator() {
        let mut cfg = fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]);
        let clean = render(&cfg, 220.0, 1.0, 1.0);
        cfg.feedback = 0.8;
        let fed = render(&cfg, 220.0, 1.0, 1.0);
        assert!(
            db(goertzel_power(&fed, 440.0, SR) / goertzel_power(&clean, 440.0, SR)) > 20.0,
            "feedback creates a second harmonic"
        );
    }

    #[test]
    fn release_sounds_past_the_gate_and_then_stops() {
        let mut carrier = op(1.0, 1.0);
        carrier.env = Adsr {
            release: 0.5,
            ..fast()
        };
        let mut v = FmVoice::new(&fm(FmAlgorithm::Stack, vec![carrier]), 220.0, SR);
        let mods = Modulation::default();
        for _ in 0..4410 {
            v.tick(&mods);
        }
        v.gate_off();
        let after: Vec<f32> = (0..4410).map(|_| v.tick(&mods).0).collect();
        assert!(rms(&after) > 0.1);
        assert!(v.is_active());
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
    }

    #[test]
    fn level_pitch_ratio_and_amplitude_modulation_apply() {
        let mut cfg = fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]);
        cfg.level = 0.5;
        let mut v = FmVoice::new(&cfg, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
        };
        let s: Vec<f32> = (0..SR as usize).map(|_| v.tick(&mods).0).collect();
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            (peak - 0.25).abs() < 0.03,
            "level 0.5 x amplitude 0.5: {peak}"
        );
    }

    #[test]
    fn missing_operators_are_silent_not_a_panic() {
        // Stack with 4 carriers' worth of routing but only 1 operator supplied.
        let s = render(&fm(FmAlgorithm::FanIn, vec![op(1.0, 1.0)]), 220.0, 0.5, 0.5);
        assert!(s.iter().all(|x| x.is_finite()));
        assert!(rms(&s) > 0.3);
    }

    #[test]
    fn an_empty_operator_list_is_silent_not_a_panic() {
        let cfg = Fm {
            operators: vec![],
            ..Default::default()
        };
        let mut v = FmVoice::new(&cfg, 220.0, SR);
        let mods = Modulation::default();
        let s: Vec<f32> = (0..100).map(|_| v.tick(&mods).0).collect();
        assert!(s.iter().all(|x| *x == 0.0));
        assert!(!v.is_active());
    }

    #[test]
    fn more_than_four_operators_are_ignored() {
        // Stack only ever routes operators 0..4; the 5th and 6th entries here
        // must be ignored rather than panicking or being indexed. A pure
        // 220 Hz sine crosses zero twice per cycle (440/s); with weak
        // modulators the carrier still dominates that rate.
        let six = vec![
            op(1.0, 1.0),
            op(2.0, 0.05),
            op(3.0, 0.05),
            op(4.0, 0.05),
            op(5.0, 0.05),
            op(6.0, 0.05),
        ];
        let s = render(&fm(FmAlgorithm::Stack, six), 220.0, 1.0, 1.0);
        assert!(s.iter().all(|x| x.is_finite()));
        assert!(
            (zero_crossing_rate(&s, SR) - 440.0).abs() < 15.0,
            "dominated by the carrier"
        );
    }
}
