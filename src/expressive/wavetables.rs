//! Procedural wavetables, generated once and band-limited per octave so a
//! high note never includes partials above Nyquist.

use crate::expressive::TableName;
use std::f32::consts::TAU;
use std::sync::OnceLock;

pub const TABLE_SIZE: usize = 2048;
/// One level per octave from A0 (27.5 Hz) up; level 9 covers 14 kHz and above.
pub const MIP_LEVELS: usize = 10;
pub const LOWEST_FUNDAMENTAL_HZ: f32 = 27.5;
/// Partials above this frequency are left out of every level.
const HIGHEST_PARTIAL_HZ: f32 = 20000.0;

/// One sinusoidal component of a table: `ratio` times the fundamental.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Partial {
    pub ratio: f32,
    pub amplitude: f32,
    /// Phase offset in cycles (0..1).
    pub phase: f32,
}

fn harmonic(n: u32, amplitude: f32) -> Partial {
    Partial {
        ratio: n as f32,
        amplitude,
        phase: 0.0,
    }
}

/// Fourier series of a pulse wave with duty `d` (DC removed), harmonics 1..=max.
fn pulse_partials(duty: f32, max: u32, gain: f32) -> impl Iterator<Item = Partial> {
    (1..=max).map(move |n| {
        let nf = n as f32;
        let amplitude =
            gain * 2.0 / (nf * std::f32::consts::PI) * (nf * std::f32::consts::PI * duty).sin();
        Partial {
            ratio: nf,
            amplitude,
            phase: -(nf * duty) / 2.0, // centres the pulse so the three duty cycles add coherently
        }
    })
}

/// The additive recipe for each table (mirrors the tryx-fx generators).
pub fn partials(table: TableName) -> Vec<Partial> {
    match table {
        TableName::Basic => vec![harmonic(1, 0.8), harmonic(2, 0.15), harmonic(3, 0.05)],
        TableName::Warm => (1..=15u32)
            .step_by(2)
            .map(|h| harmonic(h, 0.7 / h as f32))
            .collect(),
        TableName::Bright => (1..=20u32).map(|h| harmonic(h, 0.5 / h as f32)).collect(),
        TableName::Digital => vec![
            harmonic(1, 0.4),
            Partial {
                ratio: 2.5,
                amplitude: 0.3,
                phase: 0.0,
            },
            Partial {
                ratio: 4.1,
                amplitude: 0.2,
                phase: 0.0,
            },
            Partial {
                ratio: 7.3,
                amplitude: 0.1,
                phase: 0.0,
            },
        ],
        TableName::Vocal => vec![
            harmonic(1, 0.3),
            harmonic(3, 0.5),
            harmonic(5, 0.4),
            harmonic(7, 0.2),
            harmonic(9, 0.1),
        ],
        TableName::Pwm => {
            let mut v: Vec<Partial> = Vec::new();
            for duty in [0.3, 0.5, 0.7] {
                v.extend(pulse_partials(duty, 48, 0.25));
            }
            v
        }
        TableName::Organ => vec![
            harmonic(1, 0.8),
            harmonic(2, 0.6),
            harmonic(3, 0.5),
            harmonic(4, 0.3),
            harmonic(5, 0.2),
            harmonic(6, 0.15),
            harmonic(8, 0.1),
        ],
        TableName::Noise => (1..=32u32)
            .map(|h| {
                let golden = h as f32 * h as f32 * 1.618_034;
                Partial {
                    ratio: h as f32,
                    amplitude: 0.3 / h as f32,
                    phase: golden - golden.floor(),
                }
            })
            .collect(),
    }
}

/// Mip level for a fundamental: one per octave above A0, clamped to the table.
pub fn mip_level(frequency: f32) -> usize {
    let octaves = (frequency.max(1.0) / LOWEST_FUNDAMENTAL_HZ).log2().floor();
    (octaves.max(0.0) as usize).min(MIP_LEVELS - 1)
}

/// Highest partial ratio that stays below `HIGHEST_PARTIAL_HZ` for every
/// fundamental in this level's octave (the octave's top, not its bottom).
pub fn max_ratio(level: usize) -> f32 {
    let top_of_octave = LOWEST_FUNDAMENTAL_HZ * 2f32.powi(level as i32 + 1);
    (HIGHEST_PARTIAL_HZ / top_of_octave).max(1.0)
}

fn synthesize(partials: &[Partial], max_ratio: f32) -> Vec<f32> {
    let mut table = vec![0.0f32; TABLE_SIZE];
    for p in partials.iter().filter(|p| p.ratio <= max_ratio) {
        for (i, s) in table.iter_mut().enumerate() {
            let phase = i as f32 / TABLE_SIZE as f32;
            *s += p.amplitude * (TAU * (p.ratio * phase + p.phase)).sin();
        }
    }
    // Normalise each level to a peak of 1 so morphing and layering are predictable.
    let peak = table.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        for s in &mut table {
            *s /= peak;
        }
    }
    table
}

struct Tables {
    /// [table][level][sample]
    data: Vec<Vec<Vec<f32>>>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| Tables {
        data: TableName::ALL
            .iter()
            .map(|&t| {
                let parts = partials(t);
                (0..MIP_LEVELS)
                    .map(|level| synthesize(&parts, max_ratio(level)))
                    .collect()
            })
            .collect(),
    })
}

#[inline]
fn interpolate(table: &[f32], phase: f32) -> f32 {
    let pos = phase.rem_euclid(1.0) * TABLE_SIZE as f32;
    let i0 = pos as usize % TABLE_SIZE;
    let i1 = (i0 + 1) % TABLE_SIZE;
    let frac = pos - pos.floor();
    table[i0] * (1.0 - frac) + table[i1] * frac
}

/// One sample of `table` at `level`, unit `phase` 0..1.
#[inline]
pub fn sample(table: TableName, level: usize, phase: f32) -> f32 {
    interpolate(
        &tables().data[table.index()][level.min(MIP_LEVELS - 1)],
        phase,
    )
}

/// Every partial regardless of Nyquist; used by tests as the aliasing baseline.
#[allow(dead_code)] // only exercised by this module's own tests
pub fn naive_sample(table: TableName, phase: f32) -> f32 {
    partials(table)
        .iter()
        .map(|p| p.amplitude * (TAU * (p.ratio * phase + p.phase)).sin())
        .sum::<f32>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms};

    const SR: f32 = 44100.0;

    fn render(table: TableName, level: usize, freq: f32, seconds: f32) -> Vec<f32> {
        let n = (seconds * SR) as usize;
        (0..n)
            .map(|i| sample(table, level, (freq * i as f32 / SR).rem_euclid(1.0)))
            .collect()
    }

    fn render_naive(table: TableName, freq: f32, seconds: f32) -> Vec<f32> {
        let n = (seconds * SR) as usize;
        (0..n)
            .map(|i| naive_sample(table, (freq * i as f32 / SR).rem_euclid(1.0)))
            .collect()
    }

    #[test]
    fn mip_level_follows_octaves_from_a0() {
        assert_eq!(mip_level(20.0), 0);
        assert_eq!(mip_level(27.5), 0);
        assert_eq!(mip_level(54.9), 0);
        assert_eq!(mip_level(55.0), 1);
        assert_eq!(mip_level(440.0), 4);
        assert_eq!(mip_level(20000.0), 9);
        assert!(max_ratio(0) > max_ratio(9));
        assert!(max_ratio(9) >= 1.0, "the fundamental always survives");
    }

    #[test]
    fn every_table_is_finite_normalised_and_has_a_fundamental() {
        for table in TableName::ALL {
            let s = render(table, 4, 440.0, 0.5);
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(s.iter().all(|x| x.is_finite()), "{table:?}");
            assert!(peak > 0.5 && peak <= 1.0, "{table:?} peak {peak}");
            assert!(
                goertzel_power(&s, 440.0, SR) > goertzel_power(&s, 1000.0, SR),
                "{table:?} has its fundamental"
            );
        }
    }

    #[test]
    fn organ_and_warm_have_their_signature_harmonics() {
        let organ = render(TableName::Organ, 3, 220.0, 0.5);
        assert!(db(goertzel_power(&organ, 440.0, SR) / goertzel_power(&organ, 220.0, SR)) > -6.0);
        let warm = render(TableName::Warm, 3, 220.0, 0.5);
        let even = goertzel_power(&warm, 440.0, SR);
        let odd = goertzel_power(&warm, 660.0, SR);
        assert!(db(odd / even) > 20.0, "warm is odd harmonics only");
    }

    #[test]
    fn high_notes_alias_less_than_the_naive_table() {
        // 5000 Hz fundamental: harmonics above the 4th fold back below Nyquist.
        let level = mip_level(5000.0);
        let limited = render(TableName::Bright, level, 5000.0, 0.5);
        let naive = render_naive(TableName::Bright, 5000.0, 0.5);
        // 6th harmonic 30000 Hz folds to 14100 Hz.
        let alias = 14100.0;
        assert!(
            db(goertzel_power(&limited, alias, SR) / goertzel_power(&naive, alias, SR)) < -20.0,
            "band-limited table removes the folded partial"
        );
        assert!(rms(&limited) > 0.1, "still audible");
    }

    #[test]
    fn noise_table_is_broadband_but_periodic() {
        let s = render(TableName::Noise, 2, 110.0, 0.5);
        let lo = goertzel_power(&s, 110.0 * 3.0, SR);
        let hi = goertzel_power(&s, 110.0 * 20.0, SR);
        assert!(
            db(lo / hi).abs() < 25.0,
            "energy spread over many harmonics"
        );
    }
}
