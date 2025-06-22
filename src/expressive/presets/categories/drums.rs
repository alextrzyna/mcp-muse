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
            signature_effects: PresetLibrary::create_empty_signature_effects(),
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
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });

        // TR-909 Hi-Hat
        self.add_preset(ClassicSynthPreset {
            name: "TR-909 Hi-Hat".to_string(),
            category: PresetCategory::Drums,
            subcategory: "Classic Drum Machines".to_string(),
            description: "Sharp, metallic TR-909 hi-hat with crisp attack".to_string(),
            inspiration: "Roland TR-909".to_string(),
            tags: vec![
                "909".to_string(),
                "hihat".to_string(),
                "metallic".to_string(),
                "sharp".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::HiHat {
                    metallic: 0.8,
                    decay: 0.1,      // Short, crisp decay
                    brightness: 2.5, // High frequency content
                },
                frequency: 8000.0,
                amplitude: 0.6,
                duration: 0.15, // Very short
                envelope: PresetLibrary::create_envelope(0.001, 0.01, 0.1, 0.1),
                filter: Some(PresetLibrary::create_filter(
                    12000.0, // High-pass for hi-hat character
                    0.2,
                    crate::expressive::FilterType::HighPass,
                )),
                effects: vec![],
            },
            variations: HashMap::new(),
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });

        // Crash Cymbal
        self.add_preset(ClassicSynthPreset {
            name: "Crash Cymbal".to_string(),
            category: PresetCategory::Drums,
            subcategory: "Cymbals".to_string(),
            description: "Bright crash cymbal with complex harmonic content".to_string(),
            inspiration: "Acoustic Drum Kits".to_string(),
            tags: vec![
                "crash".to_string(),
                "cymbal".to_string(),
                "bright".to_string(),
                "harmonic".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Cymbal {
                    size: 0.7,             // Medium-large cymbal
                    metallic: 0.9,         // Very metallic
                    strike_intensity: 0.8, // Hard strike
                },
                frequency: 3000.0,
                amplitude: 0.7,
                duration: 2.0, // Long decay like real cymbal
                envelope: PresetLibrary::create_envelope(0.001, 0.2, 0.4, 1.5),
                filter: Some(PresetLibrary::create_filter(
                    8000.0,
                    0.1,
                    crate::expressive::FilterType::HighPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.3)],
            },
            variations: HashMap::new(),
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });

        // 808 Hi-Hat (different style)
        self.add_preset(ClassicSynthPreset {
            name: "TR-808 Hi-Hat".to_string(),
            category: PresetCategory::Drums,
            subcategory: "Classic Drum Machines".to_string(),
            description: "Classic TR-808 hi-hat with distinctive tone".to_string(),
            inspiration: "Roland TR-808".to_string(),
            tags: vec![
                "808".to_string(),
                "hihat".to_string(),
                "classic".to_string(),
                "hip-hop".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::HiHat {
                    metallic: 0.6,
                    decay: 0.05,     // Very short 808 style
                    brightness: 3.0, // Bright but not as harsh as 909
                },
                frequency: 9000.0,
                amplitude: 0.5,
                duration: 0.08, // Extremely short
                envelope: PresetLibrary::create_envelope(0.001, 0.005, 0.05, 0.05),
                filter: Some(PresetLibrary::create_filter(
                    15000.0,
                    0.1,
                    crate::expressive::FilterType::HighPass,
                )),
                effects: vec![],
            },
            variations: HashMap::new(),
            signature_effects: PresetLibrary::create_empty_signature_effects(),
        });
    }
}
