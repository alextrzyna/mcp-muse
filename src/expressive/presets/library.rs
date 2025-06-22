use crate::expressive::{
    EffectParams, EffectType, EnvelopeParams, FilterParams, FilterType, SynthParams,
};
use crate::midi::EffectConfig;
use rand::prelude::IndexedRandom;
use rand::rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classic synthesizer preset inspired by vintage hardware
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassicSynthPreset {
    pub name: String,
    pub category: PresetCategory,
    pub subcategory: String,
    pub description: String,
    pub inspiration: String, // Original synthesizer that inspired this preset
    pub tags: Vec<String>,
    pub synth_params: SynthParams,
    pub variations: HashMap<String, PresetVariation>,
    /// Signature effects for this preset (subtle, appropriate)
    pub signature_effects: Vec<EffectConfig>,
}

/// Preset categories organized by musical context
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresetCategory {
    Bass,
    Pad,
    Lead,
    Keys,
    Organ,
    Arp,
    Drums,
    Effects,
}

/// Preset variations allow slight modifications to base presets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetVariation {
    pub name: String,
    pub description: String,
    pub parameter_overrides: HashMap<String, f32>,
}

/// Central preset library with efficient lookup and categorization
pub struct PresetLibrary {
    presets: HashMap<String, ClassicSynthPreset>,
    categories: HashMap<PresetCategory, Vec<String>>,
    tags: HashMap<String, Vec<String>>,
}

impl PresetLibrary {
    /// Create a new preset library with all classic synthesizer presets loaded
    pub fn new() -> Self {
        let mut library = PresetLibrary {
            presets: HashMap::new(),
            categories: HashMap::new(),
            tags: HashMap::new(),
        };

        // Load all preset categories
        library.load_bass_presets();
        library.load_pad_presets();
        library.load_lead_presets();
        library.load_keys_presets();
        library.load_organ_presets();
        library.load_arp_presets();
        library.load_drum_presets();
        library.load_effects_presets();

        library
    }

    /// Load a preset by name with fuzzy matching
    pub fn load_preset(&self, name: &str) -> Option<&ClassicSynthPreset> {
        // First try exact match
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

        // Try fuzzy matching - check if the input contains key words from preset names
        let name_words: Vec<&str> = name_lower.split_whitespace().collect();
        for (preset_name, preset) in &self.presets {
            let preset_lower = preset_name.to_lowercase();
            let preset_words: Vec<&str> = preset_lower.split_whitespace().collect();

            // Check if most key words match
            let mut matches = 0;
            for word in &name_words {
                if preset_words
                    .iter()
                    .any(|pw| pw.contains(word) || word.contains(pw))
                {
                    matches += 1;
                }
            }

            // If most words match, consider it a match
            if matches >= name_words.len().min(preset_words.len()) {
                return Some(preset);
            }
        }

        None
    }

    /// Get all presets in a category
    pub fn get_by_category(&self, category: PresetCategory) -> Vec<&ClassicSynthPreset> {
        if let Some(preset_names) = self.categories.get(&category) {
            preset_names
                .iter()
                .filter_map(|name| self.presets.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Search presets by tags
    #[allow(dead_code)]
    pub fn search_by_tags(&self, tags: &[String]) -> Vec<&ClassicSynthPreset> {
        let mut results = Vec::new();

        for tag in tags {
            if let Some(preset_names) = self.tags.get(tag) {
                for name in preset_names {
                    if let Some(preset) = self.presets.get(name) {
                        if !results
                            .iter()
                            .any(|p: &&ClassicSynthPreset| p.name == preset.name)
                        {
                            results.push(preset);
                        }
                    }
                }
            }
        }

        results
    }

    /// Get a random preset, optionally filtered by category
    pub fn get_random_preset(
        &self,
        category: Option<PresetCategory>,
    ) -> Option<&ClassicSynthPreset> {
        let mut rng = rng();

        if let Some(cat) = category {
            let presets = self.get_by_category(cat);
            presets.choose(&mut rng).copied()
        } else {
            let all_presets: Vec<&ClassicSynthPreset> = self.presets.values().collect();
            all_presets.choose(&mut rng).copied()
        }
    }

    /// List all available preset names
    #[allow(dead_code)]
    pub fn list_preset_names(&self) -> Vec<String> {
        self.presets.keys().cloned().collect()
    }

    /// List all preset names in a category
    #[allow(dead_code)]
    pub fn list_category_presets(&self, category: PresetCategory) -> Vec<String> {
        self.categories.get(&category).cloned().unwrap_or_default()
    }

    /// Apply a preset variation to get modified parameters
    pub fn apply_variation(&self, preset_name: &str, variation_name: &str) -> Option<SynthParams> {
        if let Some(preset) = self.presets.get(preset_name) {
            if let Some(variation) = preset.variations.get(variation_name) {
                let mut params = preset.synth_params.clone();

                // Apply parameter overrides
                for (param_name, value) in &variation.parameter_overrides {
                    match param_name.as_str() {
                        "filter_cutoff" => {
                            if let Some(ref mut filter) = params.filter {
                                filter.cutoff = *value;
                            }
                        }
                        "filter_resonance" => {
                            if let Some(ref mut filter) = params.filter {
                                filter.resonance = *value;
                            }
                        }
                        "attack" => params.envelope.attack = *value,
                        "decay" => params.envelope.decay = *value,
                        "sustain" => params.envelope.sustain = *value,
                        "release" => params.envelope.release = *value,
                        "amplitude" => params.amplitude = *value,
                        _ => {} // Ignore unknown parameters
                    }
                }

                return Some(params);
            }
        }
        None
    }

    /// Add a preset to the library
    pub(crate) fn add_preset(&mut self, preset: ClassicSynthPreset) {
        let name = preset.name.clone();
        let category = preset.category.clone();
        let tags = preset.tags.clone();

        // Add to main preset map
        self.presets.insert(name.clone(), preset);

        // Add to category index
        self.categories
            .entry(category)
            .or_default()
            .push(name.clone());

        // Add to tag index
        for tag in tags {
            self.tags.entry(tag).or_default().push(name.clone());
        }
    }

    /// Helper to create common envelope parameters
    pub fn create_envelope(attack: f32, decay: f32, sustain: f32, release: f32) -> EnvelopeParams {
        EnvelopeParams {
            attack,
            decay,
            sustain,
            release,
        }
    }

    /// Helper to create common filter parameters
    pub fn create_filter(cutoff: f32, resonance: f32, filter_type: FilterType) -> FilterParams {
        FilterParams {
            cutoff,
            resonance,
            filter_type,
        }
    }

    /// Helper to create common effect parameters
    pub fn create_reverb(intensity: f32) -> EffectParams {
        EffectParams {
            effect_type: EffectType::Reverb,
            intensity,
        }
    }

    pub fn create_chorus(intensity: f32) -> EffectParams {
        EffectParams {
            effect_type: EffectType::Chorus,
            intensity,
        }
    }

    /// Helper to create signature effects for presets
    pub fn create_signature_effects_for_bass() -> Vec<EffectConfig> {
        vec![EffectConfig {
            effect: crate::midi::EffectType::Compressor {
                threshold: -16.0,
                ratio: 3.0,
                attack: 0.003,
                release: 0.05,
            },
            intensity: 0.4,
            enabled: true,
        }]
    }

    pub fn create_signature_effects_for_acid_bass() -> Vec<EffectConfig> {
        vec![
            EffectConfig {
                effect: crate::midi::EffectType::Filter {
                    filter_type: crate::midi::FilterType::LowPass,
                    cutoff: 800.0,
                    resonance: 2.0,
                    envelope_amount: 0.5,
                },
                intensity: 0.6,
                enabled: true,
            },
            EffectConfig {
                effect: crate::midi::EffectType::Delay {
                    delay_time: 0.125,
                    feedback: 0.2,
                    wet_level: 0.15,
                    sync_tempo: true,
                },
                intensity: 0.3,
                enabled: true,
            },
        ]
    }

    pub fn create_signature_effects_for_vintage_warmth() -> Vec<EffectConfig> {
        vec![
            EffectConfig {
                effect: crate::midi::EffectType::Distortion {
                    drive: 0.8,
                    tone: 0.3,
                    output_level: 0.95,
                },
                intensity: 0.2,
                enabled: true,
            },
            EffectConfig {
                effect: crate::midi::EffectType::Compressor {
                    threshold: -20.0,
                    ratio: 2.5,
                    attack: 0.01,
                    release: 0.08,
                },
                intensity: 0.5,
                enabled: true,
            },
        ]
    }

    pub fn create_signature_effects_for_modern_clarity() -> Vec<EffectConfig> {
        vec![
            EffectConfig {
                effect: crate::midi::EffectType::Compressor {
                    threshold: -14.0,
                    ratio: 4.0,
                    attack: 0.001,
                    release: 0.03,
                },
                intensity: 0.6,
                enabled: true,
            },
            EffectConfig {
                effect: crate::midi::EffectType::Filter {
                    filter_type: crate::midi::FilterType::HighPass,
                    cutoff: 80.0,
                    resonance: 0.3,
                    envelope_amount: 0.0,
                },
                intensity: 0.4,
                enabled: true,
            },
        ]
    }

    pub fn create_signature_effects_for_pad() -> Vec<EffectConfig> {
        vec![
            EffectConfig {
                effect: crate::midi::EffectType::Reverb {
                    room_size: 0.7,
                    dampening: 0.3,
                    wet_level: 0.4,
                    pre_delay: 0.04,
                },
                intensity: 0.6,
                enabled: true,
            },
            EffectConfig {
                effect: crate::midi::EffectType::Chorus {
                    rate: 0.8,
                    depth: 0.4,
                    feedback: 0.2,
                    stereo_width: 0.8,
                },
                intensity: 0.5,
                enabled: true,
            },
        ]
    }

    pub fn create_signature_effects_for_lead() -> Vec<EffectConfig> {
        vec![
            EffectConfig {
                effect: crate::midi::EffectType::Delay {
                    delay_time: 0.25,
                    feedback: 0.3,
                    wet_level: 0.25,
                    sync_tempo: false,
                },
                intensity: 0.5,
                enabled: true,
            },
            EffectConfig {
                effect: crate::midi::EffectType::Chorus {
                    rate: 1.5,
                    depth: 0.3,
                    feedback: 0.2,
                    stereo_width: 0.6,
                },
                intensity: 0.4,
                enabled: true,
            },
        ]
    }

    pub fn create_signature_effects_for_keys() -> Vec<EffectConfig> {
        vec![EffectConfig {
            effect: crate::midi::EffectType::Reverb {
                room_size: 0.4,
                dampening: 0.4,
                wet_level: 0.2,
                pre_delay: 0.02,
            },
            intensity: 0.5,
            enabled: true,
        }]
    }

    pub fn create_empty_signature_effects() -> Vec<EffectConfig> {
        Vec::new()
    }
}
