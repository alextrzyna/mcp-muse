pub mod engine;
pub mod gm_names;
pub mod parser;
pub mod player;
pub mod translate;

pub use engine::PlayMode;
pub use player::*;

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Musical time representation using bar.beat.tick notation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MusicalTime {
    /// Bar number (1-based)
    pub bar: u32,
    /// Beat within the bar (1-based)
    pub beat: u32,
    /// Tick/subdivision within the beat (0-based, typically 0-479 for 480 PPQ)
    pub tick: u32,
}

impl MusicalTime {
    #[allow(dead_code)]
    pub fn new(bar: u32, beat: u32, tick: u32) -> Self {
        Self { bar, beat, tick }
    }

    /// Convert to absolute seconds given tempo and time signature
    pub fn to_seconds(&self, tempo: u32, beats_per_bar: u32, ticks_per_beat: u32) -> f64 {
        let seconds_per_beat = 60.0 / tempo as f64;

        // Calculate total beats from start
        let total_beats = ((self.bar - 1) * beats_per_bar + (self.beat - 1)) as f64
            + (self.tick as f64 / ticks_per_beat as f64);

        total_beats * seconds_per_beat
    }

    /// Create from absolute seconds
    #[allow(dead_code)]
    pub fn from_seconds(seconds: f64, tempo: u32, beats_per_bar: u32, ticks_per_beat: u32) -> Self {
        let seconds_per_beat = 60.0 / tempo as f64;
        let total_beats = seconds / seconds_per_beat;

        let bar = (total_beats / beats_per_bar as f64).floor() as u32 + 1;
        let remaining_beats = total_beats % beats_per_bar as f64;
        let beat = remaining_beats.floor() as u32 + 1;
        let tick = ((remaining_beats.fract() * ticks_per_beat as f64).round() as u32)
            .min(ticks_per_beat - 1);

        Self { bar, beat, tick }
    }

    /// Quantize to nearest grid position
    #[allow(dead_code)]
    pub fn quantize(&self, grid_division: u32, ticks_per_beat: u32, beats_per_bar: u32) -> Self {
        let ticks_per_division = ticks_per_beat / grid_division;
        let quantized_tick =
            ((self.tick as f64 / ticks_per_division as f64).round() as u32) * ticks_per_division;

        if quantized_tick >= ticks_per_beat {
            // Overflow to next beat
            if self.beat >= beats_per_bar {
                // Overflow to next bar
                Self {
                    bar: self.bar + 1,
                    beat: 1,
                    tick: 0,
                }
            } else {
                Self {
                    bar: self.bar,
                    beat: self.beat + 1,
                    tick: 0,
                }
            }
        } else {
            Self {
                bar: self.bar,
                beat: self.beat,
                tick: quantized_tick,
            }
        }
    }
}

impl fmt::Display for MusicalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.bar, self.beat, self.tick)
    }
}

/// Duration in musical terms: a number is a length in bars, a string is a
/// note value ("quarter", "eighth", ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MusicalDuration {
    /// Duration in bars (e.g., 1.5 = one and a half bars)
    Bars(f64),
    /// Musical note values
    NoteValue(NoteValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteValue {
    #[serde(rename = "whole")]
    Whole,
    #[serde(rename = "half")]
    Half,
    #[serde(rename = "quarter")]
    Quarter,
    #[serde(rename = "eighth")]
    Eighth,
    #[serde(rename = "sixteenth")]
    Sixteenth,
    #[serde(rename = "triplet")]
    Triplet,
}

impl MusicalDuration {
    /// Convert to seconds given tempo
    pub fn to_seconds(&self, tempo: u32, beats_per_bar: u32) -> f64 {
        let seconds_per_beat = 60.0 / tempo as f64;

        match self {
            MusicalDuration::Bars(bars) => bars * beats_per_bar as f64 * seconds_per_beat,
            MusicalDuration::NoteValue(note) => match note {
                NoteValue::Whole => 4.0 * seconds_per_beat,
                NoteValue::Half => 2.0 * seconds_per_beat,
                NoteValue::Quarter => seconds_per_beat,
                NoteValue::Eighth => 0.5 * seconds_per_beat,
                NoteValue::Sixteenth => 0.25 * seconds_per_beat,
                NoteValue::Triplet => (2.0 / 3.0) * seconds_per_beat,
            },
        }
    }
}

/// Quantization grid options
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum QuantizeGrid {
    #[serde(rename = "off")]
    #[default]
    Off,
    #[serde(rename = "bar")]
    Bar,
    #[serde(rename = "beat")]
    Beat,
    #[serde(rename = "8th")]
    Eighth,
    #[serde(rename = "16th")]
    Sixteenth,
    #[serde(rename = "32nd")]
    ThirtySecond,
    #[serde(rename = "triplet")]
    Triplet,
}

impl QuantizeGrid {
    /// Grid divisions per beat, or `None` for `Off` and `Bar` (handled separately).
    fn divisions_per_beat(&self) -> Option<u32> {
        match self {
            QuantizeGrid::Off | QuantizeGrid::Bar => None,
            QuantizeGrid::Beat => Some(1),
            QuantizeGrid::Eighth => Some(2),
            QuantizeGrid::Sixteenth => Some(4),
            QuantizeGrid::ThirtySecond => Some(8),
            QuantizeGrid::Triplet => Some(3),
        }
    }

    /// Snap a musical position to this grid.
    pub fn apply(
        &self,
        time: &MusicalTime,
        ticks_per_beat: u32,
        beats_per_bar: u32,
    ) -> MusicalTime {
        match self {
            QuantizeGrid::Off => time.clone(),
            QuantizeGrid::Bar => {
                // Round to the nearest bar line.
                let beats_in = (time.beat - 1) as f64 + time.tick as f64 / ticks_per_beat as f64;
                let bar = if beats_in * 2.0 >= beats_per_bar as f64 {
                    time.bar + 1
                } else {
                    time.bar
                };
                MusicalTime {
                    bar,
                    beat: 1,
                    tick: 0,
                }
            }
            _ => {
                let divisions = self.divisions_per_beat().unwrap_or(1);
                time.quantize(divisions, ticks_per_beat, beats_per_bar)
            }
        }
    }
}

/// Custom deserializer that converts null to None for optional fields
fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt)
}

/// Universal effect configuration for all audio sources.
///
/// Serialized flat: `{"type": "reverb", "room_size": 0.7, "intensity": 0.6}`.
/// Deserialization also accepts the older nested form
/// `{"effect": {"type": "Reverb", ...}, "intensity": 0.6}` and PascalCase
/// or camelCase names for `type` and `filter_type`.
#[derive(Debug, Clone, Serialize)]
pub struct EffectConfig {
    /// Effect type and parameters
    #[serde(flatten)]
    pub effect: EffectType,
    /// Effect intensity/mix level (0.0-1.0)
    pub intensity: f32,
    /// Whether this effect is enabled
    pub enabled: bool,
}

/// The strict flat representation; `EffectConfig` normalizes into this.
#[derive(Deserialize)]
struct EffectConfigRepr {
    #[serde(flatten)]
    effect: EffectType,
    #[serde(default = "default_effect_intensity")]
    intensity: f32,
    #[serde(default = "default_true")]
    enabled: bool,
}

/// "LowPass" / "lowPass" / "lowpass" / "low_pass" → "low_pass".
fn normalize_variant_name(raw: &str, snake_variants: &[&str]) -> String {
    let squashed: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    snake_variants
        .iter()
        .find(|v| v.replace('_', "") == squashed)
        .map(|v| v.to_string())
        .unwrap_or_else(|| raw.to_string())
}

const EFFECT_TYPE_NAMES: [&str; 6] = [
    "reverb",
    "delay",
    "chorus",
    "filter",
    "compressor",
    "distortion",
];
const FILTER_TYPE_NAMES: [&str; 7] = [
    "low_pass",
    "high_pass",
    "band_pass",
    "notch",
    "peak",
    "low_shelf",
    "high_shelf",
];

impl<'de> Deserialize<'de> for EffectConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let Some(object) = value.as_object_mut() else {
            return Err(serde::de::Error::custom("effect must be an object"));
        };

        // Nested form: hoist the inner "effect" object's fields to the top level.
        if let Some(serde_json::Value::Object(inner)) = object.remove("effect") {
            for (k, v) in inner {
                object.entry(k).or_insert(v);
            }
        }
        if let Some(serde_json::Value::String(t)) = object.get("type") {
            let normalized = normalize_variant_name(t, &EFFECT_TYPE_NAMES);
            object.insert("type".into(), serde_json::Value::String(normalized));
        }
        if let Some(serde_json::Value::String(t)) = object.get("filter_type") {
            let normalized = normalize_variant_name(t, &FILTER_TYPE_NAMES);
            object.insert("filter_type".into(), serde_json::Value::String(normalized));
        }

        let repr: EffectConfigRepr =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(EffectConfig {
            effect: repr.effect,
            intensity: repr.intensity,
            enabled: repr.enabled,
        })
    }
}

fn default_effect_intensity() -> f32 {
    0.5
}

/// Effect types with their specific parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectType {
    /// High-quality reverb effect
    Reverb {
        /// Room size (0.0-1.0, default: 0.5)
        #[serde(default = "default_half")]
        room_size: f32,
        /// High-frequency dampening (0.0-1.0, default: 0.3)
        #[serde(default = "default_dampening")]
        dampening: f32,
        /// Wet signal level (0.0-1.0, default: 0.3)
        #[serde(default = "default_wet_level")]
        wet_level: f32,
        /// Pre-delay in seconds (0.0-0.1, default: 0.02)
        #[serde(default = "default_pre_delay")]
        pre_delay: f32,
    },
    /// Delay/echo effect
    Delay {
        /// Delay time in seconds (0.0-2.0, default: 0.25)
        #[serde(default = "default_delay_time")]
        delay_time: f32,
        /// Feedback amount (0.0-0.95, default: 0.4)
        #[serde(default = "default_feedback")]
        feedback: f32,
        /// Wet signal level (0.0-1.0, default: 0.3)
        #[serde(default = "default_wet_level")]
        wet_level: f32,
        /// Sync to tempo (if true, delay_time is in beats)
        #[serde(default)]
        sync_tempo: bool,
        /// Time Fracture: random delay time between `[min, max]` beats of the
        /// sequence tempo; replaces `delay_time` when present.
        #[serde(default)]
        random_beats: Option<[f32; 2]>,
        /// How fast the random delay time moves, in Hz (0 = fixed at `min`).
        #[serde(default)]
        random_rate: f32,
        /// Semitone shifts applied to successive repeats (up to 12 values).
        #[serde(default)]
        pitch_intervals: Vec<f32>,
        /// Order the intervals are visited in.
        #[serde(default)]
        pitch_mode: PitchMode,
    },
    /// Chorus effect
    Chorus {
        /// LFO rate in Hz (0.1-10.0, default: 1.5)
        #[serde(default = "default_chorus_rate")]
        rate: f32,
        /// Modulation depth (0.0-1.0, default: 0.3)
        #[serde(default = "default_chorus_depth")]
        depth: f32,
        /// Feedback amount (0.0-0.9, default: 0.2)
        #[serde(default = "default_chorus_feedback")]
        feedback: f32,
        /// Stereo width (0.0-1.0, default: 0.7)
        #[serde(default = "default_stereo_width")]
        stereo_width: f32,
    },
    /// Parametric filter
    Filter {
        /// Filter type
        #[serde(default)]
        filter_type: FilterType,
        /// Cutoff frequency in Hz (20-20000, default: 1000)
        #[serde(default = "default_filter_cutoff")]
        cutoff: f32,
        /// Resonance/Q factor (0.0-10.0, default: 1.0)
        #[serde(default = "default_resonance")]
        resonance: f32,
        /// Envelope modulation amount (-1.0 to 1.0, default: 0.0)
        #[serde(default)]
        envelope_amount: f32,
    },
    /// Compressor/limiter
    Compressor {
        /// Threshold in dB (-60.0 to 0.0, default: -12.0)
        #[serde(default = "default_threshold")]
        threshold: f32,
        /// Compression ratio (1.0-20.0, default: 4.0)
        #[serde(default = "default_ratio")]
        ratio: f32,
        /// Attack time in seconds (0.001-1.0, default: 0.01)
        #[serde(default = "default_attack")]
        attack: f32,
        /// Release time in seconds (0.01-10.0, default: 0.1)
        #[serde(default = "default_release")]
        release: f32,
    },
    /// Distortion/overdrive
    Distortion {
        /// Drive amount (0.0-10.0, default: 2.0)
        #[serde(default = "default_drive")]
        drive: f32,
        /// Tone control (0.0-1.0, default: 0.5)
        #[serde(default = "default_half")]
        tone: f32,
        /// Output level (0.0-2.0, default: 1.0)
        #[serde(default = "default_one")]
        output_level: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

/// Order in which Time Fracture picks the next pitch interval for a repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchMode {
    #[default]
    Random,
    Up,
    Down,
    UpDown,
}

impl PitchMode {
    #[allow(dead_code)] // used by the Time Fracture DSP (Task 2) and list_sounds
    pub const ALL: [PitchMode; 4] = [
        PitchMode::Random,
        PitchMode::Up,
        PitchMode::Down,
        PitchMode::UpDown,
    ];

    #[allow(dead_code)] // used by the Time Fracture DSP (Task 2) and list_sounds
    pub fn as_str(&self) -> &'static str {
        match self {
            PitchMode::Random => "random",
            PitchMode::Up => "up",
            PitchMode::Down => "down",
            PitchMode::UpDown => "up_down",
        }
    }
}

// Default value functions for effects
fn default_half() -> f32 {
    0.5
}
fn default_one() -> f32 {
    1.0
}
fn default_dampening() -> f32 {
    0.3
}
fn default_wet_level() -> f32 {
    0.3
}
fn default_pre_delay() -> f32 {
    0.02
}
fn default_delay_time() -> f32 {
    0.25
}
fn default_feedback() -> f32 {
    0.4
}
fn default_chorus_rate() -> f32 {
    1.5
}
fn default_chorus_depth() -> f32 {
    0.3
}
fn default_chorus_feedback() -> f32 {
    0.2
}
fn default_stereo_width() -> f32 {
    0.7
}
fn default_filter_cutoff() -> f32 {
    1000.0
}
fn default_resonance() -> f32 {
    1.0
}
fn default_threshold() -> f32 {
    -12.0
}
fn default_ratio() -> f32 {
    4.0
}
fn default_attack() -> f32 {
    0.01
}
fn default_release() -> f32 {
    0.1
}
fn default_drive() -> f32 {
    2.0
}

fn default_true() -> bool {
    true
}

/// Longest `start_time` or `duration` (in seconds) a single note may ask for.
/// Rendering is sized from these, so an unbounded value would allocate
/// gigabytes and never answer the caller.
pub const MAX_NOTE_SECONDS: f64 = 300.0;

/// Simple note representation that's easy to work with
/// Can represent both MIDI notes and R2D2 expressions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimpleNote {
    /// MIDI note number (0-127, where 60 = middle C) - Optional for R2D2 notes
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub note: Option<u8>,
    /// Velocity (0-127, where 127 = loudest) - Optional for R2D2 notes
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub velocity: Option<u8>,
    /// Start time in seconds (deprecated - use musical_time when possible)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    /// Duration in seconds (deprecated - use musical_duration when possible)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Musical start time (bar.beat.tick notation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub musical_time: Option<MusicalTime>,
    /// Musical duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub musical_duration: Option<MusicalDuration>,
    /// MIDI channel (0-15)
    #[serde(default)]
    pub channel: u8,

    /// Note type: "midi" for musical notes, "r2d2" for robotic expressions
    #[serde(default = "default_note_type")]
    pub note_type: String,

    // MIDI-specific parameters (optional)
    /// MIDI instrument (0-127, General MIDI program number, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub instrument: Option<u8>,
    /// Reverb depth (0-127, optional, where 127 = maximum reverb)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub reverb: Option<u8>,
    /// Chorus depth (0-127, optional, where 127 = maximum chorus)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub chorus: Option<u8>,
    /// Channel volume (0-127, optional, where 127 = maximum volume)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub volume: Option<u8>,
    /// Pan position (0-127, optional, where 0 = left, 64 = center, 127 = right)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub pan: Option<u8>,
    /// Balance control (0-127, optional, where 0 = left, 64 = center, 127 = right)
    /// Note: Balance works better than pan for stereo samples
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub balance: Option<u8>,
    /// Expression control (0-127, optional, for dynamic expression)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub expression: Option<u8>,
    /// Sustain pedal (0-127, optional, where 0 = off, 127 = full sustain)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub sustain: Option<u8>,

    // R2D2-specific parameters (optional)
    /// R2D2 emotion: "Happy", "Sad", "Excited", "Worried", "Curious", "Affirmative", "Negative", "Surprised", "Thoughtful"
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub r2d2_emotion: Option<String>,
    /// R2D2 emotional intensity (0.0-1.0)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub r2d2_intensity: Option<f32>,
    /// R2D2 phrase complexity (1-5 syllables)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub r2d2_complexity: Option<u8>,
    /// R2D2 pitch range [min_hz, max_hz]
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub r2d2_pitch_range: Option<Vec<f32>>,
    /// R2D2 context for enhanced expression
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub r2d2_context: Option<String>,

    // NEW: Universal Effects Parameters (compatible with all audio sources)
    /// Effects chain to apply to this note
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub effects: Option<Vec<EffectConfig>>,
    /// Effects preset to apply (e.g., "studio", "concert_hall", "vintage")
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub effects_preset: Option<String>,

    /// Agent-defined synth patch: a name from define_synth / the built-in
    /// library, or an inline patch object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synth: Option<crate::expressive::SynthRef>,
}

fn default_note_type() -> String {
    "midi".to_string()
}

impl Default for SimpleNote {
    /// A one-second MIDI note at time zero with nothing else set.
    fn default() -> Self {
        Self {
            note: None,
            velocity: None,
            start_time: Some(0.0),
            duration: Some(1.0),
            musical_time: None,
            musical_duration: None,
            channel: 0,
            note_type: "midi".to_string(),
            instrument: None,
            reverb: None,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
            r2d2_emotion: None,
            r2d2_intensity: None,
            r2d2_complexity: None,
            r2d2_pitch_range: None,
            r2d2_context: None,
            effects: None,
            effects_preset: None,
            synth: None,
        }
    }
}

/// Simple sequence of notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleSequence {
    pub notes: Vec<SimpleNote>,
    /// Tempo in BPM (optional, defaults to 120)
    #[serde(default = "default_tempo")]
    pub tempo: u32,
    /// Time signature numerator (beats per bar), defaults to 4
    #[serde(default = "default_beats_per_bar")]
    pub beats_per_bar: u32,
}

fn default_tempo() -> u32 {
    120
}

impl SimpleSequence {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    /// Add a note to the sequence
    #[allow(dead_code)]
    pub fn add_note(
        &mut self,
        note: u8,
        velocity: u8,
        start_time: f64,
        duration: f64,
    ) -> &mut Self {
        self.notes.push(SimpleNote {
            note: Some(note),
            velocity: Some(velocity),
            start_time: Some(start_time),
            duration: Some(duration),
            ..Default::default()
        });
        self
    }

    /// Add a note with channel
    #[allow(dead_code)]
    pub fn add_note_with_channel(
        &mut self,
        note: u8,
        velocity: u8,
        start_time: f64,
        duration: f64,
        channel: u8,
    ) -> &mut Self {
        self.notes.push(SimpleNote {
            note: Some(note),
            velocity: Some(velocity),
            start_time: Some(start_time),
            duration: Some(duration),
            channel,
            ..Default::default()
        });
        self
    }

    /// Add a note with channel and instrument
    #[allow(dead_code)]
    pub fn add_note_with_instrument(
        &mut self,
        note: u8,
        velocity: u8,
        start_time: f64,
        duration: f64,
        channel: u8,
        instrument: u8,
    ) -> &mut Self {
        self.notes.push(SimpleNote {
            note: Some(note),
            velocity: Some(velocity),
            start_time: Some(start_time),
            duration: Some(duration),
            channel,
            instrument: Some(instrument),
            ..Default::default()
        });
        self
    }

    /// Create a simple melody with equal note durations
    #[allow(dead_code)]
    pub fn melody(notes: &[u8], note_duration: f64, velocity: u8) -> Self {
        let mut sequence = Self::new();
        for (i, &note) in notes.iter().enumerate() {
            sequence.add_note(note, velocity, i as f64 * note_duration, note_duration);
        }
        sequence
    }

    /// Create a chord (notes played simultaneously)
    #[allow(dead_code)]
    pub fn chord(notes: &[u8], start_time: f64, duration: f64, velocity: u8) -> Self {
        let mut sequence = Self::new();
        for &note in notes {
            sequence.add_note(note, velocity, start_time, duration);
        }
        sequence
    }

    /// Add an R2D2 expression to the sequence
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_r2d2_expression(
        &mut self,
        emotion: &str,
        intensity: f32,
        start_time: f64,
        duration: f64,
        complexity: u8,
        pitch_range: Option<Vec<f32>>,
        context: Option<String>,
    ) -> &mut Self {
        self.notes.push(SimpleNote {
            start_time: Some(start_time),
            duration: Some(duration),
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some(emotion.to_string()),
            r2d2_intensity: Some(intensity),
            r2d2_complexity: Some(complexity),
            r2d2_pitch_range: pitch_range,
            r2d2_context: context,
            ..Default::default()
        });
        self
    }
}

/// Named sequence pattern that can be reused with transformations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencePattern {
    /// Pattern name/identifier
    pub name: String,
    /// Description of what this pattern represents
    pub description: Option<String>,
    /// The notes in this pattern
    pub notes: Vec<SimpleNote>,
    /// Default tempo for this pattern (can be overridden)
    #[serde(default = "default_tempo")]
    pub tempo: u32,
    /// Pattern length in bars (ensures proper looping/alignment)
    #[serde(default = "default_pattern_bars")]
    pub pattern_bars: f64,
    /// Time signature (beats per bar)
    #[serde(default = "default_beats_per_bar")]
    pub beats_per_bar: u32,
    /// Quantization grid for this pattern
    #[serde(default)]
    pub quantize_grid: QuantizeGrid,
    /// Pattern category for organization (e.g., "drums", "bass", "melody")
    pub category: Option<String>,
    /// Tags for searching/filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_pattern_bars() -> f64 {
    4.0
}

fn default_beats_per_bar() -> u32 {
    4
}

/// Reference to a sequence pattern with transformations applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceReference {
    /// Name of the pattern to reference
    pub pattern_name: String,
    /// Start time offset for this pattern instance (seconds - deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_offset: Option<f64>,
    /// Musical start position (bar number, 1-based)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_bar: Option<u32>,
    /// Start on specific beat within the bar (1-based)
    #[serde(default = "default_start_beat")]
    pub start_beat: u32,
    /// Specific bars to play this pattern on (e.g., [1, 5, 9, 13])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bars: Option<Vec<u32>>,
    /// Transpose all notes by this many semitones (-12 to +12)
    #[serde(default)]
    pub transpose: i8,
    /// Override instrument for all MIDI notes in this pattern
    pub instrument_override: Option<u8>,
    /// Scale all velocities by this factor (0.1 to 2.0)
    #[serde(default = "default_one")]
    pub velocity_scale: f32,
    /// Scale all durations by this factor (0.1 to 4.0)
    #[serde(default = "default_one")]
    pub duration_scale: f32,
    /// Override channel for all notes in this pattern
    pub channel_override: Option<u8>,
    /// Number of times to repeat this pattern (ignored if 'bars' is specified)
    #[serde(default = "default_repeat_count")]
    pub repeat_count: u32,
    /// Time between repeats in bars (musical spacing)
    #[serde(default)]
    pub repeat_spacing_bars: f64,
    /// Align pattern to bar boundaries
    #[serde(default = "default_true")]
    pub align_to_bars: bool,
}

fn default_start_beat() -> u32 {
    1
}

fn default_repeat_count() -> u32 {
    1
}

/// Extended sequence that supports both individual notes and pattern references
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedSequence {
    /// Individual notes (existing functionality)
    #[serde(default)]
    pub notes: Vec<SimpleNote>,
    /// Pattern references with transformations
    #[serde(default)]
    pub patterns: Vec<SequenceReference>,
    /// Tempo in BPM (optional, defaults to 120)
    #[serde(default = "default_tempo")]
    pub tempo: u32,
    /// Time signature numerator (beats per bar), defaults to 4
    #[serde(default = "default_beats_per_bar")]
    pub beats_per_bar: u32,
}

impl SequencePattern {
    #[allow(dead_code)]
    pub fn new(name: String, notes: Vec<SimpleNote>) -> Self {
        Self {
            name,
            description: None,
            notes,
            tempo: 120,
            pattern_bars: 4.0,
            beats_per_bar: 4,
            quantize_grid: QuantizeGrid::Off,
            category: None,
            tags: Vec::new(),
        }
    }

    /// Snap every note with a musical position to this pattern's quantize grid.
    pub fn quantize_notes(&mut self) {
        if self.quantize_grid == QuantizeGrid::Off {
            return;
        }
        for note in &mut self.notes {
            if let Some(time) = &note.musical_time {
                note.musical_time = Some(self.quantize_grid.apply(time, 480, self.beats_per_bar));
            }
        }
    }

    /// Apply transformations to create a concrete sequence of notes
    pub fn apply_reference(
        &self,
        reference: &SequenceReference,
        sequence_tempo: u32,
        sequence_beats_per_bar: u32,
    ) -> Result<Vec<SimpleNote>, String> {
        let mut transformed_notes = Vec::new();

        // Determine where to place pattern instances
        let placements: Vec<(u32, u32)> = if let Some(bars) = &reference.bars {
            // Use specific bar positions
            bars.iter()
                .map(|&bar| (bar, reference.start_beat))
                .collect()
        } else if let Some(start_bar) = reference.start_bar {
            // Use start_bar with repeat count
            (0..reference.repeat_count)
                .map(|i| {
                    let bar_offset = i as f64 * (self.pattern_bars + reference.repeat_spacing_bars);
                    ((start_bar as f64 + bar_offset) as u32, reference.start_beat)
                })
                .collect()
        } else if let Some(start_offset) = reference.start_time_offset {
            // Fallback to legacy seconds-based timing
            return self.apply_reference_legacy(
                reference,
                start_offset,
                sequence_tempo,
                sequence_beats_per_bar,
            );
        } else {
            // Default: start at bar 1
            (0..reference.repeat_count)
                .map(|i| {
                    let bar_offset = i as f64 * (self.pattern_bars + reference.repeat_spacing_bars);
                    ((1.0 + bar_offset) as u32, reference.start_beat)
                })
                .collect()
        };

        // Process each pattern placement
        for (bar, beat) in placements {
            let bar_start_time =
                self.calculate_bar_start_time(bar, sequence_tempo, sequence_beats_per_bar);
            let beat_offset = ((beat - 1) as f64 / sequence_beats_per_bar as f64)
                * (60.0 / sequence_tempo as f64)
                * sequence_beats_per_bar as f64;
            let placement_start_time = bar_start_time + beat_offset;

            for note in &self.notes {
                let mut transformed_note = note.clone();

                // Convert musical time to seconds if needed - use pattern's own tempo for internal timing
                let note_start_offset = note.get_start_time(self.tempo, self.beats_per_bar);
                let note_duration = note.get_duration(self.tempo, self.beats_per_bar);

                // Apply timing transformation
                if let Some(musical_time) = &transformed_note.musical_time {
                    // Calculate the note's absolute position in the sequence
                    // Pattern placement bar + note's relative position within pattern
                    let note_relative_beats =
                        (musical_time.bar - 1) * self.beats_per_bar + (musical_time.beat - 1);
                    let note_relative_beat_fraction = musical_time.tick as f64 / 480.0; // Convert ticks to beat fraction

                    let absolute_bar = bar + (note_relative_beats / sequence_beats_per_bar);
                    let absolute_beat = beat - 1 + (note_relative_beats % sequence_beats_per_bar);

                    // Ensure we don't exceed beats per bar
                    let final_bar = absolute_bar + (absolute_beat / sequence_beats_per_bar);
                    let final_beat = (absolute_beat % sequence_beats_per_bar) + 1;

                    // Convert to seconds for now (since the player expects seconds)
                    let absolute_beats = (final_bar - 1) as f64 * sequence_beats_per_bar as f64
                        + (final_beat - 1) as f64
                        + note_relative_beat_fraction;
                    let seconds_per_beat = 60.0 / sequence_tempo as f64;
                    transformed_note.start_time = Some(absolute_beats * seconds_per_beat);

                    // Clear musical time since we're using seconds
                    transformed_note.musical_time = None;
                } else {
                    // Use seconds-based timing
                    transformed_note.start_time = Some(placement_start_time + note_start_offset);
                }

                // Apply duration scaling
                if let Some(musical_duration) = &transformed_note.musical_duration {
                    match musical_duration {
                        MusicalDuration::Bars(bars) => {
                            transformed_note.musical_duration = Some(MusicalDuration::Bars(
                                bars * reference.duration_scale as f64,
                            ));
                        }
                        MusicalDuration::NoteValue(_) => {
                            // Note values cannot be scaled symbolically; fall back to seconds.
                            transformed_note.musical_duration = None;
                            transformed_note.duration =
                                Some(note_duration * reference.duration_scale as f64);
                        }
                    }
                } else {
                    transformed_note.duration =
                        Some(note_duration * reference.duration_scale as f64);
                }

                // Apply transposition to MIDI notes
                if let Some(midi_note) = transformed_note.note {
                    let new_note =
                        (midi_note as i16 + reference.transpose as i16).clamp(0, 127) as u8;
                    transformed_note.note = Some(new_note);
                }

                // Apply instrument override
                if let Some(instrument) = reference.instrument_override {
                    transformed_note.instrument = Some(instrument);
                }

                // Apply velocity scaling
                if let Some(velocity) = transformed_note.velocity {
                    let new_velocity =
                        ((velocity as f32 * reference.velocity_scale).clamp(1.0, 127.0)) as u8;
                    transformed_note.velocity = Some(new_velocity);
                }

                // Apply channel override
                if let Some(channel) = reference.channel_override {
                    transformed_note.channel = channel;
                }

                transformed_notes.push(transformed_note);
            }
        }

        Ok(transformed_notes)
    }

    /// Legacy method for seconds-based timing
    fn apply_reference_legacy(
        &self,
        reference: &SequenceReference,
        start_offset: f64,
        sequence_tempo: u32,
        _sequence_beats_per_bar: u32,
    ) -> Result<Vec<SimpleNote>, String> {
        let mut transformed_notes = Vec::new();

        for repeat in 0..reference.repeat_count {
            let repeat_offset = repeat as f64
                * (self.get_pattern_duration()
                    + reference.repeat_spacing_bars
                        * (60.0 / sequence_tempo as f64)
                        * self.beats_per_bar as f64);

            for note in &self.notes {
                let mut transformed_note = note.clone();

                let note_start = note.get_start_time(self.tempo, self.beats_per_bar);
                let note_duration = note.get_duration(self.tempo, self.beats_per_bar);

                transformed_note.start_time = Some(start_offset + repeat_offset + note_start);
                transformed_note.duration = Some(note_duration * reference.duration_scale as f64);

                // Apply other transformations...
                if let Some(midi_note) = transformed_note.note {
                    let new_note =
                        (midi_note as i16 + reference.transpose as i16).clamp(0, 127) as u8;
                    transformed_note.note = Some(new_note);
                }

                if let Some(instrument) = reference.instrument_override {
                    transformed_note.instrument = Some(instrument);
                }

                if let Some(velocity) = transformed_note.velocity {
                    let new_velocity =
                        ((velocity as f32 * reference.velocity_scale).clamp(1.0, 127.0)) as u8;
                    transformed_note.velocity = Some(new_velocity);
                }

                if let Some(channel) = reference.channel_override {
                    transformed_note.channel = channel;
                }

                transformed_notes.push(transformed_note);
            }
        }

        Ok(transformed_notes)
    }

    /// Calculate start time in seconds for a given bar
    fn calculate_bar_start_time(&self, bar: u32, tempo: u32, beats_per_bar: u32) -> f64 {
        let seconds_per_beat = 60.0 / tempo as f64;
        (bar - 1) as f64 * beats_per_bar as f64 * seconds_per_beat
    }

    /// Calculate the total duration of this pattern in seconds
    pub fn get_pattern_duration(&self) -> f64 {
        // Use pattern_bars if specified, otherwise calculate from notes
        let seconds_per_beat = 60.0 / self.tempo as f64;
        let bar_duration = self.pattern_bars * self.beats_per_bar as f64 * seconds_per_beat;

        // If we have explicit pattern_bars, use that
        if self.pattern_bars > 0.0 {
            bar_duration
        } else {
            // Fallback to calculating from notes
            self.notes
                .iter()
                .map(|note| {
                    let start = note.get_start_time(self.tempo, self.beats_per_bar);
                    let duration = note.get_duration(self.tempo, self.beats_per_bar);
                    start + duration
                })
                .fold(0.0, f64::max)
        }
    }
}

impl ExtendedSequence {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            patterns: Vec::new(),
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    /// Convert to SimpleSequence by resolving all pattern references
    pub fn resolve_patterns(
        &self,
        pattern_store: &std::collections::HashMap<String, SequencePattern>,
    ) -> Result<SimpleSequence, String> {
        let mut all_notes = self.notes.clone();

        // Resolve all pattern references
        for pattern_ref in &self.patterns {
            let pattern = pattern_store
                .get(&pattern_ref.pattern_name)
                .ok_or_else(|| format!("Pattern '{}' not found", pattern_ref.pattern_name))?;

            let resolved_notes =
                pattern.apply_reference(pattern_ref, self.tempo, self.beats_per_bar)?;
            all_notes.extend(resolved_notes);
        }

        // Sort notes by start time for proper playback order
        all_notes.sort_by(|a, b| {
            let a_time = a.get_start_time(self.tempo, self.beats_per_bar);
            let b_time = b.get_start_time(self.tempo, self.beats_per_bar);
            a_time
                .partial_cmp(&b_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(SimpleSequence {
            notes: all_notes,
            tempo: self.tempo,
            beats_per_bar: self.beats_per_bar,
        })
    }
}

impl SimpleNote {
    /// Get start time in seconds, converting from musical time if needed
    pub fn get_start_time(&self, tempo: u32, beats_per_bar: u32) -> f64 {
        if let Some(musical_time) = &self.musical_time {
            musical_time.to_seconds(tempo, beats_per_bar, 480) // Using standard 480 PPQ
        } else {
            self.start_time.unwrap_or(0.0)
        }
    }

    /// Get duration in seconds, converting from musical duration if needed
    pub fn get_duration(&self, tempo: u32, beats_per_bar: u32) -> f64 {
        if let Some(musical_duration) = &self.musical_duration {
            musical_duration.to_seconds(tempo, beats_per_bar)
        } else {
            self.duration.unwrap_or(1.0)
        }
    }

    /// Check if this note is an R2D2 expression
    pub fn is_r2d2(&self) -> bool {
        self.note_type == "r2d2"
    }

    /// Check if this note renders through an agent-defined synth patch.
    pub fn is_synthesis(&self) -> bool {
        self.synth.is_some()
    }

    /// Check if this note has effects
    pub fn has_effects(&self) -> bool {
        self.effects.is_some() || self.effects_preset.is_some()
    }

    /// Validate R2D2 parameters if this is an R2D2 note
    pub fn validate_r2d2(&self) -> Result<(), String> {
        if !self.is_r2d2() {
            return Ok(());
        }

        // Check required R2D2 parameters
        if self.r2d2_emotion.is_none() {
            return Err("R2D2 note requires 'r2d2_emotion' parameter".to_string());
        }

        if self.r2d2_intensity.is_none() {
            return Err("R2D2 note requires 'r2d2_intensity' parameter".to_string());
        }

        if self.r2d2_complexity.is_none() {
            return Err("R2D2 note requires 'r2d2_complexity' parameter".to_string());
        }

        // Validate emotion
        let emotion = self.r2d2_emotion.as_ref().unwrap();
        let valid_emotions = [
            "Happy",
            "Sad",
            "Excited",
            "Worried",
            "Curious",
            "Affirmative",
            "Negative",
            "Surprised",
            "Thoughtful",
        ];
        if !valid_emotions.contains(&emotion.as_str()) {
            return Err(format!(
                "Invalid R2D2 emotion '{}'. Valid emotions: {:?}",
                emotion, valid_emotions
            ));
        }

        // Validate intensity
        let intensity = self.r2d2_intensity.unwrap();
        if !(0.0..=1.0).contains(&intensity) {
            return Err(format!(
                "R2D2 intensity must be between 0.0 and 1.0, got {}",
                intensity
            ));
        }

        // Validate complexity
        let complexity = self.r2d2_complexity.unwrap();
        if !(1..=5).contains(&complexity) {
            return Err(format!(
                "R2D2 complexity must be between 1 and 5, got {}",
                complexity
            ));
        }

        // Validate pitch range if provided
        if let Some(range) = &self.r2d2_pitch_range {
            if range.len() != 2 {
                return Err("R2D2 pitch range must be a vector of length 2".to_string());
            }
            let (min, max) = (range[0], range[1]);
            if min >= max {
                return Err(format!(
                    "R2D2 pitch_range min ({}) must be less than pitch_range max ({})",
                    min, max
                ));
            }
            if min < 50.0 || max > 2000.0 {
                return Err("R2D2 pitch range should be between 50Hz and 2000Hz".to_string());
            }
        }

        Ok(())
    }

    /// Reject note lengths and offsets that would render an unbounded buffer.
    pub fn validate_timing(&self) -> Result<(), String> {
        for (name, value) in [("duration", self.duration), ("start_time", self.start_time)] {
            if let Some(v) = value
                && v > MAX_NOTE_SECONDS
            {
                return Err(format!(
                    "{} must be at most {} seconds, got {}",
                    name, MAX_NOTE_SECONDS, v
                ));
            }
        }
        Ok(())
    }

    /// Validate the patch reference: an inline patch must validate, and R2D2 keeps its own voice.
    pub fn validate_synth(&self) -> Result<(), String> {
        match &self.synth {
            None => Ok(()),
            Some(_) if self.note_type == "r2d2" => {
                Err("a note cannot have both note_type \"r2d2\" and synth".to_string())
            }
            // A patch renders through its own chain, so a note-level chain
            // would be silently dropped; say so instead.
            Some(_) if self.effects.is_some() || self.effects_preset.is_some() => Err(
                "effects on a synth note belong in the patch's \"effects\" chain (define_synth, or an inline patch); remove effects/effects_preset from the note"
                    .to_string(),
            ),
            Some(crate::expressive::SynthRef::Inline(p)) => p.validate(),
            Some(crate::expressive::SynthRef::Name(n)) if n.trim().is_empty() => {
                Err("synth name must not be empty".to_string())
            }
            Some(_) => Ok(()),
        }
    }

    /// Validate MIDI note parameters and effects parameters if this note has effects
    pub fn validate_effects(&self) -> Result<(), String> {
        if !self.has_effects() {
            return Ok(());
        }

        // Validate effects chain
        if let Some(effects) = &self.effects {
            for (i, effect) in effects.iter().enumerate() {
                if let Err(e) = self.validate_single_effect(effect) {
                    return Err(format!("Effect {} in chain: {}", i + 1, e));
                }
            }
        }

        // Validate effects preset (use actual library presets)
        if let Some(preset) = &self.effects_preset {
            use crate::expressive::EffectsPresetLibrary;
            let library = EffectsPresetLibrary::new();
            if library.get_preset(preset).is_none() {
                let valid_presets: Vec<String> =
                    library.get_preset_names().into_iter().cloned().collect();
                return Err(format!(
                    "Invalid effects preset '{}'. Valid presets: {:?}",
                    preset, valid_presets
                ));
            }
        }

        Ok(())
    }

    /// Validate a single effect configuration
    fn validate_single_effect(&self, effect: &EffectConfig) -> Result<(), String> {
        effect.validate_effect_config()
    }
}

impl EffectConfig {
    /// Validate this effect's intensity and effect-specific parameters.
    pub fn validate_effect_config(&self) -> Result<(), String> {
        // Validate intensity
        if !(0.0..=1.0).contains(&self.intensity) {
            return Err(format!(
                "Effect intensity {} is out of range (0.0-1.0)",
                self.intensity
            ));
        }

        // Validate effect-specific parameters
        match &self.effect {
            EffectType::Reverb {
                room_size,
                dampening,
                wet_level,
                pre_delay,
            } => {
                if !(0.0..=1.0).contains(room_size) {
                    return Err(format!(
                        "Reverb room_size {} is out of range (0.0-1.0)",
                        room_size
                    ));
                }
                if !(0.0..=1.0).contains(dampening) {
                    return Err(format!(
                        "Reverb dampening {} is out of range (0.0-1.0)",
                        dampening
                    ));
                }
                if !(0.0..=1.0).contains(wet_level) {
                    return Err(format!(
                        "Reverb wet_level {} is out of range (0.0-1.0)",
                        wet_level
                    ));
                }
                if !(0.0..=0.2).contains(pre_delay) {
                    return Err(format!(
                        "Reverb pre_delay {} is out of range (0.0-0.2 seconds)",
                        pre_delay
                    ));
                }
            }
            EffectType::Delay {
                delay_time,
                feedback,
                wet_level,
                sync_tempo: _,
                random_beats,
                random_rate,
                pitch_intervals,
                pitch_mode: _,
            } => {
                if !(0.001..=3.0).contains(delay_time) {
                    return Err(format!(
                        "Delay delay_time {} is out of range (0.001-3.0 seconds)",
                        delay_time
                    ));
                }
                if !(0.0..=0.95).contains(feedback) {
                    return Err(format!(
                        "Delay feedback {} is out of range (0.0-0.95)",
                        feedback
                    ));
                }
                if !(0.0..=1.0).contains(wet_level) {
                    return Err(format!(
                        "Delay wet_level {} is out of range (0.0-1.0)",
                        wet_level
                    ));
                }
                if let Some([min, max]) = random_beats {
                    for (i, v) in [min, max].into_iter().enumerate() {
                        if !(0.0..=4.0).contains(v) {
                            return Err(format!(
                                "Delay random_beats[{i}] {v} is out of range (0-4 beats)"
                            ));
                        }
                    }
                    if min > max {
                        return Err(format!(
                            "Delay random_beats min {min} must not exceed max {max}"
                        ));
                    }
                }
                if !(0.0..=10.0).contains(random_rate) {
                    return Err(format!(
                        "Delay random_rate {random_rate} is out of range (0-10 Hz)"
                    ));
                }
                if pitch_intervals.len() > 12 {
                    return Err(format!(
                        "Delay pitch_intervals has {} values; at most 12",
                        pitch_intervals.len()
                    ));
                }
                for (i, s) in pitch_intervals.iter().enumerate() {
                    if !(-12.0..=12.0).contains(s) {
                        return Err(format!(
                            "Delay pitch_intervals[{i}] {s} is out of range (-12 to 12 semitones)"
                        ));
                    }
                }
            }
            EffectType::Chorus {
                rate,
                depth,
                feedback,
                stereo_width,
            } => {
                if !(0.1..=20.0).contains(rate) {
                    return Err(format!(
                        "Chorus rate {} is out of range (0.1-20.0 Hz)",
                        rate
                    ));
                }
                if !(0.0..=1.0).contains(depth) {
                    return Err(format!("Chorus depth {} is out of range (0.0-1.0)", depth));
                }
                if !(0.0..=0.9).contains(feedback) {
                    return Err(format!(
                        "Chorus feedback {} is out of range (0.0-0.9)",
                        feedback
                    ));
                }
                if !(0.0..=1.0).contains(stereo_width) {
                    return Err(format!(
                        "Chorus stereo_width {} is out of range (0.0-1.0)",
                        stereo_width
                    ));
                }
            }
            EffectType::Filter {
                filter_type: _,
                cutoff,
                resonance,
                envelope_amount,
            } => {
                if !(20.0..=20000.0).contains(cutoff) {
                    return Err(format!(
                        "Filter cutoff {} is out of range (20-20000 Hz)",
                        cutoff
                    ));
                }
                if !(0.0..=20.0).contains(resonance) {
                    return Err(format!(
                        "Filter resonance {} is out of range (0.0-20.0)",
                        resonance
                    ));
                }
                if !(-1.0..=1.0).contains(envelope_amount) {
                    return Err(format!(
                        "Filter envelope_amount {} is out of range (-1.0 to 1.0)",
                        envelope_amount
                    ));
                }
            }
            EffectType::Compressor {
                threshold,
                ratio,
                attack,
                release,
            } => {
                if !(-60.0..=0.0).contains(threshold) {
                    return Err(format!(
                        "Compressor threshold {} is out of range (-60.0 to 0.0 dB)",
                        threshold
                    ));
                }
                if !(1.0..=50.0).contains(ratio) {
                    return Err(format!(
                        "Compressor ratio {} is out of range (1.0-50.0)",
                        ratio
                    ));
                }
                if !(0.0001..=2.0).contains(attack) {
                    return Err(format!(
                        "Compressor attack {} is out of range (0.0001-2.0 seconds)",
                        attack
                    ));
                }
                if !(0.001..=20.0).contains(release) {
                    return Err(format!(
                        "Compressor release {} is out of range (0.001-20.0 seconds)",
                        release
                    ));
                }
            }
            EffectType::Distortion {
                drive,
                tone,
                output_level,
            } => {
                if !(0.0..=20.0).contains(drive) {
                    return Err(format!(
                        "Distortion drive {} is out of range (0.0-20.0)",
                        drive
                    ));
                }
                if !(0.0..=1.0).contains(tone) {
                    return Err(format!(
                        "Distortion tone {} is out of range (0.0-1.0)",
                        tone
                    ));
                }
                if !(0.0..=3.0).contains(output_level) {
                    return Err(format!(
                        "Distortion output_level {} is out of range (0.0-3.0)",
                        output_level
                    ));
                }
            }
        }

        Ok(())
    }

    /// Seconds of silence to render after the last note so this effect can ring out.
    #[allow(dead_code)] // wired into render_patch's tail calculation in Task 2
    pub fn tail_seconds(&self, tempo: u32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        match &self.effect {
            EffectType::Reverb { .. } => 1.0,
            EffectType::Delay {
                delay_time,
                sync_tempo,
                random_beats,
                ..
            } => {
                let longest = match random_beats {
                    Some([_, max]) => max * seconds_per_beat,
                    None if *sync_tempo => delay_time * seconds_per_beat,
                    None => *delay_time,
                };
                (4.0 * longest + 0.5).max(1.0)
            }
            _ => 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn notes_reject_unknown_and_removed_fields_by_name() {
        for (field, value) in [
            ("synth_type", json!("sine")),
            ("preset_name", json!("Minimoog Bass")),
            ("synth_attack", json!(0.1)),
            ("colour", json!("blue")),
        ] {
            let mut v = json!({"note": 60, "start_time": 0.0, "duration": 1.0});
            v[field] = value;
            let err = serde_json::from_value::<SimpleNote>(v)
                .unwrap_err()
                .to_string();
            assert!(err.contains(field), "{field}: {err}");
        }
    }

    #[test]
    fn r2d2_and_midi_notes_still_parse_with_every_documented_field() {
        let r2d2 = json!({"note_type": "r2d2", "r2d2_emotion": "Happy", "r2d2_intensity": 0.8,
            "r2d2_complexity": 2, "r2d2_pitch_range": [200.0, 800.0], "r2d2_context": "hi",
            "start_time": 0.0, "duration": 1.0, "effects": [{"type": "reverb"}]});
        assert!(serde_json::from_value::<SimpleNote>(r2d2).is_ok());
        let midi = json!({"note": 60, "velocity": 90, "channel": 2, "instrument": 5, "reverb": 40,
            "chorus": 10, "volume": 100, "pan": 64, "balance": 64, "expression": 100, "sustain": 0,
            "musical_time": {"bar": 1, "beat": 1, "tick": 0}, "musical_duration": "quarter",
            "effects_preset": "studio"});
        assert!(serde_json::from_value::<SimpleNote>(midi).is_ok());
    }

    #[test]
    fn a_duration_over_the_limit_is_rejected_by_name() {
        let long = SimpleNote {
            duration: Some(100_000.0),
            ..Default::default()
        };
        let err = long.validate_timing().unwrap_err();
        assert!(err.contains("duration") && err.contains("300"), "{err}");

        let late = SimpleNote {
            start_time: Some(MAX_NOTE_SECONDS + 1.0),
            ..Default::default()
        };
        let err = late.validate_timing().unwrap_err();
        assert!(err.contains("start_time") && err.contains("300"), "{err}");

        let ok = SimpleNote {
            start_time: Some(10.0),
            duration: Some(MAX_NOTE_SECONDS),
            ..Default::default()
        };
        assert!(ok.validate_timing().is_ok());
    }

    #[test]
    fn effects_on_a_synth_note_are_rejected() {
        let synth: crate::expressive::SynthRef = serde_json::from_value(json!("sub_bass")).unwrap();
        let with_chain = SimpleNote {
            synth: Some(synth.clone()),
            effects: Some(vec![serde_json::from_str(r#"{"type": "reverb"}"#).unwrap()]),
            ..Default::default()
        };
        let err = with_chain.validate_synth().unwrap_err();
        assert!(
            err.contains("effects") && err.contains("define_synth"),
            "{err}"
        );

        let with_preset = SimpleNote {
            synth: Some(synth.clone()),
            effects_preset: Some("studio".into()),
            ..Default::default()
        };
        assert!(with_preset.validate_synth().is_err());

        // MIDI and R2D2 notes keep both fields.
        let midi = SimpleNote {
            effects_preset: Some("studio".into()),
            ..Default::default()
        };
        assert!(midi.validate_synth().is_ok());
        let plain_synth = SimpleNote {
            synth: Some(synth),
            ..Default::default()
        };
        assert!(plain_synth.validate_synth().is_ok());
    }

    #[test]
    fn effect_config_accepts_flat_snake_case() {
        let e: EffectConfig =
            serde_json::from_str(r#"{"type": "reverb", "room_size": 0.7, "intensity": 0.6}"#)
                .unwrap();
        assert!(matches!(e.effect, EffectType::Reverb { room_size, .. } if room_size == 0.7));
        assert_eq!(e.intensity, 0.6);
        assert!(e.enabled);
    }

    #[test]
    fn delay_accepts_time_fracture_fields_with_defaults() {
        let e: EffectConfig = serde_json::from_str(r#"{"type": "delay"}"#).unwrap();
        match &e.effect {
            EffectType::Delay {
                random_beats,
                random_rate,
                pitch_intervals,
                pitch_mode,
                ..
            } => {
                assert!(random_beats.is_none());
                assert_eq!(*random_rate, 0.0);
                assert!(pitch_intervals.is_empty());
                assert_eq!(*pitch_mode, PitchMode::Random);
            }
            other => panic!("not a delay: {other:?}"),
        }
        let e: EffectConfig = serde_json::from_str(
            r#"{"type": "delay", "random_beats": [0.25, 1.0], "random_rate": 2.0,
                "pitch_intervals": [7, 12], "pitch_mode": "up_down", "feedback": 0.5}"#,
        )
        .unwrap();
        assert!(e.validate_effect_config().is_ok());
        match &e.effect {
            EffectType::Delay {
                random_beats,
                pitch_intervals,
                pitch_mode,
                ..
            } => {
                assert_eq!(*random_beats, Some([0.25, 1.0]));
                assert_eq!(pitch_intervals, &vec![7.0, 12.0]);
                assert_eq!(*pitch_mode, PitchMode::UpDown);
            }
            other => panic!("not a delay: {other:?}"),
        }
        assert!(
            serde_json::from_str::<EffectConfig>(r#"{"type": "delay", "pitch_mode": "spiral"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<EffectConfig>(r#"{"type": "delay", "min_beats": 1}"#).is_err()
        );
    }

    #[test]
    fn time_fracture_validation_names_the_field_and_range() {
        let cases = [
            (
                r#"{"type": "delay", "random_beats": [1.0, 5.0]}"#,
                "random_beats",
                "4",
            ),
            (
                r#"{"type": "delay", "random_beats": [2.0, 1.0]}"#,
                "random_beats",
                "min",
            ),
            (
                r#"{"type": "delay", "random_rate": 50}"#,
                "random_rate",
                "10",
            ),
            (
                r#"{"type": "delay", "pitch_intervals": [30]}"#,
                "pitch_intervals",
                "12",
            ),
            (
                r#"{"type": "delay", "pitch_intervals": [1,2,3,4,5,6,7,8,9,10,11,12,13]}"#,
                "pitch_intervals",
                "12",
            ),
        ];
        for (json, field, needle) in cases {
            let e: EffectConfig = serde_json::from_str(json).unwrap();
            let err = e.validate_effect_config().unwrap_err();
            assert!(err.contains(field) && err.contains(needle), "{json}: {err}");
        }
    }

    #[test]
    fn effect_tail_seconds_follows_the_longest_delay() {
        let parse = |s: &str| serde_json::from_str::<EffectConfig>(s).unwrap();
        assert_eq!(parse(r#"{"type": "reverb"}"#).tail_seconds(120), 1.0);
        assert_eq!(parse(r#"{"type": "chorus"}"#).tail_seconds(120), 0.5);
        // 0.25 s static delay: 4 x 0.25 + 0.5 = 1.5
        assert!(
            (parse(r#"{"type": "delay", "delay_time": 0.25}"#).tail_seconds(120) - 1.5).abs()
                < 1e-6
        );
        // 2 beats at 60 BPM = 2 s: 4 x 2 + 0.5 = 8.5
        assert!(
            (parse(r#"{"type": "delay", "random_beats": [0.5, 2.0]}"#).tail_seconds(60) - 8.5)
                .abs()
                < 1e-6
        );
        // sync_tempo: delay_time is in beats: 1 beat at 120 = 0.5 s -> 2.5
        assert!(
            (parse(r#"{"type": "delay", "delay_time": 1.0, "sync_tempo": true}"#)
                .tail_seconds(120)
                - 2.5)
                .abs()
                < 1e-6
        );
        let mut off = parse(r#"{"type": "reverb"}"#);
        off.enabled = false;
        assert_eq!(off.tail_seconds(120), 0.0);
    }

    #[test]
    fn effect_config_accepts_nested_pascal_case_from_the_old_schema() {
        let e: EffectConfig = serde_json::from_str(
            r#"{"effect": {"type": "Filter", "filter_type": "LowPass", "cutoff": 800.0}, "intensity": 0.4}"#,
        )
        .unwrap();
        match e.effect {
            EffectType::Filter {
                filter_type: FilterType::LowPass,
                cutoff,
                ..
            } => assert_eq!(cutoff, 800.0),
            other => panic!("unexpected {other:?}"),
        }
        let e: EffectConfig =
            serde_json::from_str(r#"{"type": "Distortion", "drive": 2.0}"#).unwrap();
        assert!(matches!(e.effect, EffectType::Distortion { .. }));
        assert_eq!(e.intensity, 0.5, "intensity defaults");
    }

    #[test]
    fn effect_config_rejects_unknown_type_with_a_clear_message() {
        let err = serde_json::from_str::<EffectConfig>(r#"{"type": "flanger"}"#).unwrap_err();
        assert!(err.to_string().contains("flanger"), "{err}");
    }

    #[test]
    fn musical_duration_number_is_bars_and_string_is_note_value() {
        let bars: MusicalDuration = serde_json::from_str("2").unwrap();
        assert!(matches!(bars, MusicalDuration::Bars(b) if b == 2.0));
        assert_eq!(bars.to_seconds(120, 4), 4.0);

        let eighth: MusicalDuration = serde_json::from_str("\"eighth\"").unwrap();
        assert!(matches!(
            eighth,
            MusicalDuration::NoteValue(NoteValue::Eighth)
        ));
        assert_eq!(eighth.to_seconds(120, 4), 0.25);
    }

    #[test]
    fn quantize_grid_snaps_ticks() {
        let t = MusicalTime::new(1, 2, 100);
        assert_eq!(
            QuantizeGrid::Sixteenth.apply(&t, 480, 4),
            MusicalTime::new(1, 2, 120)
        );
        assert_eq!(
            QuantizeGrid::Eighth.apply(&t, 480, 4),
            MusicalTime::new(1, 2, 0)
        );
        assert_eq!(QuantizeGrid::Off.apply(&t, 480, 4), t);
        // 3rd beat of a 4/4 bar rounds up to the next bar line
        assert_eq!(
            QuantizeGrid::Bar.apply(&MusicalTime::new(3, 3, 0), 480, 4),
            MusicalTime::new(4, 1, 0)
        );
        assert_eq!(
            QuantizeGrid::Bar.apply(&MusicalTime::new(3, 2, 0), 480, 4),
            MusicalTime::new(3, 1, 0)
        );
    }

    #[test]
    fn pattern_quantizes_its_notes_on_request() {
        let mut pattern = SequencePattern::new(
            "p".to_string(),
            vec![SimpleNote {
                note: Some(60),
                musical_time: Some(MusicalTime::new(1, 1, 100)),
                musical_duration: Some(MusicalDuration::NoteValue(NoteValue::Quarter)),
                start_time: None,
                duration: None,
                ..Default::default()
            }],
        );
        pattern.quantize_grid = QuantizeGrid::Sixteenth;
        pattern.quantize_notes();
        assert_eq!(pattern.notes[0].musical_time.as_ref().unwrap().tick, 120);
    }

    #[test]
    fn pattern_placement_honours_time_signature() {
        // A one-bar pattern in 3/4 at 120 BPM: bar 2 starts at 1.5 s, not 2.0 s.
        let mut pattern = SequencePattern::new(
            "waltz".to_string(),
            vec![SimpleNote {
                note: Some(60),
                start_time: Some(0.0),
                duration: Some(0.5),
                ..Default::default()
            }],
        );
        pattern.pattern_bars = 1.0;
        pattern.beats_per_bar = 3;
        let mut store = std::collections::HashMap::new();
        store.insert("waltz".to_string(), pattern);

        let seq = ExtendedSequence {
            notes: Vec::new(),
            patterns: vec![SequenceReference {
                pattern_name: "waltz".to_string(),
                start_time_offset: None,
                start_bar: Some(2),
                start_beat: 1,
                bars: None,
                transpose: 0,
                instrument_override: None,
                velocity_scale: 1.0,
                duration_scale: 1.0,
                channel_override: None,
                repeat_count: 1,
                repeat_spacing_bars: 0.0,
                align_to_bars: true,
            }],
            tempo: 120,
            beats_per_bar: 3,
        };
        let resolved = seq.resolve_patterns(&store).unwrap();
        assert_eq!(resolved.beats_per_bar, 3);
        assert!((resolved.notes[0].start_time.unwrap() - 1.5).abs() < 1e-9);
    }
}
