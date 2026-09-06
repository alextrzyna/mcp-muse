//! Percussion and one-shot sound-effect generators.
//!
//! These sounds carry their own amplitude envelopes (a kick decays the way a
//! kick decays), so the caller applies filter and effects but not an ADSR.
//! Every swept oscillator accumulates phase; `sin(2π·f(t)·t)` would shift the
//! perceived pitch by `t·f'(t)` and make descending sweeps bounce back up.

use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::{Percussion, PercussionKind};
use rand::{Rng, RngExt};
use std::f32::consts::TAU;

/// Render one percussive hit of `sample_count` samples. Every kind carries its
/// own envelope; the caller only applies `level`.
pub fn render(sample_rate: f32, p: &Percussion, sample_count: usize) -> Vec<f32> {
    let freq = p.frequency.unwrap_or(p.kind.default_frequency());
    match p.kind {
        PercussionKind::Kick => kick(
            sample_rate,
            p.punch.unwrap_or(0.8),
            p.sustain.unwrap_or(0.3),
            p.click_freq.unwrap_or(8000.0),
            freq,
            sample_count,
        ),
        PercussionKind::Snare => snare(
            sample_rate,
            p.snap.unwrap_or(0.7),
            p.buzz.unwrap_or(0.6),
            freq,
            p.noise_amount.unwrap_or(0.8),
            sample_count,
        ),
        PercussionKind::Hihat => hihat(
            sample_rate,
            p.metallic.unwrap_or(0.8),
            p.decay.unwrap_or(0.15),
            p.brightness.unwrap_or(0.9),
            freq,
            sample_count,
        ),
        PercussionKind::Cymbal => cymbal(
            sample_rate,
            p.size.unwrap_or(0.7),
            p.metallic.unwrap_or(0.9),
            p.strike_intensity.unwrap_or(0.8),
            freq,
            sample_count,
        ),
        PercussionKind::Zap => zap(
            sample_rate,
            freq,
            p.energy.unwrap_or(0.8),
            p.decay.unwrap_or(0.3),
            p.harmonic_content.unwrap_or(0.7),
            sample_count,
        ),
        PercussionKind::Swoosh => {
            let [start, end] = p.sweep.unwrap_or([200.0, 2000.0]);
            swoosh(
                sample_rate,
                p.direction.unwrap_or(0.0),
                p.intensity.unwrap_or(0.7),
                (start, end),
                sample_count,
            )
        }
        PercussionKind::Chime => chime(
            sample_rate,
            freq,
            p.harmonic_count.unwrap_or(5),
            p.decay.unwrap_or(0.5),
            p.inharmonicity.unwrap_or(0.1),
            sample_count,
        ),
        PercussionKind::Burst => burst(
            sample_rate,
            freq,
            p.bandwidth.unwrap_or(500.0),
            p.intensity.unwrap_or(0.8),
            p.shape.unwrap_or(0.5),
            sample_count,
        ),
    }
}

fn noise(rng: &mut impl Rng) -> f32 {
    rng.random::<f32>() * 2.0 - 1.0
}

/// Click transient plus a pitch-swept sine body.
fn kick(
    sample_rate: f32,
    punch: f32,
    sustain: f32,
    click_freq: f32,
    body_freq: f32,
    n: usize,
) -> Vec<f32> {
    let mut body = PhaseAccumulator::new(sample_rate);
    let mut click = PhaseAccumulator::new(sample_rate);
    let click_decay = 15.0 + punch * 25.0;
    let body_decay = 3.0 + 2.0 / sustain.max(0.1);

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let click_env = (-t * click_decay).exp();
            let click_sample = click.next(click_freq) * click_env * punch * 0.5;

            // Body starts four times above its resting pitch and drops fast.
            let sweep = body_freq * (1.0 + 3.0 * (-t * 10.0).exp());
            let body_env = (-t * body_decay).exp();
            let body_sample = body.next(sweep) * body_env;

            click_sample + body_sample
        })
        .collect()
}

/// Tonal shell resonance plus bandpassed noise "wires".
fn snare(
    sample_rate: f32,
    snap: f32,
    buzz: f32,
    tone_freq: f32,
    noise_amount: f32,
    n: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let mut tone = PhaseAccumulator::new(sample_rate);
    let mut buzz_osc = PhaseAccumulator::new(sample_rate);
    let decay_rate = 8.0 + snap * 12.0;
    let attack = 0.001;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let env = (t / attack).min(1.0) * (-t * decay_rate).exp();
            let tonal = tone.next(tone_freq) * (1.0 - noise_amount);
            let white = noise(&mut rng);
            // Wire buzz: noise amplitude-modulated by a higher tone.
            let wires = white * (1.0 - buzz * 0.5 + buzz_osc.next(tone_freq * 2.5).abs() * buzz);
            (tonal + wires * noise_amount) * env
        })
        .collect()
}

/// Three inharmonic partials plus noise, short decay.
fn hihat(
    sample_rate: f32,
    metallic: f32,
    decay: f32,
    brightness: f32,
    base_freq: f32,
    n: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let f1 = base_freq * brightness;
    let mut oscs = [
        PhaseAccumulator::new(sample_rate),
        PhaseAccumulator::new(sample_rate),
        PhaseAccumulator::new(sample_rate),
    ];
    let ratios = [1.0, 1.414, 1.732];
    let gains = [0.4, 0.3, 0.2];
    let decay_rate = 8.0 + 5.0 / decay.max(0.01);
    let attack = 0.002;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let env = (t / attack).min(1.0) * (-t * decay_rate).exp();
            let mut metal = 0.0;
            for k in 0..3 {
                metal += oscs[k].next(f1 * ratios[k]) * gains[k] * metallic;
            }
            let hiss = noise(&mut rng) * (1.0 - metallic * 0.5);
            (metal + hiss) * env
        })
        .collect()
}

/// Six inharmonic partials with a slow shimmer, long decay.
fn cymbal(
    sample_rate: f32,
    size: f32,
    metallic: f32,
    strike_intensity: f32,
    base_freq: f32,
    n: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let fundamental = base_freq * (0.5 + size * 0.5);
    let ratios = [
        1.0,
        1.593,
        2.135,
        std::f32::consts::E,
        std::f32::consts::PI,
        4.236,
    ];
    let gains = [0.3, 0.25, 0.2, 0.15, 0.1, 0.05];
    let mut oscs = [PhaseAccumulator::new(sample_rate); 6];
    let decay_rate = 1.0 + size * 2.0;
    let strike = 0.7 + strike_intensity * 0.3;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let env = (-t * decay_rate).exp();
            let shimmer = (TAU * 4.0 * t).sin() * 0.1 + 1.0;
            let mut metal = 0.0;
            for k in 0..6 {
                metal += oscs[k].next(fundamental * ratios[k]) * gains[k] * metallic;
            }
            let hiss = noise(&mut rng) * (1.0 - metallic * 0.3) * 0.3;
            (metal * shimmer + hiss) * env * strike
        })
        .collect()
}

/// Descending inharmonic sweep with a noise burst.
fn zap(
    sample_rate: f32,
    frequency: f32,
    energy: f32,
    decay: f32,
    harmonic_content: f32,
    n: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let mut base = PhaseAccumulator::new(sample_rate);
    let duration = n as f32 / sample_rate;
    let env_rate = 8.0 + decay * 12.0;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let progress = t / duration.max(0.01);
            let env = (-t * env_rate).exp();

            // Sweep from 2x down to 0.3x over the sound.
            let freq = frequency * (2.0 + (0.3 - 2.0) * progress);
            let phase = base.next_phase(freq);
            let fundamental = phase.sin();
            let overtone2 = (phase * 2.3).sin() * 0.6;
            let overtone3 = (phase * 3.7).sin() * 0.4;
            let harmonic_sum =
                (fundamental + overtone2 + overtone3) * (0.4 + harmonic_content * 0.6);

            let burst = noise(&mut rng) * (-t * 25.0 * energy.max(0.1)).exp() * energy * 0.3;
            (harmonic_sum + burst) * env
        })
        .collect()
}

/// Filtered noise sweep; `direction` > 0.5 fades out, otherwise fades in.
fn swoosh(
    sample_rate: f32,
    direction: f32,
    intensity: f32,
    frequency_sweep: (f32, f32),
    n: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let duration = n as f32 / sample_rate;
    let mut lp = 0.0f32;
    let mut lp2 = 0.0f32;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let progress = t / duration.max(0.01);
            let env = if direction > 0.5 {
                (1.0 - progress).powi(2)
            } else {
                progress.powi(2)
            } * intensity;

            let center = frequency_sweep.0 + (frequency_sweep.1 - frequency_sweep.0) * progress;
            // Two cascaded one-pole lowpasses tracking the sweep give a
            // simple moving "whoosh" colour without a full bandpass stage.
            let alpha = (TAU * center / sample_rate).min(0.99);
            let white = noise(&mut rng);
            lp += alpha * (white - lp);
            lp2 += alpha * (lp - lp2);
            lp2 * env * 2.0
        })
        .collect()
}

/// Sum of decaying inharmonic partials, like a struck bell or bar.
fn chime(
    sample_rate: f32,
    fundamental: f32,
    harmonic_count: u8,
    decay: f32,
    inharmonicity: f32,
    sample_count: usize,
) -> Vec<f32> {
    let count = harmonic_count.max(1) as usize;
    let norm = 1.0 / (count as f32).sqrt();
    (0..sample_count)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let mut out = 0.0;
            for n in 1..=count {
                let partial =
                    fundamental * n as f32 * (1.0 + inharmonicity * (n as f32 - 1.0) * 0.01);
                let env = (-t * (1.0 / decay.max(0.01)) * (1.0 + n as f32 * 0.1)).exp();
                out += (TAU * partial * t).sin() * env / n as f32;
            }
            out * norm
        })
        .collect()
}

/// Noise centered on `center_freq` with a shaped decay envelope.
fn burst(
    sample_rate: f32,
    center_freq: f32,
    bandwidth: f32,
    intensity: f32,
    shape: f32,
    sample_count: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let mut tone = PhaseAccumulator::new(sample_rate);
    let mut lp = 0.0f32;
    let alpha = (TAU * (center_freq + bandwidth) / sample_rate).min(0.99);
    let duration = (sample_count as f32 / sample_rate).max(0.1);
    (0..sample_count)
        .map(|i| {
            let progress = i as f32 / sample_rate / duration;
            let env = if shape < 0.5 {
                (-progress * 8.0).exp()
            } else {
                (-(progress * 3.0).powi(2)).exp()
            };
            lp += alpha * (noise(&mut rng) - lp);
            (lp * 1.5 + tone.next(center_freq) * 0.3) * env * intensity
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::PercussionKind;
    use crate::expressive::test_util::{goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn cfg(kind: PercussionKind) -> Percussion {
        Percussion {
            kind,
            level: 1.0,
            frequency: None,
            punch: None,
            sustain: None,
            click_freq: None,
            snap: None,
            buzz: None,
            noise_amount: None,
            metallic: None,
            decay: None,
            brightness: None,
            size: None,
            strike_intensity: None,
            energy: None,
            harmonic_content: None,
            direction: None,
            intensity: None,
            sweep: None,
            harmonic_count: None,
            inharmonicity: None,
            bandwidth: None,
            shape: None,
        }
    }

    #[test]
    fn every_kind_renders_finite_bounded_audio() {
        for kind in [
            PercussionKind::Kick,
            PercussionKind::Snare,
            PercussionKind::Hihat,
            PercussionKind::Cymbal,
            PercussionKind::Zap,
            PercussionKind::Swoosh,
            PercussionKind::Chime,
            PercussionKind::Burst,
        ] {
            let s = render(SR, &cfg(kind), (0.5 * SR) as usize);
            assert_eq!(s.len(), (0.5 * SR) as usize);
            // Zap sums three incommensurate partials plus a noise burst, so
            // constructive interference can push its peak toward ~1.8 even
            // though every other kind stays under 1.4; 2.0 still catches a
            // genuine runaway (NaN/blow-up) while tolerating that legitimate
            // peak.
            assert!(
                s.iter().all(|x| x.is_finite() && x.abs() <= 2.0),
                "{kind:?}"
            );
            assert!(rms(&s) > 0.01, "{kind:?} is silent");
        }
    }

    #[test]
    fn kick_pitch_sweeps_downward() {
        let s = render(SR, &cfg(PercussionKind::Kick), (0.5 * SR) as usize);
        let early = zero_crossing_rate(&s[..2205], SR);
        let late = zero_crossing_rate(&s[8820..13230], SR);
        assert!(early > late * 1.3, "early {early} vs late {late}");
    }

    #[test]
    fn frequency_moves_the_kick_body() {
        let mut low = cfg(PercussionKind::Kick);
        low.frequency = Some(45.0);
        let mut high = cfg(PercussionKind::Kick);
        high.frequency = Some(90.0);
        let l = render(SR, &low, (0.5 * SR) as usize);
        let h = render(SR, &high, (0.5 * SR) as usize);
        assert!(zero_crossing_rate(&h[4410..], SR) > zero_crossing_rate(&l[4410..], SR) * 1.5);
    }

    #[test]
    fn chime_has_a_partial_at_its_fundamental_that_decays() {
        let mut c = cfg(PercussionKind::Chime);
        c.frequency = Some(880.0);
        let s = render(SR, &c, SR as usize);
        let early = goertzel_power(&s[..8820], 880.0, SR);
        let late = goertzel_power(&s[35280..], 880.0, SR);
        assert!(
            early > goertzel_power(&s[..8820], 1100.0, SR) * 10.0,
            "fundamental present"
        );
        assert!(early > late * 4.0, "decays: {early} vs {late}");
    }

    #[test]
    fn swoosh_sweeps_between_its_endpoints() {
        let mut up = cfg(PercussionKind::Swoosh);
        up.sweep = Some([200.0, 4000.0]);
        let s = render(SR, &up, SR as usize);
        let early = zero_crossing_rate(&s[..4410], SR);
        let late = zero_crossing_rate(&s[39690..], SR);
        assert!(late > early * 2.0, "rises: {early} -> {late}");
    }
}
