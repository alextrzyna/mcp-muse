use crate::midi::{EffectConfig, EffectType};
use std::collections::HashMap;

/// Effects preset library for common audio scenarios
pub struct EffectsPresetLibrary {
    presets: HashMap<String, Vec<EffectConfig>>,
}

impl EffectsPresetLibrary {
    /// Create a new effects preset library with all standard presets
    pub fn new() -> Self {
        let mut library = EffectsPresetLibrary {
            presets: HashMap::new(),
        };

        // Load all standard effects presets
        library.load_studio_presets();
        library.load_ambient_presets();
        library.load_vintage_presets();
        library.load_creative_presets();

        library
    }

    /// Get effects preset by name
    pub fn get_preset(&self, name: &str) -> Option<&Vec<EffectConfig>> {
        // Try exact match first
        if let Some(preset) = self.presets.get(name) {
            return Some(preset);
        }

        // Try case-insensitive match
        let name_lower = name.to_lowercase();
        for (preset_name, preset) in &self.presets {
            if preset_name.to_lowercase() == name_lower {
                return Some(preset);
            }
        }

        None
    }

    /// Get all available preset names
    pub fn get_preset_names(&self) -> Vec<&String> {
        self.presets.keys().collect()
    }

    /// Load studio/professional effects presets
    fn load_studio_presets(&mut self) {
        // Studio - Clean professional sound
        self.presets.insert(
            "studio".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -18.0,
                        ratio: 3.0,
                        attack: 0.005,
                        release: 0.05,
                    },
                    intensity: 0.6,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.3,
                        dampening: 0.4,
                        wet_level: 0.15,
                        pre_delay: 0.02,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
            ],
        );

        // Concert Hall - Large, spacious venue
        self.presets.insert(
            "concert_hall".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.8,
                        dampening: 0.2,
                        wet_level: 0.4,
                        pre_delay: 0.05,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -24.0,
                        ratio: 2.0,
                        attack: 0.01,
                        release: 0.1,
                    },
                    intensity: 0.3,
                    enabled: true,
                },
            ],
        );

        // Live Stage - Medium room with some compression
        self.presets.insert(
            "live_stage".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -15.0,
                        ratio: 4.0,
                        attack: 0.003,
                        release: 0.03,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.5,
                        dampening: 0.3,
                        wet_level: 0.25,
                        pre_delay: 0.03,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
            ],
        );

        // Tight Mix - Punchy, controlled sound
        self.presets.insert(
            "tight_mix".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -12.0,
                        ratio: 6.0,
                        attack: 0.001,
                        release: 0.02,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.2,
                        dampening: 0.6,
                        wet_level: 0.1,
                        pre_delay: 0.01,
                    },
                    intensity: 0.3,
                    enabled: true,
                },
            ],
        );
    }

    /// Load ambient/atmospheric effects presets
    fn load_ambient_presets(&mut self) {
        // Ambient - Lush, atmospheric soundscape
        self.presets.insert(
            "ambient".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.9,
                        dampening: 0.1,
                        wet_level: 0.6,
                        pre_delay: 0.08,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 0.8,
                        depth: 0.6,
                        feedback: 0.4,
                        stereo_width: 1.0,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Delay {
                        delay_time: 0.5,
                        feedback: 0.3,
                        wet_level: 0.3,
                        sync_tempo: false,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
            ],
        );

        // Dreamy - Soft, ethereal atmosphere
        self.presets.insert(
            "dreamy".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.7,
                        dampening: 0.2,
                        wet_level: 0.5,
                        pre_delay: 0.06,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 0.5,
                        depth: 0.8,
                        feedback: 0.2,
                        stereo_width: 0.9,
                    },
                    intensity: 0.6,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::LowPass,
                        cutoff: 3000.0,
                        resonance: 0.5,
                        envelope_amount: 0.0,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
            ],
        );

        // Spacious - Large, open sound
        self.presets.insert(
            "spacious".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.85,
                        dampening: 0.25,
                        wet_level: 0.45,
                        pre_delay: 0.07,
                    },
                    intensity: 0.75,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Delay {
                        delay_time: 0.375,
                        feedback: 0.25,
                        wet_level: 0.2,
                        sync_tempo: false,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
            ],
        );
    }

    /// Load vintage/analog effects presets
    fn load_vintage_presets(&mut self) {
        // Vintage - Classic analog warmth
        self.presets.insert(
            "vintage".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Distortion {
                        drive: 1.0,
                        tone: 0.3,
                        output_level: 0.9,
                    },
                    intensity: 0.3,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 1.2,
                        depth: 0.4,
                        feedback: 0.3,
                        stereo_width: 0.8,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.5,
                        dampening: 0.6,
                        wet_level: 0.25,
                        pre_delay: 0.03,
                    },
                    intensity: 0.6,
                    enabled: true,
                },
            ],
        );

        // Analog Warmth - Warm, tube-like character
        self.presets.insert(
            "analog_warmth".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Distortion {
                        drive: 0.8,
                        tone: 0.2,
                        output_level: 1.0,
                    },
                    intensity: 0.25,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -20.0,
                        ratio: 2.5,
                        attack: 0.01,
                        release: 0.08,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::HighShelf,
                        cutoff: 8000.0,
                        resonance: 0.3,
                        envelope_amount: 0.0,
                    },
                    intensity: 0.3,
                    enabled: true,
                },
            ],
        );

        // Retro Echo - Classic tape delay sound
        self.presets.insert(
            "retro_echo".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Delay {
                        delay_time: 0.25,
                        feedback: 0.45,
                        wet_level: 0.35,
                        sync_tempo: false,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::HighPass,
                        cutoff: 200.0,
                        resonance: 0.2,
                        envelope_amount: 0.0,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Distortion {
                        drive: 0.5,
                        tone: 0.4,
                        output_level: 0.95,
                    },
                    intensity: 0.2,
                    enabled: true,
                },
            ],
        );
    }

    /// Load creative/experimental effects presets
    fn load_creative_presets(&mut self) {
        // Psychedelic - Wild, experimental sound
        self.presets.insert(
            "psychedelic".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 2.5,
                        depth: 0.8,
                        feedback: 0.6,
                        stereo_width: 1.0,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Delay {
                        delay_time: 0.33,
                        feedback: 0.6,
                        wet_level: 0.4,
                        sync_tempo: false,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::BandPass,
                        cutoff: 1500.0,
                        resonance: 3.0,
                        envelope_amount: 0.5,
                    },
                    intensity: 0.6,
                    enabled: true,
                },
            ],
        );

        // Distorted - Aggressive, edgy sound
        self.presets.insert(
            "distorted".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Distortion {
                        drive: 3.0,
                        tone: 0.7,
                        output_level: 0.8,
                    },
                    intensity: 0.7,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -10.0,
                        ratio: 8.0,
                        attack: 0.001,
                        release: 0.01,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::HighPass,
                        cutoff: 150.0,
                        resonance: 0.5,
                        envelope_amount: 0.0,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
            ],
        );

        // Filtered - Prominent filter effects
        self.presets.insert(
            "filtered".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Filter {
                        filter_type: crate::midi::FilterType::LowPass,
                        cutoff: 2000.0,
                        resonance: 2.0,
                        envelope_amount: 0.3,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 1.8,
                        depth: 0.3,
                        feedback: 0.2,
                        stereo_width: 0.6,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.4,
                        dampening: 0.4,
                        wet_level: 0.2,
                        pre_delay: 0.02,
                    },
                    intensity: 0.5,
                    enabled: true,
                },
            ],
        );

        // Lush Chorus - Rich, modulated sound
        self.presets.insert(
            "lush_chorus".to_string(),
            vec![
                EffectConfig {
                    effect: EffectType::Chorus {
                        rate: 1.0,
                        depth: 0.7,
                        feedback: 0.4,
                        stereo_width: 0.9,
                    },
                    intensity: 0.8,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Reverb {
                        room_size: 0.6,
                        dampening: 0.3,
                        wet_level: 0.3,
                        pre_delay: 0.04,
                    },
                    intensity: 0.6,
                    enabled: true,
                },
                EffectConfig {
                    effect: EffectType::Compressor {
                        threshold: -22.0,
                        ratio: 2.0,
                        attack: 0.008,
                        release: 0.06,
                    },
                    intensity: 0.4,
                    enabled: true,
                },
            ],
        );
    }
}

impl Default for EffectsPresetLibrary {
    fn default() -> Self {
        Self::new()
    }
}
