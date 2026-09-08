//! A gentle per-patch peak limiter: instant attack, exponential release,
//! fixed ceiling. Below the ceiling the gain is exactly 1.0, so quiet
//! material passes through bit-identically.

#[derive(Debug, Clone)]
pub struct PeakLimiter {
    ceiling: f32,
    /// Per-sample release coefficient toward unity gain.
    release: f32,
    gain: f32,
}

impl PeakLimiter {
    pub fn new(ceiling: f32, release_seconds: f32, sample_rate: f32) -> Self {
        let samples = (release_seconds * sample_rate).max(1.0);
        Self {
            ceiling: ceiling.max(1e-3),
            release: 1.0 - (-1.0 / samples).exp(),
            gain: 1.0,
        }
    }

    /// Current gain: 1.0 when idle, below 1.0 while recovering from a peak.
    #[allow(dead_code)] // exercised by tests; a future task may read it live
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Limits a stereo frame with one shared gain so the image is preserved.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        // A non-finite sample would give `target = ceiling / inf = 0` and then
        // `inf * 0 = NaN`, which would travel on to the engine's soft clipper
        // and poison the mix. Drop it to silence instead; a branch, no
        // allocation, so this stays safe in the per-sample loop.
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let peak = l.abs().max(r.abs());
        let target = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.gain = if target < self.gain {
            target
        } else {
            (self.gain + (1.0 - self.gain) * self.release).min(target)
        };
        (l * self.gain, r * self.gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::sine;

    const SR: f32 = 44100.0;

    fn run(lim: &mut PeakLimiter, input: &[f32]) -> Vec<f32> {
        input.iter().map(|&x| lim.process(x, x).0).collect()
    }

    #[test]
    fn output_never_exceeds_the_ceiling() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let out = run(&mut lim, &sine(220.0, 0.5, SR, 3.0));
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 1.5 + 1e-6, "peak {peak}");
        assert!(
            peak > 1.4,
            "the limiter holds the signal near the ceiling, not far under it: {peak}"
        );
    }

    #[test]
    fn signal_under_the_ceiling_is_bit_identical() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let input = sine(220.0, 0.5, SR, 1.49);
        let out = run(&mut lim, &input);
        assert_eq!(out, input);
        assert_eq!(lim.gain(), 1.0);
    }

    #[test]
    fn gain_recovers_after_the_loud_part_ends() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let mut input = sine(220.0, 0.2, SR, 3.0);
        input.extend(sine(220.0, 0.5, SR, 0.3));
        let out = run(&mut lim, &input);
        let loud_end = (0.2 * SR) as usize;
        // Right after the burst the gain is still ~0.5, so the quiet part is attenuated...
        let early = &out[loud_end..loud_end + 100];
        assert!(early.iter().fold(0.0f32, |m, x| m.max(x.abs())) < 0.25);
        // ...and within five release times it is back to unity.
        let late = &out[loud_end + (0.3 * SR) as usize..];
        let late_peak = late.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((late_peak - 0.3).abs() < 0.01, "recovered to {late_peak}");
    }

    #[test]
    fn a_non_finite_sample_becomes_silence_instead_of_nan() {
        for bad in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let mut lim = PeakLimiter::new(1.5, 0.05, SR);
            let (l, r) = lim.process(bad, 0.5);
            assert!(l.is_finite() && r.is_finite(), "{bad} produced ({l}, {r})");
            assert_eq!(l, 0.0, "the bad channel is silenced: {l}");
            assert_eq!(r, 0.5, "the good channel passes through: {r}");
            assert_eq!(lim.gain(), 1.0, "no phantom gain reduction");
        }
    }

    #[test]
    fn stereo_uses_the_louder_channel_and_keeps_the_image() {
        let mut lim = PeakLimiter::new(1.0, 0.05, SR);
        let (l, r) = lim.process(2.0, 0.5);
        assert!((l - 1.0).abs() < 1e-6);
        assert!((r - 0.25).abs() < 1e-6, "same gain on both channels: {r}");
    }
}
