use crate::expressive::presets::{ClassicSynthPreset, PresetCategory, PresetLibrary};
use crate::expressive::{SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load keys presets inspired by legendary synthesizers
    pub(crate) fn load_keys_presets(&mut self) {
        // Placeholder - DX7 Electric Piano
        self.add_preset(ClassicSynthPreset {
            name: "DX7 E.Piano".to_string(),
            category: PresetCategory::Keys,
            subcategory: "Electric Piano".to_string(),
            description: "Classic DX7 electric piano with FM synthesis".to_string(),
            inspiration: "Yamaha DX7".to_string(),
            tags: vec![
                "dx7".to_string(),
                "electric".to_string(),
                "piano".to_string(),
                "fm".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::FM {
                    modulator_freq: 880.0,
                    modulation_index: 3.0,
                },
                frequency: 440.0,
                amplitude: 0.7,
                duration: 3.0,
                envelope: PresetLibrary::create_envelope(0.01, 0.8, 0.3, 1.2),
                filter: Some(PresetLibrary::create_filter(
                    2000.0,
                    0.2,
                    crate::expressive::FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.3)],
            },
            variations: HashMap::new(),
        });
    }
}
