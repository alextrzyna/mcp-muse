use crate::expressive::presets::{ClassicSynthPreset, PresetCategory, PresetLibrary};
use crate::expressive::{SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load lead presets inspired by legendary synthesizers
    pub(crate) fn load_lead_presets(&mut self) {
        // Placeholder - will be implemented with full lead presets
        self.add_preset(ClassicSynthPreset {
            name: "Prophet Lead".to_string(),
            category: PresetCategory::Lead,
            subcategory: "Analog".to_string(),
            description: "Classic Sequential Prophet-5 lead sound".to_string(),
            inspiration: "Sequential Prophet-5".to_string(),
            tags: vec!["prophet".to_string(), "analog".to_string(), "classic".to_string()],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 440.0,
                amplitude: 0.8,
                duration: 2.0,
                envelope: PresetLibrary::create_envelope(0.05, 0.3, 0.6, 0.5),
                filter: Some(PresetLibrary::create_filter(1200.0, 0.5, crate::expressive::FilterType::LowPass)),
                effects: vec![PresetLibrary::create_reverb(0.25)],
            },
            variations: HashMap::new(),
        });
    }
} 