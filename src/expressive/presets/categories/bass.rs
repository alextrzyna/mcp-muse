use crate::expressive::presets::{
    ClassicSynthPreset, PresetCategory, PresetLibrary, PresetVariation,
};
use crate::expressive::{FilterType, SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load bass presets inspired by legendary synthesizers
    pub(crate) fn load_bass_presets(&mut self) {
        // ANALOG BASSES (8 presets)

        // 1. Minimoog Bass - The "fat" analog bass standard
        let mut variations = HashMap::new();
        variations.insert(
            "bright".to_string(),
            PresetVariation {
                name: "Bright".to_string(),
                description: "Brighter filter setting for cutting through mix".to_string(),
                parameter_overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert("filter_cutoff".to_string(), 1200.0);
                    overrides
                },
            },
        );
        variations.insert(
            "dark".to_string(),
            PresetVariation {
                name: "Dark".to_string(),
                description: "Darker, more muffled bass".to_string(),
                parameter_overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert("filter_cutoff".to_string(), 400.0);
                    overrides
                },
            },
        );

        self.add_preset(ClassicSynthPreset {
            name: "Minimoog Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Analog".to_string(),
            description:
                "Classic warm Moog bass with ladder filter resonance and characteristic punch"
                    .to_string(),
            inspiration: "Moog Minimoog Model D".to_string(),
            tags: vec![
                "vintage".to_string(),
                "warm".to_string(),
                "funk".to_string(),
                "rock".to_string(),
                "analog".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 110.0, // Will be overridden by note frequency
                amplitude: 0.8,
                duration: 1.0, // Will be overridden
                envelope: PresetLibrary::create_envelope(0.01, 0.3, 0.7, 0.5),
                filter: Some(PresetLibrary::create_filter(
                    800.0,
                    0.6,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.15)],
            },
            variations,
        });

        // 2. TB-303 Acid Bass - Acid house foundation
        let mut tb303_variations = HashMap::new();
        tb303_variations.insert(
            "squelchy".to_string(),
            PresetVariation {
                name: "Squelchy".to_string(),
                description: "Maximum resonance for acid house character".to_string(),
                parameter_overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert("filter_resonance".to_string(), 0.9);
                    overrides.insert("filter_cutoff".to_string(), 600.0);
                    overrides
                },
            },
        );

        self.add_preset(ClassicSynthPreset {
            name: "TB-303 Acid".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Analog".to_string(),
            description: "Squelchy, resonant acid bass with characteristic slides and accents"
                .to_string(),
            inspiration: "Roland TB-303 Bassline".to_string(),
            tags: vec![
                "acid".to_string(),
                "house".to_string(),
                "electronic".to_string(),
                "squelchy".to_string(),
                "resonant".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Square { pulse_width: 0.5 },
                frequency: 110.0,
                amplitude: 0.75,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.005, 0.2, 0.4, 0.3),
                filter: Some(PresetLibrary::create_filter(
                    700.0,
                    0.8,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.1)],
            },
            variations: tb303_variations,
        });

        // 3. ARP Odyssey Bass - Biting character
        self.add_preset(ClassicSynthPreset {
            name: "Odyssey Bite".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Analog".to_string(),
            description: "Sharp, aggressive bass with cutting edge character".to_string(),
            inspiration: "ARP Odyssey".to_string(),
            tags: vec![
                "sharp".to_string(),
                "aggressive".to_string(),
                "bite".to_string(),
                "vintage".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 110.0,
                amplitude: 0.85,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.005, 0.15, 0.6, 0.4),
                filter: Some(PresetLibrary::create_filter(
                    900.0,
                    0.7,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.08)],
            },
            variations: HashMap::new(),
        });

        // 4. Jupiter Bass - Rich Jupiter-8 bass
        self.add_preset(ClassicSynthPreset {
            name: "Jupiter Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Analog".to_string(),
            description: "Rich, warm Jupiter-8 style bass with lush analog character".to_string(),
            inspiration: "Roland Jupiter-8".to_string(),
            tags: vec![
                "rich".to_string(),
                "warm".to_string(),
                "lush".to_string(),
                "analog".to_string(),
                "vintage".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 110.0,
                amplitude: 0.8,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.02, 0.4, 0.8, 0.6),
                filter: Some(PresetLibrary::create_filter(
                    750.0,
                    0.5,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.3),
                    PresetLibrary::create_reverb(0.2),
                ],
            },
            variations: HashMap::new(),
        });

        // DIGITAL/FM BASSES (5 presets)

        // 5. TX81Z Lately Bass - Digital FM warmth
        self.add_preset(ClassicSynthPreset {
            name: "TX81Z Lately".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Digital".to_string(),
            description: "Clean digital FM bass with punchy attack, famous in hip-hop and house"
                .to_string(),
            inspiration: "Yamaha TX81Z 'Lately Bass'".to_string(),
            tags: vec![
                "digital".to_string(),
                "fm".to_string(),
                "clean".to_string(),
                "punchy".to_string(),
                "hip-hop".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::FM {
                    modulator_freq: 220.0,
                    modulation_index: 2.5,
                },
                frequency: 110.0,
                amplitude: 0.85,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.005, 0.2, 0.6, 0.4),
                filter: Some(PresetLibrary::create_filter(
                    1000.0,
                    0.3,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.12)],
            },
            variations: HashMap::new(),
        });

        // 6. DX7 Slap Bass - Percussive FM bass
        self.add_preset(ClassicSynthPreset {
            name: "DX7 Slap Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Digital".to_string(),
            description: "Percussive FM bass with sharp attack and metallic overtones".to_string(),
            inspiration: "Yamaha DX7".to_string(),
            tags: vec![
                "percussive".to_string(),
                "fm".to_string(),
                "sharp".to_string(),
                "metallic".to_string(),
                "80s".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::FM {
                    modulator_freq: 165.0,
                    modulation_index: 4.0,
                },
                frequency: 110.0,
                amplitude: 0.9,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.002, 0.1, 0.3, 0.2),
                filter: Some(PresetLibrary::create_filter(
                    1200.0,
                    0.4,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.08)],
            },
            variations: HashMap::new(),
        });

        // MODERN/HYBRID BASSES (7 presets)

        // 7. Saw Bass - Classic sawtooth bass
        self.add_preset(ClassicSynthPreset {
            name: "Saw Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Modern".to_string(),
            description: "Classic sawtooth bass with modern filter and envelope".to_string(),
            inspiration: "Modern Digital Synthesizers".to_string(),
            tags: vec![
                "sawtooth".to_string(),
                "classic".to_string(),
                "modern".to_string(),
                "clean".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 110.0,
                amplitude: 0.8,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.01, 0.25, 0.65, 0.45),
                filter: Some(PresetLibrary::create_filter(
                    850.0,
                    0.5,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.15)],
            },
            variations: HashMap::new(),
        });

        // 8. Square Bass - Pulse width bass
        let mut square_variations = HashMap::new();
        square_variations.insert(
            "narrow".to_string(),
            PresetVariation {
                name: "Narrow Pulse".to_string(),
                description: "Narrower pulse width for thinner character".to_string(),
                parameter_overrides: HashMap::new(), // Note: pulse_width would need special handling
            },
        );

        self.add_preset(ClassicSynthPreset {
            name: "Square Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Modern".to_string(),
            description: "Square wave bass with variable pulse width character".to_string(),
            inspiration: "Modern Digital Synthesizers".to_string(),
            tags: vec![
                "square".to_string(),
                "pulse".to_string(),
                "digital".to_string(),
                "clean".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Square { pulse_width: 0.6 },
                frequency: 110.0,
                amplitude: 0.75,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.01, 0.3, 0.7, 0.5),
                filter: Some(PresetLibrary::create_filter(
                    900.0,
                    0.4,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.12)],
            },
            variations: square_variations,
        });

        // SPECIALTY BASSES (5 presets)

        // 9. Sub Bass - Deep fundamental bass
        self.add_preset(ClassicSynthPreset {
            name: "Sub Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Specialty".to_string(),
            description: "Deep fundamental bass for electronic music foundations".to_string(),
            inspiration: "Modern Electronic Production".to_string(),
            tags: vec![
                "sub".to_string(),
                "deep".to_string(),
                "fundamental".to_string(),
                "electronic".to_string(),
                "foundation".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sine,
                frequency: 55.0, // Sub-bass frequency
                amplitude: 0.9,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.02, 0.1, 0.9, 0.8),
                filter: Some(PresetLibrary::create_filter(
                    120.0,
                    0.2,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.05)],
            },
            variations: HashMap::new(),
        });

        // 10. Rubber Bass - Elastic character with portamento effect
        self.add_preset(ClassicSynthPreset {
            name: "Rubber Bass".to_string(),
            category: PresetCategory::Bass,
            subcategory: "Specialty".to_string(),
            description: "Elastic bass character with bouncy, rubber-like feel".to_string(),
            inspiration: "Creative Sound Design".to_string(),
            tags: vec![
                "elastic".to_string(),
                "bouncy".to_string(),
                "creative".to_string(),
                "character".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 110.0,
                amplitude: 0.8,
                duration: 1.0,
                envelope: PresetLibrary::create_envelope(0.08, 0.4, 0.5, 0.6), // Slower attack for "bouncy" feel
                filter: Some(PresetLibrary::create_filter(
                    600.0,
                    0.7,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.4),
                    PresetLibrary::create_reverb(0.18),
                ],
            },
            variations: HashMap::new(),
        });
    }
}
