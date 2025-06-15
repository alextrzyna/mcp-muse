pub mod r2d2;
pub mod synth;
pub mod fundsp_synth;

pub use r2d2::{R2D2Emotion, R2D2Expression, R2D2Voice};
pub use synth::{ExpressiveSynth, SynthParams, SynthType, EnvelopeParams, FilterParams, FilterType, EffectParams, EffectType, NoiseColor};
pub use fundsp_synth::{FunDSPSynth, FunDSPParams, FunDSPSynthType};
