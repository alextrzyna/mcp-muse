use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// R2D2 emotional states for expressive vocalizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum R2D2Emotion {
    Happy,
    Sad,
    Excited,
    Worried,
    Curious,
    Affirmative,
    Negative,
    Surprised,
    Thoughtful,
}

/// Parameters that define an R2D2 emotional expression
#[derive(Debug, Clone)]
pub struct EmotionParameters {
    pub carrier_freq_range: (f32, f32),    // Base frequency range
    pub modulation_depth: f32,             // Ring mod intensity
    pub formant_shift: f32,                // Vocal tract simulation
    pub pitch_contour: Vec<f32>,           // Melodic shape
    pub duration_multiplier: f32,          // Timing characteristics
    pub harmonic_content: f32,             // Spectral richness
}

/// Complete R2D2 expression with all parameters
#[derive(Debug, Clone)]
pub struct R2D2Expression {
    pub emotion: R2D2Emotion,
    pub intensity: f32,           // 0.0-1.0
    pub duration: f32,            // seconds
    pub phrase_complexity: u8,    // 1-5 syllables
    pub pitch_range: (f32, f32),  // Hz range
    pub context: Option<String>,  // conversation context
}

/// R2D2 voice generator with emotion-based synthesis
pub struct R2D2Voice {
    emotion_presets: HashMap<String, EmotionParameters>,
}

impl R2D2Voice {
    /// Create a new R2D2 voice with predefined emotion presets
    pub fn new() -> Self {
        let mut emotion_presets = HashMap::new();
        
        // Happy - Cheerful warbling, musical frequencies
        emotion_presets.insert("Happy".to_string(), EmotionParameters {
            carrier_freq_range: (220.0, 440.0), // Musical A notes for pleasant sound
            modulation_depth: 0.6,
            formant_shift: 1.2,
            pitch_contour: vec![0.3, 0.7, 0.9, 1.0, 0.8, 0.9], // Cheerful bounce
            duration_multiplier: 1.0,
            harmonic_content: 0.8,
        });
        
        // Sad - Low, mournful descending tones
        emotion_presets.insert("Sad".to_string(), EmotionParameters {
            carrier_freq_range: (110.0, 220.0), // Lower register for sadness
            modulation_depth: 0.3,
            formant_shift: 0.7,
            pitch_contour: vec![1.0, 0.8, 0.6, 0.4, 0.2], // Descending whimper
            duration_multiplier: 1.4,
            harmonic_content: 0.3,
        });
        
        // Excited - High energy rapid beeps
        emotion_presets.insert("Excited".to_string(), EmotionParameters {
            carrier_freq_range: (330.0, 660.0), // Higher frequencies for excitement
            modulation_depth: 0.9,
            formant_shift: 1.4,
            pitch_contour: vec![0.4, 1.0, 0.6, 0.9, 0.7, 1.0, 0.5, 0.8], // Rapid excited pattern
            duration_multiplier: 0.7,
            harmonic_content: 1.0,
        });
        
        // Worried - Nervous trembling, unstable pitch
        emotion_presets.insert("Worried".to_string(), EmotionParameters {
            carrier_freq_range: (165.0, 330.0), // Mid-range anxiety
            modulation_depth: 0.7,
            formant_shift: 0.9,
            pitch_contour: vec![0.5, 0.7, 0.4, 0.8, 0.3, 0.6, 0.4], // Nervous wavering
            duration_multiplier: 1.3,
            harmonic_content: 0.5,
        });
        
        // Curious - Rising inquisitive tones
        emotion_presets.insert("Curious".to_string(), EmotionParameters {
            carrier_freq_range: (196.0, 392.0), // G to G octave for questions
            modulation_depth: 0.7,
            formant_shift: 1.1,
            pitch_contour: vec![0.2, 0.4, 0.6, 0.8, 1.0], // Rising question
            duration_multiplier: 1.1,
            harmonic_content: 0.7,
        });
        
        // Affirmative - Confident, steady confirmation
        emotion_presets.insert("Affirmative".to_string(), EmotionParameters {
            carrier_freq_range: (147.0, 294.0), // Lower, confident register
            modulation_depth: 0.4,
            formant_shift: 1.0,
            pitch_contour: vec![0.8, 0.9, 1.0, 0.95, 0.9], // Steady confirmation
            duration_multiplier: 0.9,
            harmonic_content: 0.8,
        });
        
        // Negative - Sharp disapproval, declining tones
        emotion_presets.insert("Negative".to_string(), EmotionParameters {
            carrier_freq_range: (123.0, 246.0), // Lower disapproving tones
            modulation_depth: 0.5,
            formant_shift: 0.7,
            pitch_contour: vec![0.9, 0.6, 0.4, 0.2, 0.1], // Sharp decline
            duration_multiplier: 0.8,
            harmonic_content: 0.6,
        });
        
        // Surprised - Dramatic upward sweeps
        emotion_presets.insert("Surprised".to_string(), EmotionParameters {
            carrier_freq_range: (277.0, 554.0), // Wide range for surprise
            modulation_depth: 0.9,
            formant_shift: 1.3,
            pitch_contour: vec![0.1, 0.9, 1.0, 0.7, 0.8], // Dramatic surprise sweep
            duration_multiplier: 0.6,
            harmonic_content: 0.9,
        });
        
        // Thoughtful - Deep, contemplative pondering
        emotion_presets.insert("Thoughtful".to_string(), EmotionParameters {
            carrier_freq_range: (98.0, 196.0), // Deep thoughtful tones
            modulation_depth: 0.25,
            formant_shift: 0.8,
            pitch_contour: vec![0.6, 0.7, 0.5, 0.8, 0.6, 0.4], // Gentle contemplation
            duration_multiplier: 1.6,
            harmonic_content: 0.4,
        });
        
        R2D2Voice {
            emotion_presets,
        }
    }
    
    /// Get emotion parameters for a specific emotion
    pub fn get_emotion_params(&self, emotion: &R2D2Emotion) -> Option<&EmotionParameters> {
        let emotion_key = format!("{:?}", emotion);
        self.emotion_presets.get(&emotion_key)
    }
    
    /// Generate synthesis parameters for an R2D2 expression
    pub fn generate_expression_params(&self, expression: &R2D2Expression) -> Option<R2D2SynthParams> {
        let emotion_params = self.get_emotion_params(&expression.emotion)?;
        
        // Calculate base frequency from range and intensity
        let freq_range = emotion_params.carrier_freq_range.1 - emotion_params.carrier_freq_range.0;
        let base_freq = emotion_params.carrier_freq_range.0 + freq_range * expression.intensity;
        
        // Adjust duration based on emotion and complexity
        let duration = expression.duration * emotion_params.duration_multiplier * 
                      (1.0 + expression.phrase_complexity as f32 * 0.2);
        
        // Scale pitch contour by intensity and pitch range
        let pitch_range_size = expression.pitch_range.1 - expression.pitch_range.0;
        let scaled_contour: Vec<f32> = emotion_params.pitch_contour
            .iter()
            .map(|&p| p * expression.intensity * pitch_range_size)
            .collect();
        
        Some(R2D2SynthParams {
            base_freq,
            modulation_depth: emotion_params.modulation_depth * expression.intensity,
            formant_shift: emotion_params.formant_shift,
            pitch_contour: scaled_contour,
            duration,
            harmonic_content: emotion_params.harmonic_content,
        })
    }
    
    /// Create a multi-syllable R2D2 phrase
    pub fn create_phrase(&self, expression: &R2D2Expression) -> Vec<R2D2SynthParams> {
        let mut phrase = Vec::new();
        let syllable_count = expression.phrase_complexity.max(1) as usize;
        
        for i in 0..syllable_count {
            // Create slight variations for each syllable
            let mut syllable_expression = expression.clone();
            
            // Vary intensity slightly for each syllable
            let intensity_variation = 0.1 * (i as f32 / syllable_count as f32 - 0.5);
            syllable_expression.intensity = (expression.intensity + intensity_variation).clamp(0.0, 1.0);
            
            // Adjust duration for syllables
            syllable_expression.duration = expression.duration / syllable_count as f32;
            
            if let Some(params) = self.generate_expression_params(&syllable_expression) {
                phrase.push(params);
            }
        }
        
        phrase
    }
}

/// Synthesizer parameters for R2D2 voice generation
#[derive(Debug, Clone)]
pub struct R2D2SynthParams {
    pub base_freq: f32,
    pub modulation_depth: f32,
    pub formant_shift: f32,
    pub pitch_contour: Vec<f32>,
    pub duration: f32,
    pub harmonic_content: f32,
}

impl Default for R2D2Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for R2D2Emotion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            R2D2Emotion::Happy => write!(f, "Happy"),
            R2D2Emotion::Sad => write!(f, "Sad"),
            R2D2Emotion::Excited => write!(f, "Excited"),
            R2D2Emotion::Worried => write!(f, "Worried"),
            R2D2Emotion::Curious => write!(f, "Curious"),
            R2D2Emotion::Affirmative => write!(f, "Affirmative"),
            R2D2Emotion::Negative => write!(f, "Negative"),
            R2D2Emotion::Surprised => write!(f, "Surprised"),
            R2D2Emotion::Thoughtful => write!(f, "Thoughtful"),
        }
    }
} 