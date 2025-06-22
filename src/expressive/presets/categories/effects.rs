use crate::expressive::presets::{ClassicSynthPreset, PresetCategory, PresetLibrary};
use crate::expressive::{SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load effects presets for sound design
    pub(crate) fn load_effects_presets(&mut self) {
        // Sci-Fi Zap
        self.add_preset(ClassicSynthPreset {
            name: "Sci-Fi Zap".to_string(),
            category: PresetCategory::Effects,
            subcategory: "Sound Effects".to_string(),
            description: "Classic science fiction energy zap sound".to_string(),
            inspiration: "Classic Sci-Fi Films".to_string(),
            tags: vec![
                "sci-fi".to_string(),
                "zap".to_string(),
                "energy".to_string(),
                "laser".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Zap {
                    energy: 0.8,
                    decay: 0.6,
                    harmonic_content: 0.7,
                },
                frequency: 800.0,
                amplitude: 0.8,
                duration: 0.4,
                envelope: PresetLibrary::create_envelope(0.001, 0.05, 0.3, 0.2),
                filter: None,
                effects: vec![PresetLibrary::create_reverb(0.2)],
            },
            variations: HashMap::new(),
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });

        // Sweep Up
        self.add_preset(ClassicSynthPreset {
            name: "Sweep Up".to_string(),
            category: PresetCategory::Effects,
            subcategory: "Sound Effects".to_string(),
            description: "Rising sweep effect for transitions and builds".to_string(),
            inspiration: "Electronic Music Production".to_string(),
            tags: vec![
                "sweep".to_string(),
                "rising".to_string(),
                "transition".to_string(),
                "build".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Swoosh {
                    direction: 1.0, // Upward sweep
                    intensity: 0.7,
                    frequency_sweep: (200.0, 2000.0), // Low to high
                },
                frequency: 200.0,
                amplitude: 0.8, // Standardized effect level
                duration: 2.0,
                envelope: PresetLibrary::create_envelope(0.1, 0.5, 0.8, 1.0),
                filter: Some(PresetLibrary::create_filter(
                    1500.0,
                    0.2,
                    crate::expressive::FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.4)],
            },
            variations: HashMap::new(),
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });
    }
}
