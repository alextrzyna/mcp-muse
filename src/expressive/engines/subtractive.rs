//! Two oscillators, an optional 12/24 dB state-variable filter with its own
//! envelope, and an amplitude envelope.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::{PhaseAccumulator, wave_sample};
use crate::expressive::{FilterKind, GateEnvelope, Subtractive, Svf, SvfMode};

pub struct SubtractiveVoice {
    cfg: Subtractive,
    sample_rate: f32,
    frequency: f32,
    osc1: PhaseAccumulator,
    osc2: PhaseAccumulator,
    amp_env: GateEnvelope,
    filter_env: Option<GateEnvelope>,
    svf1: Option<Svf>,
    svf2: Option<Svf>,
    /// Cutoff the filters are already tuned to; recomputing the coefficients
    /// every sample is wasted work while the filter envelope sits still.
    last_cutoff: f32,
    rng: rand::rngs::ThreadRng,
}

fn svf_mode(kind: FilterKind) -> SvfMode {
    match kind {
        FilterKind::LowPass => SvfMode::LowPass,
        FilterKind::HighPass => SvfMode::HighPass,
        FilterKind::BandPass => SvfMode::BandPass,
    }
}

impl SubtractiveVoice {
    pub fn new(cfg: &Subtractive, frequency: f32, sample_rate: f32) -> Self {
        let (filter_env, svf1, svf2) = match &cfg.filter {
            Some(f) => {
                let q = 0.5 + f.resonance.clamp(0.0, 1.0) * 9.5;
                let make = || Svf::new(sample_rate, f.cutoff, q, svf_mode(f.kind));
                (
                    Some(GateEnvelope::new(&f.env, sample_rate)),
                    Some(make()),
                    (f.slope == 24).then(make),
                )
            }
            None => (None, None, None),
        };
        Self {
            cfg: cfg.clone(),
            sample_rate,
            frequency,
            osc1: PhaseAccumulator::new(sample_rate),
            osc2: PhaseAccumulator::new(sample_rate),
            amp_env: GateEnvelope::new(&cfg.env, sample_rate),
            filter_env,
            svf1,
            svf2,
            last_cutoff: cfg.filter.as_ref().map(|f| f.cutoff).unwrap_or(0.0),
            rng: rand::rng(),
        }
    }
}

impl Voice for SubtractiveVoice {
    fn gate_off(&mut self) {
        self.amp_env.gate_off();
        if let Some(env) = &mut self.filter_env {
            env.gate_off();
        }
    }

    fn is_active(&self) -> bool {
        self.amp_env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let freq = self.frequency * mods.pitch_ratio;
        let dt1 = freq / self.sample_rate;
        let p1 = self.osc1.next_unit(freq);
        let mut s = wave_sample(
            self.cfg.osc1.wave,
            p1,
            dt1,
            self.cfg.osc1.pulse_width,
            &mut self.rng,
        );

        if let Some(o2) = &self.cfg.osc2
            && o2.mix > 0.0
        {
            let f2 = freq * 2f32.powi(o2.octave as i32) * 2f32.powf(o2.detune_cents / 1200.0);
            let p2 = self.osc2.next_unit(f2);
            let s2 = wave_sample(
                o2.wave,
                p2,
                f2 / self.sample_rate,
                o2.pulse_width,
                &mut self.rng,
            );
            s = s * (1.0 - o2.mix) + s2 * o2.mix;
        }

        if let (Some(f), Some(svf1)) = (&self.cfg.filter, &mut self.svf1) {
            let env = self.filter_env.as_mut().map(|e| e.next()).unwrap_or(0.0);
            let cutoff = (f.cutoff * 2f32.powf(f.env_amount * env * 4.0) * mods.cutoff_ratio)
                .clamp(20.0, 20000.0);
            if cutoff != self.last_cutoff {
                svf1.set_cutoff(cutoff);
                if let Some(svf2) = &mut self.svf2 {
                    svf2.set_cutoff(cutoff);
                }
                self.last_cutoff = cutoff;
            }
            s = svf1.process(s);
            if let Some(svf2) = &mut self.svf2 {
                s = svf2.process(s);
            }
        }

        let amp = self.amp_env.next() * self.cfg.level * mods.amplitude;
        let out = s * amp;
        (out, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};
    use crate::expressive::{Adsr, Filter, Osc, Osc2, Wave};

    const SR: f32 = 44100.0;

    fn render(cfg: &Subtractive, freq: f32, gate: f32, total: f32) -> Vec<f32> {
        let mut v = SubtractiveVoice::new(cfg, freq, SR);
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

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    #[test]
    fn pitch_follows_the_requested_frequency() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 330.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 660.0).abs() < 10.0);
    }

    #[test]
    fn release_sounds_past_the_gate_and_then_stops() {
        let cfg = Subtractive {
            env: Adsr {
                release: 0.5,
                ..fast()
            },
            ..Default::default()
        };
        let mut v = SubtractiveVoice::new(&cfg, 220.0, SR);
        let mods = Modulation::default();
        for _ in 0..4410 {
            v.tick(&mods);
        }
        v.gate_off();
        let after: Vec<f32> = (0..4410).map(|_| v.tick(&mods).0).collect();
        assert!(rms(&after) > 0.1, "still audible 0.1 s after gate");
        assert!(v.is_active());
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active(), "silent after the release");
    }

    #[test]
    fn low_pass_filter_removes_high_harmonics() {
        let open = Subtractive {
            env: fast(),
            ..Default::default()
        };
        let closed = Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 300.0,
                resonance: 0.1,
                slope: 12,
                env_amount: 0.0,
                env: Adsr::default(),
            }),
            env: fast(),
            ..Default::default()
        };
        let a = render(&open, 110.0, 1.0, 1.0);
        let b = render(&closed, 110.0, 1.0, 1.0);
        let h10 = 1100.0;
        assert!(
            db(goertzel_power(&b, h10, SR) / goertzel_power(&a, h10, SR)) < -15.0,
            "10th harmonic attenuated"
        );
    }

    #[test]
    fn slope_24_attenuates_more_than_12() {
        let mk = |slope| Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 300.0,
                resonance: 0.1,
                slope,
                env_amount: 0.0,
                env: Adsr::default(),
            }),
            env: fast(),
            ..Default::default()
        };
        let a = render(&mk(12), 110.0, 1.0, 1.0);
        let b = render(&mk(24), 110.0, 1.0, 1.0);
        let p = |s: &[f32]| goertzel_power(s, 2200.0, SR);
        assert!(db(p(&b) / p(&a)) < -6.0, "24 dB is steeper");
    }

    #[test]
    fn positive_filter_envelope_opens_then_closes_the_cutoff() {
        let cfg = Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 200.0,
                resonance: 0.1,
                slope: 12,
                env_amount: 1.0,
                env: Adsr {
                    attack: 0.001,
                    decay: 0.4,
                    sustain: 0.0,
                    release: 0.1,
                },
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 110.0, 1.0, 1.0);
        let early = goertzel_power(&s[..4410], 1100.0, SR);
        let late = goertzel_power(&s[26460..30870], 1100.0, SR);
        assert!(
            db(early / late) > 12.0,
            "bright at first, dark later: {}",
            db(early / late)
        );
    }

    #[test]
    fn detuned_second_oscillator_beats_at_the_detune_rate() {
        // 220 Hz with osc2 at +50 cents beats at about 220 * (2^(50/1200) - 1) = 6.5 Hz.
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            osc2: Some(Osc2 {
                wave: Wave::Sine,
                pulse_width: 0.5,
                mix: 0.5,
                detune_cents: 50.0,
                octave: 0,
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 220.0, 2.0, 2.0);
        // RMS over 20 ms windows follows the ~6.5 Hz beat with enough time
        // resolution that the peaks and troughs aren't averaged away (a 50 ms
        // window only reaches a ~3x ratio here, too close to the threshold).
        let win = 882;
        let env: Vec<f32> = s.chunks(win).map(rms).collect();
        let (min, max) = env
            .iter()
            .skip(1)
            .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(max > min * 3.0, "amplitude beats: min {min} max {max}");
    }

    #[test]
    fn octave_offset_shifts_osc2_pitch() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            osc2: Some(Osc2 {
                wave: Wave::Sine,
                pulse_width: 0.5,
                mix: 1.0,
                detune_cents: 0.0,
                octave: 1,
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 220.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
    }

    #[test]
    fn modulation_pitch_ratio_and_amplitude_are_applied() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            env: fast(),
            ..Default::default()
        };
        let mut v = SubtractiveVoice::new(&cfg, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
        };
        let s: Vec<f32> = (0..SR as usize).map(|_| v.tick(&mods).0).collect();
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
        assert!((s.iter().cloned().fold(0.0f32, f32::max) - 0.5).abs() < 0.05);
    }
}
