//! Low-frequency oscillator: a -1..1 control signal in one of five shapes,
//! advanced once per sample and mapped onto a `Modulation` by the renderer.
// Not yet wired into the renderer (that's the next PR task), so this whole
// module is dead code from `cargo build`'s point of view for now.
#![allow(dead_code)]

use crate::expressive::LfoWave;
use rand::RngExt;
use std::f32::consts::TAU;

pub struct Lfo {
    wave: LfoWave,
    /// Cycles per sample, accumulated in `f64` so a rate like 1000 Hz at a
    /// 1000 Hz sample rate wraps on the exact sample instead of drifting.
    increment: f64,
    /// Unit phase 0..1.
    phase: f64,
    held: f32,
    rng: rand::rngs::ThreadRng,
}

impl Lfo {
    pub fn new(rate: f32, wave: LfoWave, sample_rate: f32) -> Self {
        let mut rng = rand::rng();
        let held = rng.random::<f32>() * 2.0 - 1.0;
        Self {
            wave,
            increment: rate.max(0.0) as f64 / sample_rate as f64,
            phase: 0.0,
            held,
            rng,
        }
    }

    /// The value for the current sample, then advance one sample.
    #[inline]
    pub fn next(&mut self) -> f32 {
        let p = self.phase as f32;
        let value = match self.wave {
            LfoWave::Sine => (p * TAU).sin(),
            LfoWave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            LfoWave::Saw => 2.0 * p - 1.0,
            LfoWave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWave::SampleHold => self.held,
        };
        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.held = self.rng.random::<f32>() * 2.0 - 1.0;
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::zero_crossing_rate;

    const SR: f32 = 1000.0;

    fn run(wave: LfoWave, rate: f32, seconds: f32) -> Vec<f32> {
        let mut lfo = Lfo::new(rate, wave, SR);
        (0..(seconds * SR) as usize).map(|_| lfo.next()).collect()
    }

    #[test]
    fn every_shape_stays_within_minus_one_to_one() {
        for wave in LfoWave::ALL {
            let s = run(wave, 3.0, 2.0);
            assert!(s.iter().all(|x| (-1.0..=1.0).contains(x)), "{wave:?}");
        }
    }

    #[test]
    fn sine_and_triangle_cross_zero_at_twice_the_rate() {
        for wave in [LfoWave::Sine, LfoWave::Triangle] {
            let s = run(wave, 2.0, 5.0);
            assert!((zero_crossing_rate(&s, SR) - 4.0).abs() < 0.5, "{wave:?}");
        }
    }

    #[test]
    fn saw_ramps_up_and_resets_once_per_cycle() {
        let s = run(LfoWave::Saw, 1.0, 2.0);
        // With rate 1 Hz and SR 1000, one cycle is 1000 samples: the ramp
        // reaches +1 just before it wraps (sample 999), not at the midpoint.
        assert!(
            s[0] < -0.99 && s[999] > 0.99,
            "ramps from -1 to +1 over one second"
        );
        assert!(s[1000] < -0.99, "resets at the cycle boundary");
    }

    #[test]
    fn square_holds_plus_one_then_minus_one() {
        let s = run(LfoWave::Square, 1.0, 1.0);
        assert!(s[..500].iter().all(|&x| x == 1.0));
        assert!(s[500..].iter().all(|&x| x == -1.0));
    }

    #[test]
    fn sample_hold_holds_one_value_per_cycle_and_changes_between_cycles() {
        let s = run(LfoWave::SampleHold, 2.0, 5.0);
        for cycle in 0..10 {
            let c = &s[cycle * 500..(cycle + 1) * 500];
            assert!(c.iter().all(|&x| x == c[0]), "held within cycle {cycle}");
        }
        let firsts: Vec<f32> = (0..10).map(|c| s[c * 500]).collect();
        assert!(
            firsts.windows(2).any(|w| w[0] != w[1]),
            "changes between cycles"
        );
    }
}
