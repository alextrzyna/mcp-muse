use crate::expressive::presets::{ClassicSynthPreset, PresetCategory, PresetLibrary};
use crate::expressive::{SynthParams, SynthType, DX7Operator};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load keys presets inspired by legendary synthesizers
    pub(crate) fn load_keys_presets(&mut self) {
        // DX7 E.Piano 1 - Authentic ROM 1A 11 recreation
        self.add_preset(ClassicSynthPreset {
            name: "DX7 E.Piano".to_string(),
            category: PresetCategory::Keys,
            subcategory: "Electric Piano".to_string(),
            description: "Authentic DX7 E.Piano 1 - the most famous electric piano of the 80s".to_string(),
            inspiration: "Yamaha DX7 ROM 1A 11 E.Piano 1 (Algorithm 5)".to_string(),
            tags: vec![
                "authentic".to_string(),
                "electric".to_string(),
                "piano".to_string(),
                "fm".to_string(),
                "80s".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::DX7FM {
                    algorithm: 5,  // Algorithm 5: Three independent towers with two operator stacking
                    operators: [
                        // Operator 1 (Carrier)
                        DX7Operator {
                            frequency_ratio: 1.0,
                            output_level: 0.7,
                            detune: 0.0,
                            envelope: PresetLibrary::create_envelope(0.005, 0.3, 0.6, 3.0),
                        },
                        // Operator 2 (Modulator for piano character)
                        DX7Operator {
                            frequency_ratio: 14.0,  // High ratio for bell-like overtones
                            output_level: 0.3,      // Lower level for subtle modulation
                            detune: 0.0,
                            envelope: PresetLibrary::create_envelope(0.001, 0.1, 0.2, 1.0),
                        },
                        // Operator 3 (Additional harmonic)
                        DX7Operator {
                            frequency_ratio: 1.0,
                            output_level: 0.4,
                            detune: 7.0,  // Slight detune for richness
                            envelope: PresetLibrary::create_envelope(0.01, 0.4, 0.5, 2.5),
                        },
                        // Operators 4-6 (unused for simplified version)
                        DX7Operator::default(),
                        DX7Operator::default(),
                        DX7Operator::default(),
                    ],
                },
                frequency: 440.0,
                amplitude: 0.8,  // Standardized level for proper audibility
                duration: 3.0,
                envelope: PresetLibrary::create_envelope(0.005, 0.3, 0.6, 3.0), // Even longer release to prevent cut-off
                filter: Some(PresetLibrary::create_filter(
                    2800.0,  // Lower cutoff for warmer, less bell-like tone
                    0.02,    // Very minimal resonance for clean piano sound
                    crate::expressive::FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.15)],
            },
            variations: HashMap::new(),
        });
    }
}
