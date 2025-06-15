pub mod parser;
pub mod player;
pub mod polyphonic_source;

pub use player::*;
pub use polyphonic_source::*;

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
    /// MIDI note number (0-127, where 60 = middle C) - Optional for R2D2 notes
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub note: Option<u8>,
    /// Velocity (0-127, where 127 = loudest) - Optional for R2D2 notes
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub velocity: Option<u8>,
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

    // NEW: Synthesis parameters (optional)
    /// Synthesis type: "sine", "square", "sawtooth", "triangle", "noise", "fm", "dx7fm", "granular", "wavetable",
    /// "kick", "snare", "hihat", "cymbal", "swoosh", "zap", "chime", "burst", "pad", "texture", "drone"
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_type: Option<String>,
    /// Synthesis frequency in Hz (20-20000, optional, overrides MIDI note if present)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_frequency: Option<f32>,
    /// Synthesis amplitude (0.0-1.0, optional, defaults to 0.7)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_amplitude: Option<f32>,

    // Synthesis envelope parameters
    /// Attack time in seconds (0.0-5.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_attack: Option<f32>,
    /// Decay time in seconds (0.0-5.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_decay: Option<f32>,
    /// Sustain level (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_sustain: Option<f32>,
    /// Release time in seconds (0.0-10.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_release: Option<f32>,

    // Synthesis filter parameters
    /// Filter type: "lowpass", "highpass", "bandpass" (optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_filter_type: Option<String>,
    /// Filter cutoff frequency in Hz (20-20000, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_filter_cutoff: Option<f32>,
    /// Filter resonance (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_filter_resonance: Option<f32>,

    // Synthesis effects parameters
    /// Reverb intensity (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_reverb: Option<f32>,
    /// Chorus intensity (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_chorus: Option<f32>,
    /// Delay intensity (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_delay: Option<f32>,
    /// Delay time in seconds (0.0-2.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_delay_time: Option<f32>,

    // Synthesis-specific parameters
    /// Pulse width for square wave (0.1-0.9, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_pulse_width: Option<f32>,
    /// FM modulator frequency in Hz (0.1-1000.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_modulator_freq: Option<f32>,
    /// FM modulation index (0.0-10.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_modulation_index: Option<f32>,
    /// Granular grain size in seconds (0.01-0.5, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_grain_size: Option<f32>,
    /// Texture roughness (0.0-1.0, optional)
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub synth_texture_roughness: Option<f32>,

    // NEW: Classic Synthesizer Preset parameters (optional)
    /// Preset name to load (e.g., "Minimoog Bass", "TB-303 Acid")
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub preset_name: Option<String>,
    /// Preset category to select from (e.g., "bass", "pad", "lead")
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub preset_category: Option<String>,
    /// Preset variation to apply (e.g., "bright", "dark")
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub preset_variation: Option<String>,
    /// If true, select random preset from category
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub preset_random: Option<bool>,
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
            note: Some(note),
            velocity: Some(velocity),
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
            synth_type: None,
            synth_frequency: None,
            synth_amplitude: None,
            synth_attack: None,
            synth_decay: None,
            synth_sustain: None,
            synth_release: None,
            synth_filter_type: None,
            synth_filter_cutoff: None,
            synth_filter_resonance: None,
            synth_reverb: None,
            synth_chorus: None,
            synth_delay: None,
            synth_delay_time: None,
            synth_pulse_width: None,
            synth_modulator_freq: None,
            synth_modulation_index: None,
            synth_grain_size: None,
            synth_texture_roughness: None,
            preset_name: None,
            preset_category: None,
            preset_variation: None,
            preset_random: None,
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
            synth_type: None,
            synth_frequency: None,
            synth_amplitude: None,
            synth_attack: None,
            synth_decay: None,
            synth_sustain: None,
            synth_release: None,
            synth_filter_type: None,
            synth_filter_cutoff: None,
            synth_filter_resonance: None,
            synth_reverb: None,
            synth_chorus: None,
            synth_delay: None,
            synth_delay_time: None,
            synth_pulse_width: None,
            synth_modulator_freq: None,
            synth_modulation_index: None,
            synth_grain_size: None,
            synth_texture_roughness: None,
            preset_name: None,
            preset_category: None,
            preset_variation: None,
            preset_random: None,
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
            synth_type: None,
            synth_frequency: None,
            synth_amplitude: None,
            synth_attack: None,
            synth_decay: None,
            synth_sustain: None,
            synth_release: None,
            synth_filter_type: None,
            synth_filter_cutoff: None,
            synth_filter_resonance: None,
            synth_reverb: None,
            synth_chorus: None,
            synth_delay: None,
            synth_delay_time: None,
            synth_pulse_width: None,
            synth_modulator_freq: None,
            synth_modulation_index: None,
            synth_grain_size: None,
            synth_texture_roughness: None,
            preset_name: None,
            preset_category: None,
            preset_variation: None,
            preset_random: None,
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
            note: None,
            velocity: None,
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
            synth_type: None,
            synth_frequency: None,
            synth_amplitude: None,
            synth_attack: None,
            synth_decay: None,
            synth_sustain: None,
            synth_release: None,
            synth_filter_type: None,
            synth_filter_cutoff: None,
            synth_filter_resonance: None,
            synth_reverb: None,
            synth_chorus: None,
            synth_delay: None,
            synth_delay_time: None,
            synth_pulse_width: None,
            synth_modulator_freq: None,
            synth_modulation_index: None,
            synth_grain_size: None,
            synth_texture_roughness: None,
            preset_name: None,
            preset_category: None,
            preset_variation: None,
            preset_random: None,
        });
        self
    }
}

impl SimpleNote {
    /// Check if this note is an R2D2 expression
    pub fn is_r2d2(&self) -> bool {
        self.note_type == "r2d2"
    }

    /// Check if this note is a synthesis note
    pub fn is_synthesis(&self) -> bool {
        self.synth_type.is_some()
    }

    /// Check if this note uses presets
    pub fn is_preset(&self) -> bool {
        self.preset_name.is_some()
            || self.preset_category.is_some()
            || self.preset_random.unwrap_or(false)
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

    /// Validate synthesis parameters if this is a synthesis note
    pub fn validate_synthesis(&self) -> Result<(), String> {
        if !self.is_synthesis() {
            return Ok(());
        }

        let synth_type = self.synth_type.as_ref().unwrap();

        // Validate synthesis type
        let valid_types = [
            "sine",
            "square",
            "sawtooth",
            "triangle",
            "noise",
            "fm",
            "dx7fm",
            "granular",
            "wavetable",
            "kick",
            "snare",
            "hihat",
            "cymbal",
            "swoosh",
            "zap",
            "chime",
            "burst",
            "pad",
            "texture",
            "drone",
        ];

        if !valid_types.contains(&synth_type.as_str()) {
            return Err(format!(
                "Invalid synthesis type: {}. Must be one of: {}",
                synth_type,
                valid_types.join(", ")
            ));
        }

        // Validate frequency range if present
        if let Some(freq) = self.synth_frequency {
            if !(20.0..=20000.0).contains(&freq) {
                return Err(format!(
                    "Synthesis frequency {} is out of range (20-20000 Hz)",
                    freq
                ));
            }
        }

        // Validate amplitude if present
        if let Some(amp) = self.synth_amplitude {
            if !(0.0..=1.0).contains(&amp) {
                return Err(format!(
                    "Synthesis amplitude {} is out of range (0.0-1.0)",
                    amp
                ));
            }
        }

        // Validate envelope parameters
        if let Some(attack) = self.synth_attack {
            if !(0.0..=5.0).contains(&attack) {
                return Err(format!(
                    "Synthesis attack {} is out of range (0.0-5.0 seconds)",
                    attack
                ));
            }
        }

        if let Some(decay) = self.synth_decay {
            if !(0.0..=5.0).contains(&decay) {
                return Err(format!(
                    "Synthesis decay {} is out of range (0.0-5.0 seconds)",
                    decay
                ));
            }
        }

        if let Some(sustain) = self.synth_sustain {
            if !(0.0..=1.0).contains(&sustain) {
                return Err(format!(
                    "Synthesis sustain {} is out of range (0.0-1.0)",
                    sustain
                ));
            }
        }

        if let Some(release) = self.synth_release {
            if !(0.0..=10.0).contains(&release) {
                return Err(format!(
                    "Synthesis release {} is out of range (0.0-10.0 seconds)",
                    release
                ));
            }
        }

        // Validate filter parameters
        if let Some(filter_type) = &self.synth_filter_type {
            let valid_filter_types = ["lowpass", "highpass", "bandpass"];
            if !valid_filter_types.contains(&filter_type.as_str()) {
                return Err(format!(
                    "Invalid filter type: {}. Must be one of: {}",
                    filter_type,
                    valid_filter_types.join(", ")
                ));
            }
        }

        if let Some(cutoff) = self.synth_filter_cutoff {
            if !(20.0..=20000.0).contains(&cutoff) {
                return Err(format!(
                    "Filter cutoff {} is out of range (20-20000 Hz)",
                    cutoff
                ));
            }
        }

        if let Some(resonance) = self.synth_filter_resonance {
            if !(0.0..=1.0).contains(&resonance) {
                return Err(format!(
                    "Filter resonance {} is out of range (0.0-1.0)",
                    resonance
                ));
            }
        }

        // Validate effect intensities
        if let Some(reverb) = self.synth_reverb {
            if !(0.0..=1.0).contains(&reverb) {
                return Err(format!(
                    "Synthesis reverb {} is out of range (0.0-1.0)",
                    reverb
                ));
            }
        }

        if let Some(chorus) = self.synth_chorus {
            if !(0.0..=1.0).contains(&chorus) {
                return Err(format!(
                    "Synthesis chorus {} is out of range (0.0-1.0)",
                    chorus
                ));
            }
        }

        if let Some(delay) = self.synth_delay {
            if !(0.0..=1.0).contains(&delay) {
                return Err(format!(
                    "Synthesis delay {} is out of range (0.0-1.0)",
                    delay
                ));
            }
        }

        if let Some(delay_time) = self.synth_delay_time {
            if !(0.0..=2.0).contains(&delay_time) {
                return Err(format!(
                    "Synthesis delay time {} is out of range (0.0-2.0 seconds)",
                    delay_time
                ));
            }
        }

        // Validate synthesis-specific parameters
        if let Some(pulse_width) = self.synth_pulse_width {
            if !(0.1..=0.9).contains(&pulse_width) {
                return Err(format!(
                    "Pulse width {} is out of range (0.1-0.9)",
                    pulse_width
                ));
            }
        }

        if let Some(mod_freq) = self.synth_modulator_freq {
            if !(0.1..=1000.0).contains(&mod_freq) {
                return Err(format!(
                    "Modulator frequency {} is out of range (0.1-1000.0 Hz)",
                    mod_freq
                ));
            }
        }

        if let Some(mod_index) = self.synth_modulation_index {
            if !(0.0..=10.0).contains(&mod_index) {
                return Err(format!(
                    "Modulation index {} is out of range (0.0-10.0)",
                    mod_index
                ));
            }
        }

        if let Some(grain_size) = self.synth_grain_size {
            if !(0.01..=0.5).contains(&grain_size) {
                return Err(format!(
                    "Grain size {} is out of range (0.01-0.5 seconds)",
                    grain_size
                ));
            }
        }

        if let Some(roughness) = self.synth_texture_roughness {
            if !(0.0..=1.0).contains(&roughness) {
                return Err(format!(
                    "Texture roughness {} is out of range (0.0-1.0)",
                    roughness
                ));
            }
        }

        Ok(())
    }

    /// Validate preset parameters if this note uses presets
    pub fn validate_preset(&self) -> Result<(), String> {
        if !self.is_preset() {
            return Ok(());
        }

        // Validate that we have either a name or category (but not both conflicting modes)
        let has_name = self.preset_name.is_some();
        let has_category = self.preset_category.is_some();
        let has_random = self.preset_random.unwrap_or(false);

        if has_name && has_random {
            return Err(
                "Cannot use both 'preset_name' and 'preset_random' - choose one".to_string(),
            );
        }

        if has_category && has_name {
            // This is fine - category can be used with name for validation
        }

        if has_random && !has_category {
            return Err(
                "When using 'preset_random', you must specify 'preset_category'".to_string(),
            );
        }

        if !has_name && !has_category && !has_random {
            return Err("Preset note must specify either 'preset_name', 'preset_category', or 'preset_random'".to_string());
        }

        // Validate category if provided
        if let Some(category) = &self.preset_category {
            let valid_categories = [
                "bass", "pad", "lead", "keys", "organ", "arp", "drums", "effects",
            ];
            if !valid_categories.contains(&category.to_lowercase().as_str()) {
                return Err(format!(
                    "Invalid preset category '{}'. Valid categories: {:?}",
                    category, valid_categories
                ));
            }
        }

        Ok(())
    }

    /// Validate MIDI note parameters
    pub fn validate_midi(&self) -> Result<(), String> {
        if self.note_type != "midi" {
            return Err("Note type must be 'midi' for MIDI validation".to_string());
        }

        // MIDI note validation
        if let Some(note) = self.note {
            if note > 127 {
                return Err(format!("MIDI note must be 0-127, got {}", note));
            }
        }

        if let Some(velocity) = self.velocity {
            if velocity > 127 {
                return Err(format!("MIDI velocity must be 0-127, got {}", velocity));
            }
        }

        if let Some(channel) = self.channel {
            if channel > 15 {
                return Err(format!("MIDI channel must be 0-15, got {}", channel));
            }
        }

        if self.duration <= 0.0 {
            return Err(format!("Duration must be positive, got {}", self.duration));
        }

        if self.start_time < 0.0 {
            return Err(format!("Start time must be non-negative, got {}", self.start_time));
        }

        // Optional parameter validation
        if let Some(instrument) = self.instrument {
            if instrument > 127 {
                return Err(format!("MIDI instrument must be 0-127, got {}", instrument));
            }
        }

        if let Some(reverb) = self.reverb {
            if reverb > 127 {
                return Err(format!("MIDI reverb must be 0-127, got {}", reverb));
            }
        }

        if let Some(chorus) = self.chorus {
            if chorus > 127 {
                return Err(format!("MIDI chorus must be 0-127, got {}", chorus));
            }
        }

        if let Some(volume) = self.volume {
            if volume > 127 {
                return Err(format!("MIDI volume must be 0-127, got {}", volume));
            }
        }

        if let Some(pan) = self.pan {
            if pan > 127 {
                return Err(format!("MIDI pan must be 0-127, got {}", pan));
            }
        }

        if let Some(balance) = self.balance {
            if balance > 127 {
                return Err(format!("MIDI balance must be 0-127, got {}", balance));
            }
        }

        if let Some(expression) = self.expression {
            if expression > 127 {
                return Err(format!("MIDI expression must be 0-127, got {}", expression));
            }
        }

        if let Some(sustain) = self.sustain {
            if sustain > 127 {
                return Err(format!("MIDI sustain must be 0-127, got {}", sustain));
            }
        }

        Ok(())
    }
}
