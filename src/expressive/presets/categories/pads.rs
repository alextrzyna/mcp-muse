use crate::expressive::presets::{
    ClassicSynthPreset, PresetCategory, PresetLibrary, PresetVariation,
};
use crate::expressive::{FilterType, SynthParams, SynthType};
use std::collections::HashMap;

impl PresetLibrary {
    /// Load pad presets inspired by legendary synthesizers
    pub(crate) fn load_pad_presets(&mut self) {
        // WARM ANALOG PADS (10 presets)

        // 1. JP-8 Strings - Classic Jupiter-8 strings
        let mut jp8_variations = HashMap::new();
        jp8_variations.insert(
            "lush".to_string(),
            PresetVariation {
                name: "Lush".to_string(),
                description: "Maximum chorus and reverb for lush character".to_string(),
                parameter_overrides: HashMap::new(), // Effects would need special handling
            },
        );

        self.add_preset(ClassicSynthPreset {
            name: "JP-8 Strings".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Warm Analog".to_string(),
            description: "Classic Jupiter-8 string ensemble with rich, warm analog character"
                .to_string(),
            inspiration: "Roland Jupiter-8".to_string(),
            tags: vec![
                "strings".to_string(),
                "warm".to_string(),
                "analog".to_string(),
                "lush".to_string(),
                "vintage".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Pad {
                    warmth: 0.8,             // Authentic Jupiter-8 warmth
                    movement: 0.4,           // Subtle analog movement/detuning
                    space: 0.6,              // Spacious Jupiter character
                    harmonic_evolution: 0.3, // Gentle harmonic changes
                },
                frequency: 440.0,
                amplitude: 0.75, // Increased for better audibility
                duration: 4.0,
                envelope: PresetLibrary::create_envelope(0.8, 0.3, 0.8, 1.5), // Slow attack for pad character
                filter: Some(PresetLibrary::create_filter(
                    1200.0,
                    0.2, // Reduced resonance for warmer Jupiter character
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.5),
                    PresetLibrary::create_reverb(0.4),
                ],
            },
            variations: jp8_variations,
        });

        // 2. OB Brass - Oberheim brass section
        self.add_preset(ClassicSynthPreset {
            name: "OB Brass".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Warm Analog".to_string(),
            description: "Oberheim-style brass section with creamy analog textures".to_string(),
            inspiration: "Oberheim OB-8".to_string(),
            tags: vec![
                "brass".to_string(),
                "creamy".to_string(),
                "analog".to_string(),
                "oberheim".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Pad {
                    warmth: 0.9,             // Very warm Oberheim character
                    movement: 0.2,           // Gentle Oberheim movement
                    space: 0.5,              // Creamy spatial character
                    harmonic_evolution: 0.4, // Oberheim harmonic richness
                },
                frequency: 440.0,
                amplitude: 0.75, // Standardized pad level
                duration: 4.0,
                envelope: PresetLibrary::create_envelope(0.5, 0.4, 0.7, 1.2),
                filter: Some(PresetLibrary::create_filter(
                    900.0, // Slightly warmer filter for Oberheim character
                    0.3,   // Moderate resonance for Oberheim "creamy" character
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.3),
                    PresetLibrary::create_reverb(0.3),
                ],
            },
            variations: HashMap::new(),
        });

        // 3. Analog Wash - Atmospheric texture
        self.add_preset(ClassicSynthPreset {
            name: "Analog Wash".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Warm Analog".to_string(),
            description: "Atmospheric analog pad with flowing texture and movement".to_string(),
            inspiration: "Analog Synthesizer Traditions".to_string(),
            tags: vec![
                "atmospheric".to_string(),
                "flowing".to_string(),
                "texture".to_string(),
                "analog".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Pad {
                    warmth: 0.8,
                    movement: 0.6,
                    space: 0.7,
                    harmonic_evolution: 0.5,
                },
                frequency: 440.0,
                amplitude: 0.75, // Increased for better presence
                duration: 8.0,   // Long pad duration
                envelope: PresetLibrary::create_envelope(1.2, 0.8, 0.85, 2.0),
                filter: Some(PresetLibrary::create_filter(
                    900.0,
                    0.3,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.4),
                    PresetLibrary::create_reverb(0.6),
                ],
            },
            variations: HashMap::new(),
        });

        // DIGITAL/HYBRID PADS (10 presets)

        // 4. D-50 Fantasia - Famous D-50 preset
        self.add_preset(ClassicSynthPreset {
            name: "D-50 Fantasia".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Digital".to_string(),
            description:
                "Famous Roland D-50 preset with complex attack samples and analog synthesis"
                    .to_string(),
            inspiration: "Roland D-50 'Fantasia'".to_string(),
            tags: vec![
                "d-50".to_string(),
                "famous".to_string(),
                "complex".to_string(),
                "digital".to_string(),
                "cinematic".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Pad {
                    warmth: 0.6,
                    movement: 0.8,
                    space: 0.9,
                    harmonic_evolution: 0.7,
                },
                frequency: 440.0,
                amplitude: 0.75, // Standardized level
                duration: 6.0,
                envelope: PresetLibrary::create_envelope(0.3, 0.8, 0.8, 1.8), // Quick attack then evolving
                filter: Some(PresetLibrary::create_filter(
                    1500.0,
                    0.2,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.6),
                    PresetLibrary::create_reverb(0.7),
                ],
            },
            variations: HashMap::new(),
        });

        // 5. Crystal Pad - Bright, clean texture
        self.add_preset(ClassicSynthPreset {
            name: "Crystal Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Digital".to_string(),
            description: "Bright, crystalline pad with clean digital character".to_string(),
            inspiration: "Modern Digital Synthesizers".to_string(),
            tags: vec![
                "crystal".to_string(),
                "bright".to_string(),
                "clean".to_string(),
                "digital".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Triangle, // Clean, pure tone
                frequency: 440.0,
                amplitude: 0.75, // Standardized level
                duration: 5.0,
                envelope: PresetLibrary::create_envelope(0.6, 0.2, 0.9, 1.5),
                filter: Some(PresetLibrary::create_filter(
                    2000.0,
                    0.1,
                    FilterType::LowPass,
                )), // Bright filter
                effects: vec![
                    PresetLibrary::create_chorus(0.3),
                    PresetLibrary::create_reverb(0.5),
                ],
            },
            variations: HashMap::new(),
        });

        // ATMOSPHERIC PADS (10 presets)

        // 6. Space Pad - Cosmic atmosphere
        self.add_preset(ClassicSynthPreset {
            name: "Space Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Atmospheric".to_string(),
            description: "Cosmic atmospheric pad with ethereal, spacious character".to_string(),
            inspiration: "Ambient Electronic Music".to_string(),
            tags: vec![
                "space".to_string(),
                "cosmic".to_string(),
                "atmospheric".to_string(),
                "ethereal".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Pad {
                    warmth: 0.4,
                    movement: 0.9,
                    space: 1.0,
                    harmonic_evolution: 0.8,
                },
                frequency: 440.0,
                amplitude: 0.7, // Atmospheric but still present
                duration: 10.0, // Very long atmospheric pad
                envelope: PresetLibrary::create_envelope(2.0, 1.0, 0.9, 3.0), // Very slow attack
                filter: Some(PresetLibrary::create_filter(
                    800.0,
                    0.2,
                    FilterType::LowPass,
                )),
                effects: vec![PresetLibrary::create_reverb(0.8)], // Heavy reverb for space
            },
            variations: HashMap::new(),
        });

        // 7. Dark Pad - Mysterious character
        self.add_preset(ClassicSynthPreset {
            name: "Dark Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Atmospheric".to_string(),
            description: "Dark, mysterious pad for suspenseful and moody atmospheres".to_string(),
            inspiration: "Film Scoring Techniques".to_string(),
            tags: vec![
                "dark".to_string(),
                "mysterious".to_string(),
                "suspense".to_string(),
                "moody".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Sawtooth,
                frequency: 220.0, // Lower frequency for darkness
                amplitude: 0.75,  // Standardized level
                duration: 6.0,
                envelope: PresetLibrary::create_envelope(1.5, 0.8, 0.7, 2.5),
                filter: Some(PresetLibrary::create_filter(
                    400.0,
                    0.5,
                    FilterType::LowPass,
                )), // Dark filter
                effects: vec![PresetLibrary::create_reverb(0.6)],
            },
            variations: HashMap::new(),
        });

        // 8. Choir Pad - Vocal atmosphere
        self.add_preset(ClassicSynthPreset {
            name: "Choir Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Atmospheric".to_string(),
            description: "Vocal-like pad simulating choir atmosphere with warm harmonics"
                .to_string(),
            inspiration: "Orchestral Synthesizer Traditions".to_string(),
            tags: vec![
                "choir".to_string(),
                "vocal".to_string(),
                "warm".to_string(),
                "harmonics".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Triangle, // Softer than sawtooth
                frequency: 440.0,
                amplitude: 0.75, // Standardized level
                duration: 8.0,
                envelope: PresetLibrary::create_envelope(1.0, 0.6, 0.8, 2.0),
                filter: Some(PresetLibrary::create_filter(
                    1100.0,
                    0.3,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.5),
                    PresetLibrary::create_reverb(0.7),
                ], // Choir-like effects
            },
            variations: HashMap::new(),
        });

        // 9. Wind Pad - Breathy texture
        self.add_preset(ClassicSynthPreset {
            name: "Wind Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Atmospheric".to_string(),
            description: "Breathy, wind-like pad texture with organic movement".to_string(),
            inspiration: "Natural Sound Design".to_string(),
            tags: vec![
                "wind".to_string(),
                "breathy".to_string(),
                "organic".to_string(),
                "movement".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Noise {
                    color: crate::expressive::NoiseColor::White,
                },
                frequency: 440.0,
                amplitude: 0.65, // Softer for wind character but still audible
                duration: 12.0,  // Very long for ambience
                envelope: PresetLibrary::create_envelope(3.0, 2.0, 0.6, 4.0), // Very slow development
                filter: Some(PresetLibrary::create_filter(
                    600.0,
                    0.1,
                    FilterType::BandPass,
                )), // Shaped noise
                effects: vec![PresetLibrary::create_reverb(0.8)],
            },
            variations: HashMap::new(),
        });

        // 10. Dream Pad - Surreal character
        self.add_preset(ClassicSynthPreset {
            name: "Dream Pad".to_string(),
            category: PresetCategory::Pad,
            subcategory: "Atmospheric".to_string(),
            description: "Surreal, dream-like pad with floating, otherworldly character"
                .to_string(),
            inspiration: "Ambient and New Age Music".to_string(),
            tags: vec![
                "dream".to_string(),
                "surreal".to_string(),
                "floating".to_string(),
                "otherworldly".to_string(),
            ],
            synth_params: SynthParams {
                synth_type: SynthType::Texture {
                    roughness: 0.2,
                    evolution: 0.8,
                    spectral_tilt: 0.3,
                    modulation_depth: 0.6,
                },
                frequency: 440.0,
                amplitude: 0.7, // Dreamy but audible
                duration: 15.0, // Very long dreamy pad
                envelope: PresetLibrary::create_envelope(4.0, 3.0, 0.8, 5.0), // Extremely slow
                filter: Some(PresetLibrary::create_filter(
                    1000.0,
                    0.2,
                    FilterType::LowPass,
                )),
                effects: vec![
                    PresetLibrary::create_chorus(0.7),
                    PresetLibrary::create_reverb(0.9),
                ], // Maximum dreaminess
            },
            variations: HashMap::new(),
        });
    }
}
