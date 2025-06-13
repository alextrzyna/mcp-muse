pub mod parser;
pub mod player;

pub use parser::*;
pub use player::*;

use serde::{Deserialize, Deserializer, Serialize};

/// Custom deserializer that converts null to None for optional fields
fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt)
}

/// Simple note representation that's easy to work with
/// Can represent both MIDI notes and R2D2 expressions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleNote {
    /// MIDI note number (0-127, where 60 = middle C)
    pub note: u8,
    /// Velocity (0-127, where 127 = loudest)
    pub velocity: u8,
    /// Start time in seconds
    pub start_time: f64,
    /// Duration in seconds
    pub duration: f64,
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
}

fn default_note_type() -> String {
    "midi".to_string()
}

/// Simple sequence of notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleSequence {
    pub notes: Vec<SimpleNote>,
    /// Tempo in BPM (optional, defaults to 120)
    #[serde(default = "default_tempo")]
    pub tempo: u32,
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
            note,
            velocity,
            start_time,
            duration,
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
            note,
            velocity,
            start_time,
            duration,
            channel,
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
            note,
            velocity,
            start_time,
            duration,
            channel,
            note_type: "midi".to_string(),
            instrument: Some(instrument),
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
            note: 60,     // Dummy MIDI note (not used for R2D2)
            velocity: 80, // Dummy velocity (not used for R2D2)
            start_time,
            duration,
            channel: 0,
            note_type: "r2d2".to_string(),
            instrument: None,
            reverb: None,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
            r2d2_emotion: Some(emotion.to_string()),
            r2d2_intensity: Some(intensity),
            r2d2_complexity: Some(complexity),
            r2d2_pitch_range: pitch_range,
            r2d2_context: context,
        });
        self
    }
}

impl SimpleNote {
    /// Check if this note is an R2D2 expression
    pub fn is_r2d2(&self) -> bool {
        self.note_type == "r2d2"
    }

    /// Check if this note is a MIDI note
    #[allow(dead_code)]
    pub fn is_midi(&self) -> bool {
        self.note_type == "midi"
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
}
