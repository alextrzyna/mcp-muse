//! A percussive hit pre-rendered at note-on; ignores gate-off.
#![allow(dead_code)]

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{Percussion, PercussionKind, percussion};

/// Shortest hit a percussion voice renders regardless of gate.
pub const MIN_HIT_SECONDS: f32 = 0.05;

/// Raised-cosine fade applied to the end of a rendered hit. The buffer is cut
/// at the gate, so without it a still-ringing cymbal stops dead and clicks.
pub const HIT_FADE_SECONDS: f32 = 0.005;

/// Every hit is normalised to this peak before `level` applies, so `level`
/// means the same loudness for a kick as for a cymbal.
pub const PERCUSSION_PEAK: f32 = 0.9;

/// The part of a kind's loudness that is a *uniform* scalar on the whole
/// buffer, as opposed to a change of shape or spectrum.
///
/// Normalising to a fixed peak divides any such scalar straight back out, so
/// a parameter that only scales the buffer would become inert. Three of them
/// are exposed on the tool schema (`strike_intensity` on a cymbal,
/// `intensity` on a swoosh and a burst), so the normalisation target is
/// `PERCUSSION_PEAK * uniform_intensity` instead of `PERCUSSION_PEAK`: every
/// kind still lands on a common peak at full intensity, and turning the
/// parameter down still makes the hit quieter. The defaults here must match
/// the ones `percussion::render` passes to each generator.
fn uniform_intensity(cfg: &Percussion) -> f32 {
    match cfg.kind {
        // `cymbal` multiplies its output by `0.7 + strike_intensity * 0.3`.
        PercussionKind::Cymbal => 0.7 + cfg.strike_intensity.unwrap_or(0.8) * 0.3,
        PercussionKind::Swoosh => cfg.intensity.unwrap_or(0.7),
        PercussionKind::Burst => cfg.intensity.unwrap_or(0.8),
        // Every other kind's parameters change shape or spectrum, not level.
        _ => 1.0,
    }
}

/// Scales `samples` in place so its absolute peak becomes `target`. A silent
/// buffer (peak near zero) is left untouched.
fn normalise_peak(samples: &mut [f32], target: f32) {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 1e-6 {
        let g = target / peak;
        for s in samples.iter_mut() {
            *s *= g;
        }
    }
}

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
        // Fade first, then normalise the faded buffer: some kinds (e.g. a
        // rising `swoosh`) peak in the last few milliseconds, which is
        // exactly the region the fade attenuates. Normalising afterwards
        // guarantees the *played* peak lands on PERCUSSION_PEAK regardless
        // of where in the envelope the loudest sample falls.
        fade_out(&mut samples, (HIT_FADE_SECONDS * sample_rate) as usize);
        normalise_peak(&mut samples, PERCUSSION_PEAK * uniform_intensity(cfg));
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
        assert!(
            peak > 0.1 && peak <= 0.5 * PERCUSSION_PEAK + 1e-4,
            "level applied: {peak}"
        );
    }

    fn played_peak(json: &str) -> f32 {
        let cfg: Percussion = serde_json::from_str(json).unwrap();
        let mut v = PercussionVoice::new(&cfg, 0.5, 44100.0);
        let mods = Modulation::default();
        let mut peak = 0.0f32;
        while v.is_active() {
            peak = peak.max(v.tick(&mods).0.abs());
        }
        peak
    }

    #[test]
    fn every_kind_is_normalised_to_the_same_peak() {
        for kind in [
            "kick", "snare", "hihat", "cymbal", "zap", "swoosh", "chime", "burst",
        ] {
            let json = format!(r#"{{"kind": "{kind}", "level": 1.0}}"#);
            let cfg: Percussion = serde_json::from_str(&json).unwrap();
            // The target is the common peak scaled by the kind's uniform
            // intensity, so a kind whose default intensity is below 1.0
            // (swoosh 0.7, burst 0.8, cymbal 0.94) still lands under it.
            let target = PERCUSSION_PEAK * uniform_intensity(&cfg);
            let peak = played_peak(&json);
            assert!((peak - target).abs() < 0.02, "{kind} peaks at {peak}");
        }
    }

    /// Normalising to a flat peak would divide the uniform intensity scalars
    /// back out and make `strike_intensity` / `intensity` inert; these are
    /// MCP-exposed parameters, and two built-in patches set them.
    #[test]
    fn uniform_intensity_parameters_still_change_the_level() {
        let loud = played_peak(r#"{"kind": "swoosh", "level": 1.0, "intensity": 1.0}"#);
        let quiet = played_peak(r#"{"kind": "swoosh", "level": 1.0, "intensity": 0.3}"#);
        assert!(
            (quiet - 0.3 * loud).abs() < 0.02,
            "a 0.3 swoosh should be 0.3x a 1.0 swoosh: {quiet} vs {loud}"
        );

        let loud = played_peak(r#"{"kind": "burst", "level": 1.0, "intensity": 1.0}"#);
        let quiet = played_peak(r#"{"kind": "burst", "level": 1.0, "intensity": 0.4}"#);
        assert!(
            (quiet - 0.4 * loud).abs() < 0.02,
            "a 0.4 burst should be 0.4x a 1.0 burst: {quiet} vs {loud}"
        );

        // A cymbal's strike runs 0.7..1.0, so a 0.0 strike is 0.7x a 1.0 one.
        let hard = played_peak(r#"{"kind": "cymbal", "level": 1.0, "strike_intensity": 1.0}"#);
        let soft = played_peak(r#"{"kind": "cymbal", "level": 1.0, "strike_intensity": 0.0}"#);
        assert!(
            (soft - 0.7 * hard).abs() < 0.02,
            "a soft strike should be 0.7x a hard one: {soft} vs {hard}"
        );
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
