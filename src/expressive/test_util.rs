//! Small signal-analysis helpers shared by the DSP unit tests.
#![allow(dead_code)]

/// Deterministic white noise in [-1, 1] (xorshift; no dependency on `rand` state).
pub fn white_noise(len: usize, seed: u32) -> Vec<f32> {
    let mut state = seed.max(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

pub fn sine(freq: f32, seconds: f32, sample_rate: f32, amplitude: f32) -> Vec<f32> {
    let n = (seconds * sample_rate) as usize;
    (0..n)
        .map(|i| (std::f32::consts::TAU * freq * i as f32 / sample_rate).sin() * amplitude)
        .collect()
}

/// Power of `samples` at `freq` via the Goertzel algorithm.
pub fn goertzel_power(samples: &[f32], freq: f32, sample_rate: f32) -> f32 {
    let k = (0.5 + samples.len() as f32 * freq / sample_rate).floor();
    let w = std::f32::consts::TAU * k / samples.len() as f32;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in samples {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / samples.len() as f32
}

/// Power ratio in decibels.
pub fn db(ratio: f32) -> f32 {
    10.0 * ratio.max(1e-12).log10()
}

/// Amplitude ratio in decibels.
pub fn db_amp(ratio: f32) -> f32 {
    20.0 * ratio.max(1e-12).log10()
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Zero crossings per second, a crude pitch estimate for a clean tone.
pub fn zero_crossing_rate(samples: &[f32], sample_rate: f32) -> f32 {
    let crossings = samples
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    crossings as f32 / (samples.len() as f32 / sample_rate)
}
