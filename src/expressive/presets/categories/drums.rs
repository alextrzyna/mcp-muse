use crate::expressive::presets::{ClassicSynthPreset, PresetCategory, PresetLibrary};
use crate::expressive::{SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load drum presets inspired by classic drum machines
    pub(crate) fn load_drum_presets(&mut self) {
        // TR-808 Kick
        self.add_preset(ClassicSynthPreset {
            name: "TR-808 Kick".to_string(),
            category: PresetCategory::Drums,
            subcategory: "Classic Drum Machines".to_string(),
            description: "Deep, punchy TR-808 kick drum with characteristic punch".to_string(),
            inspiration: "Roland TR-808".to_string(),
            tags: vec![
                "808".to_string(),
                "kick".to_string(),
                "hip-hop".to_string(),
                "deep".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Kick {
                    punch: 0.8,
                    sustain: 0.6,
                    click_freq: 800.0,
                    body_freq: 60.0,
                },
                frequency: 60.0,
                amplitude: 0.9,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.005, 0.1, 0.3, 0.6),
                filter: Some(PresetLibrary::create_filter(
                    120.0,
                    0.2,
                    crate::expressive::FilterType::LowPass,
                )),
                effects: vec![],
            },
            variations: HashMap::new(),
        });

        // TR-909 Snare
        self.add_preset(ClassicSynthPreset {
            name: "TR-909 Snare".to_string(),
            category: PresetCategory::Drums,
            subcategory: "Classic Drum Machines".to_string(),
            description: "Punchy TR-909 snare with characteristic snap and buzz".to_string(),
            inspiration: "Roland TR-909".to_string(),
            tags: vec![
                "909".to_string(),
                "snare".to_string(),
                "techno".to_string(),
                "punchy".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Snare {
                    snap: 0.7,
                    buzz: 0.6,
                    tone_freq: 200.0,
                    noise_amount: 0.8,
                },
                frequency: 200.0,
                amplitude: 0.8,
                duration: 0.3,
                envelope: PresetLibrary::create_envelope(0.001, 0.05, 0.2, 0.15),
                filter: Some(PresetLibrary::create_filter(
                    2000.0,
                    0.3,
                    crate::expressive::FilterType::HighPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.1)],
            },
            variations: HashMap::new(),
        });
    }
}
