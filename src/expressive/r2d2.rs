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
    pub filter_resonance: f32,             // ARP 2600-style filter resonance
    pub attack_speed: f32,                 // Envelope attack characteristic
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
        
        // Happy - Cheerful warbling, musical frequencies with bouncy rhythm
        emotion_presets.insert("Happy".to_string(), EmotionParameters {
            carrier_freq_range: (294.0, 587.0), // D4 to D5 - musical and bright
            modulation_depth: 0.8,
            formant_shift: 1.3,
            pitch_contour: vec![0.4, 0.8, 0.3, 0.9, 0.2, 0.7, 0.4, 0.85], // Bouncy, playful pattern
            duration_multiplier: 1.0,
            harmonic_content: 0.9,
            filter_resonance: 0.5,
            attack_speed: 0.1,
        });
        
        // Sad - FORCED descending pattern to debug the issue
        emotion_presets.insert("Sad".to_string(), EmotionParameters {
            carrier_freq_range: (100.0, 500.0), // Very wide range for obvious test
            modulation_depth: 0.3,
            formant_shift: 0.6,
            pitch_contour: vec![1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.0], // GUARANTEED descent
            duration_multiplier: 2.5,
            harmonic_content: 0.2,
            filter_resonance: 0.3,
            attack_speed: 0.05,
        });
        
        // Excited - High energy rapid beeps with staccato rhythm
        emotion_presets.insert("Excited".to_string(), EmotionParameters {
            carrier_freq_range: (440.0, 880.0), // A4 to A5 - very high and energetic
            modulation_depth: 1.0,
            formant_shift: 1.6,
            pitch_contour: vec![0.6, 1.0, 0.2, 0.9, 0.1, 1.0, 0.3, 0.8, 0.1, 0.95], // Rapid bursts
            duration_multiplier: 0.6,
            harmonic_content: 1.2,
            filter_resonance: 0.7,
            attack_speed: 0.2,
        });
        
        // Worried - Nervous trembling, unstable pitch with hesitant rhythm
        emotion_presets.insert("Worried".to_string(), EmotionParameters {
            carrier_freq_range: (174.0, 349.0), // F3 to F4 - mid-range worry
            modulation_depth: 0.6,
            formant_shift: 0.85,
            pitch_contour: vec![0.5, 0.3, 0.7, 0.2, 0.6, 0.35, 0.55, 0.25], // Nervous trembling
            duration_multiplier: 1.4,
            harmonic_content: 0.4,
            filter_resonance: 0.4,
            attack_speed: 0.1,
        });
        
        // Curious - Rising inquisitive tones with questioning rhythm (VERY different from Happy)
        emotion_presets.insert("Curious".to_string(), EmotionParameters {
            carrier_freq_range: (130.0, 520.0), // C3 to C5 - wide range for dramatic rise
            modulation_depth: 0.7,
            formant_shift: 1.2,
            pitch_contour: vec![0.1, 0.15, 0.35, 0.65, 1.0], // Dramatic upward question sweep
            duration_multiplier: 1.2,
            harmonic_content: 0.8,
            filter_resonance: 0.6,
            attack_speed: 0.15,
        });
        
        // Affirmative - Confident, steady confirmation with assertive rhythm
        emotion_presets.insert("Affirmative".to_string(), EmotionParameters {
            carrier_freq_range: (146.0, 233.0), // D3 to Bb3 - lower, confident
            modulation_depth: 0.5,
            formant_shift: 1.1,
            pitch_contour: vec![0.8, 0.85, 0.9, 0.85], // Steady, confident assertion
            duration_multiplier: 0.8,
            harmonic_content: 0.7,
            filter_resonance: 0.5,
            attack_speed: 0.05,
        });
        
        // Negative - Sharp disapproval with abrupt cutoff rhythm
        emotion_presets.insert("Negative".to_string(), EmotionParameters {
            carrier_freq_range: (110.0, 175.0), // A2 to F3 - low disapproval
            modulation_depth: 0.4,
            formant_shift: 0.7,
            pitch_contour: vec![0.7, 0.3, 0.1], // Sharp, abrupt rejection
            duration_multiplier: 0.7,
            harmonic_content: 0.3,
            filter_resonance: 0.3,
            attack_speed: 0.05,
        });
        
        // Surprised - Dramatic upward sweeps with shock rhythm (different from Curious)
        emotion_presets.insert("Surprised".to_string(), EmotionParameters {
            carrier_freq_range: (220.0, 880.0), // A3 to A5 - very wide shock range
            modulation_depth: 1.1,
            formant_shift: 1.5,
            pitch_contour: vec![0.05, 0.95, 0.1, 0.8], // Quick shock then settle
            duration_multiplier: 0.5,
            harmonic_content: 1.0,
            filter_resonance: 0.8,
            attack_speed: 0.2,
        });
        
        // Thoughtful - Deep, contemplative pondering with slow deliberate rhythm
        emotion_presets.insert("Thoughtful".to_string(), EmotionParameters {
            carrier_freq_range: (82.0, 164.0), // E2 to E3 - very deep contemplation
            modulation_depth: 0.15,
            formant_shift: 0.7,
            pitch_contour: vec![0.4, 0.6, 0.45, 0.65, 0.5, 0.35], // Slow pondering waves
            duration_multiplier: 2.0,
            harmonic_content: 0.25,
            filter_resonance: 0.2,
            attack_speed: 0.05,
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
        
        // Pass pitch contour as-is - these are multipliers, not frequencies!
        let scaled_contour: Vec<f32> = emotion_params.pitch_contour.clone();
        
        Some(R2D2SynthParams {
            base_freq,
            modulation_depth: emotion_params.modulation_depth * expression.intensity,
            formant_shift: emotion_params.formant_shift,
            pitch_contour: scaled_contour,
            duration,
            harmonic_content: emotion_params.harmonic_content,
            filter_resonance: emotion_params.filter_resonance,
            attack_speed: emotion_params.attack_speed,
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
    pub filter_resonance: f32,
    pub attack_speed: f32,
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