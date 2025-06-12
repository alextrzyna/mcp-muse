pub mod parser;
pub mod player;

pub use parser::*;
pub use player::*;

use serde::{Deserialize, Serialize};

/// Simple note representation that's easy to work with
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
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            tempo: 120,
        }
    }

    /// Add a note to the sequence
    pub fn add_note(&mut self, note: u8, velocity: u8, start_time: f64, duration: f64) -> &mut Self {
        self.notes.push(SimpleNote {
            note,
            velocity,
            start_time,
            duration,
            channel: 0,
        });
        self
    }

    /// Add a note with channel
    pub fn add_note_with_channel(&mut self, note: u8, velocity: u8, start_time: f64, duration: f64, channel: u8) -> &mut Self {
        self.notes.push(SimpleNote {
            note,
            velocity,
            start_time,
            duration,
            channel,
        });
        self
    }

    /// Create a simple melody with equal note durations
    pub fn melody(notes: &[u8], note_duration: f64, velocity: u8) -> Self {
        let mut sequence = Self::new();
        for (i, &note) in notes.iter().enumerate() {
            sequence.add_note(note, velocity, i as f64 * note_duration, note_duration);
        }
        sequence
    }

    /// Create a chord (notes played simultaneously)
    pub fn chord(notes: &[u8], start_time: f64, duration: f64, velocity: u8) -> Self {
        let mut sequence = Self::new();
        for &note in notes {
            sequence.add_note(note, velocity, start_time, duration);
        }
        sequence
    }
} 