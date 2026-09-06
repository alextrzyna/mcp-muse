//! Percussion and one-shot sound-effect generators.
//!
//! These sounds carry their own amplitude envelopes (a kick decays the way a
//! kick decays), so the caller applies filter and effects but not an ADSR.
//! Every swept oscillator accumulates phase; `sin(2π·f(t)·t)` would shift the
//! perceived pitch by `t·f'(t)` and make descending sweeps bounce back up.

use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::synth::SynthType;
use rand::{Rng, RngExt};
use std::f32::consts::TAU;

/// Render `sample_count` samples of a percussive type at unit amplitude.
/// Returns `None` for non-percussive types.
pub fn render(
    sample_rate: f32,
    synth_type: &SynthType,
    frequency: f32,
    sample_count: usize,
) -> Option<Vec<f32>> {
    let out = match synth_type {
        SynthType::Kick {
            punch,
            sustain,
            click_freq,
            body_freq,
        } => kick(
            sample_rate,
            *punch,
            *sustain,
            *click_freq,
            *body_freq,
            sample_count,
        ),
        SynthType::Snare {
            snap,
            buzz,
            tone_freq,
            noise_amount,
        } => snare(
            sample_rate,
            *snap,
            *buzz,
            *tone_freq,
            *noise_amount,
            sample_count,
        ),
        SynthType::HiHat {
            metallic,
            decay,
            brightness,
        } => hihat(
            sample_rate,
            *metallic,
            *decay,
            *brightness,
            frequency,
            sample_count,
        ),
        SynthType::Cymbal {
            size,
            metallic,
            strike_intensity,
        } => cymbal(
            sample_rate,
            *size,
            *metallic,
            *strike_intensity,
            frequency,
            sample_count,
        ),
        SynthType::Zap {
            energy,
            decay,
            harmonic_content,
        } => zap(
            sample_rate,
            frequency,
            *energy,
            *decay,
            *harmonic_content,
            sample_count,
        ),
        SynthType::Swoosh {
            direction,
            intensity,
            frequency_sweep,
        } => swoosh(
            sample_rate,
            *direction,
            *intensity,
            *frequency_sweep,
            sample_count,
        ),
        _ => return None,
    };
    Some(out)
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
