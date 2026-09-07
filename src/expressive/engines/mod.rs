//! One `Voice` per note per enabled engine. The renderer sums voices, so a
//! voice only has to produce its own signal and report when it has finished.

pub mod percussion;
pub mod subtractive;

#[allow(unused_imports)]
pub use percussion::{MIN_HIT_SECONDS, PercussionVoice};
#[allow(unused_imports)]
pub use subtractive::SubtractiveVoice;

/// Per-sample modulation inputs shared by all voices (the LFO writes these in a later PR).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modulation {
    pub pitch_ratio: f32,
    pub cutoff_ratio: f32,
    pub amplitude: f32,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            cutoff_ratio: 1.0,
            amplitude: 1.0,
        }
    }
}

#[allow(dead_code)]
pub trait Voice {
    /// The note ended; start the release (percussion ignores this).
    fn gate_off(&mut self);
    /// False once the voice has nothing more to output.
    fn is_active(&self) -> bool;
    /// One stereo sample, already scaled by the engine's `level`.
    fn tick(&mut self, mods: &Modulation) -> (f32, f32);
}
