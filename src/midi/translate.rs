//! Turns a `SimpleSequence` into a `PlayCommand`: presets, musical time,
//! pre-rendered R2D2/synthesis buffers and a time-ordered MIDI event list.

use crate::expressive::{
    EffectsChain, EffectsPresetLibrary, ExpressiveSynth, NoteEvent, Patch, PatchLibrary,
    PresetLibrary, R2D2Emotion, R2D2Expression, R2D2Voice, SynthRef, render_length_seconds,
    render_patch,
};
use crate::midi::engine::{
    EventKind, PlayCommand, PlayMode, SAMPLE_RATE, SYNTH_BUS_GAIN, seconds_to_frames,
};
use crate::midi::parser::MidiNote;
use crate::midi::{EffectConfig, SimpleSequence};
use std::collections::HashMap;
use std::time::Duration;

/// GM controller numbers the note schema exposes, in the order they are sent.
#[allow(clippy::type_complexity)]
const CONTROLLERS: [(u8, fn(&MidiNote) -> Option<u8>); 7] = [
    (91, |n| n.reverb),
    (93, |n| n.chorus),
    (7, |n| n.volume),
    (10, |n| n.pan),
    (8, |n| n.balance),
    (11, |n| n.expression),
    (64, |n| n.sustain),
];

/// Time-ordered MIDI events for one call. Per channel, the first note sends a
/// program change (its instrument, or 0 so an unspecified instrument is still
/// piano when layering); controllers are sent only when a note specifies a
/// value that differs from what this call last sent. Channel 9 needs no bank
/// select: OxiSynth treats it as the drum channel and resolves bank 128.
pub(crate) fn midi_events(notes: &[MidiNote]) -> Vec<(u64, EventKind)> {
    let mut sorted: Vec<&MidiNote> = notes.iter().collect();
    sorted.sort_by_key(|n| n.start_time);

    let mut program: HashMap<u8, u8> = HashMap::new();
    let mut controller: HashMap<(u8, u8), u8> = HashMap::new();
    let mut events = Vec::with_capacity(notes.len() * 3);

    for note in sorted {
        let at = seconds_to_frames(note.start_time);
        let channel = note.channel;
        let wanted = match note.instrument {
            Some(p) => Some(p),
            None if !program.contains_key(&channel) => Some(0),
            None => None,
        };
        if let Some(p) = wanted
            && program.get(&channel) != Some(&p)
        {
            events.push((
                at,
                EventKind::ProgramChange {
                    channel,
                    program: p,
                },
            ));
            program.insert(channel, p);
        }
        for (number, get) in CONTROLLERS {
            if let Some(value) = get(note)
                && controller.get(&(channel, number)) != Some(&value)
            {
                events.push((
                    at,
                    EventKind::ControlChange {
                        channel,
                        controller: number,
                        value,
                    },
                ));
                controller.insert((channel, number), value);
            }
        }
        events.push((
            at,
            EventKind::NoteOn {
                channel,
                key: note.note,
                velocity: note.velocity,
            },
        ));
        let off = at + seconds_to_frames(note.duration).max(1);
        events.push((
            off,
            EventKind::NoteOff {
                channel,
                key: note.note,
            },
        ));
    }

    // Stable: keeps setup-before-note-on and off-before-next-on at equal frames.
    events.sort_by_key(|(at, _)| *at);
    events
}

/// One patch's group: its key, the resolved/inline patch, and every note
/// scheduled against it this call (absolute start time, event).
type PatchGroup = (String, Patch, Vec<(f64, NoteEvent)>);

/// Result of translating one call.
#[derive(Debug)]
pub struct Translation {
    pub command: PlayCommand,
    /// Time until the last note plus its effect tail; reported to the caller.
    pub duration: Duration,
}

pub struct Translator {
    preset_library: PresetLibrary,
    effects_library: EffectsPresetLibrary,
    patch_library: PatchLibrary,
    /// `Err(reason)` when no SoundFont is loaded; MIDI notes then fail here.
    midi_available: Result<(), String>,
}

impl Translator {
    pub fn new(midi_available: Result<(), String>) -> Self {
        Self {
            preset_library: PresetLibrary::new(),
            effects_library: EffectsPresetLibrary::new(),
            patch_library: PatchLibrary::new(),
            midi_available,
        }
    }

    /// Session patches first (they may shadow built-ins), then the library, then inline.
    fn resolve_patch<'a>(
        &'a self,
        reference: &'a SynthRef,
        session: &'a HashMap<String, Patch>,
    ) -> Result<&'a Patch, String> {
        match reference {
            SynthRef::Inline(patch) => {
                patch.validate()?;
                Ok(patch)
            }
            SynthRef::Name(name) => {
                let key = name.trim().to_lowercase();
                if let Some(p) = session.get(&key) {
                    return Ok(p);
                }
                if let Some(p) = self.patch_library.get(&key) {
                    return Ok(p);
                }
                let mut defined: Vec<&str> = session.values().map(|p| p.name.as_str()).collect();
                defined.sort_unstable();
                Err(format!(
                    "Unknown synth '{}'. Defined this session: {:?}. Call list_sounds with section \"synths\" for the built-in patches, or define_synth to create one.",
                    name, defined
                ))
            }
        }
    }

    pub fn translate(
        &self,
        sequence: SimpleSequence,
        mode: PlayMode,
        session_patches: &HashMap<String, Patch>,
    ) -> Result<Translation, String> {
        if sequence.notes.is_empty() {
            return Ok(Translation {
                command: PlayCommand {
                    mode,
                    ..Default::default()
                },
                duration: Duration::ZERO,
            });
        }

        // Presets, effects validation and musical time -> seconds.
        let mut processed_notes = Vec::new();
        for (i, mut note) in sequence.notes.into_iter().enumerate() {
            self.apply_preset_to_note(&mut note)
                .map_err(|e| format!("Note {}: {}", i + 1, e))?;
            if let Err(e) = note.validate_effects() {
                tracing::warn!("Invalid effects on note: {}", e);
                note.effects = None;
                note.effects_preset = None;
            }
            if note.start_time.is_none()
                && let Some(musical_time) = &note.musical_time
            {
                note.start_time =
                    Some(musical_time.to_seconds(sequence.tempo, sequence.beats_per_bar, 480));
            }
            if note.duration.is_none()
                && let Some(ref musical_duration) = note.musical_duration
            {
                note.duration =
                    Some(musical_duration.to_seconds(sequence.tempo, sequence.beats_per_bar));
            }
            processed_notes.push(note);
        }

        // Synthesis and R2D2 notes render their own effects into their sample
        // buffers. MIDI comes out of OxiSynth as one mixed bus, so the first
        // MIDI note that specifies effects defines the chain for that bus.
        let midi_effects: Option<Vec<EffectConfig>> = processed_notes
            .iter()
            .filter(|n| n.note_type != "r2d2" && !n.is_synthesis())
            .find_map(|n| n.effects.clone().filter(|e| !e.is_empty()));

        let mut midi_notes: Vec<MidiNote> = Vec::new();
        let mut buffers: Vec<(u64, Vec<[f32; 2]>)> = Vec::new();
        let mut note_end = Duration::ZERO;
        let expressive_synth = ExpressiveSynth::new();
        let r2d2_voice = R2D2Voice::new();

        // Notes referencing an agent-defined patch, grouped so every note of
        // one patch in this call renders into a single stereo buffer. `f64`
        // here is the note's absolute start time; insertion order is kept
        // with `patch_groups` alongside a lookup index for grouping.
        let mut patch_groups: Vec<PatchGroup> = Vec::new();
        let mut patch_group_index: HashMap<String, usize> = HashMap::new();

        for (i, note) in processed_notes.into_iter().enumerate() {
            // A negative start time would panic in `Duration`; treat it as 0.
            let start = Duration::from_secs_f64(note.start_time.unwrap_or(0.0).max(0.0));
            if let Some(reference) = note.synth.as_ref() {
                note.validate_synth()
                    .map_err(|e| format!("Note {}: {}", i + 1, e))?;
                let patch = self
                    .resolve_patch(reference, session_patches)
                    .map_err(|e| format!("Note {}: {}", i + 1, e))?;
                let abs_start = note.start_time.unwrap_or(0.0).max(0.0);
                let duration = note.duration.unwrap_or(1.0).max(0.0) as f32;
                let velocity = note.velocity.unwrap_or(100) as f32 / 127.0;
                let frequency = match note.note {
                    Some(n) => 440.0 * 2f32.powf((n as f32 - 69.0) / 12.0),
                    None if patch.has_pitched_engine() => {
                        return Err(format!(
                            "Note {}: synth '{}' is pitched, so it needs a MIDI note",
                            i + 1,
                            patch.name
                        ));
                    }
                    None => 0.0,
                };
                let event = NoteEvent {
                    start: 0.0, // corrected once the group's earliest note is known
                    duration,
                    frequency,
                    velocity,
                };
                let group_key = match reference {
                    SynthRef::Name(_) => patch.key(),
                    SynthRef::Inline(_) => serde_json::to_string(patch).map_err(|e| {
                        format!("Note {}: failed to key inline patch: {}", i + 1, e)
                    })?,
                };
                let idx = *patch_group_index
                    .entry(group_key.clone())
                    .or_insert_with(|| {
                        patch_groups.push((group_key, patch.clone(), Vec::new()));
                        patch_groups.len() - 1
                    });
                patch_groups[idx].2.push((abs_start, event));
            } else if note.note_type == "r2d2" {
                note.validate_r2d2()
                    .map_err(|e| format!("Invalid R2D2 note: {}", e))?;
                let emotion = parse_emotion(
                    note.r2d2_emotion
                        .as_deref()
                        .ok_or("R2D2 emotion is required")?,
                )?;
                let expression = R2D2Expression {
                    emotion,
                    intensity: note.r2d2_intensity.unwrap_or(0.7),
                    // A negative duration would panic in `Duration::from_secs_f32` below.
                    duration: note.duration.unwrap_or(1.0).max(0.0) as f32,
                    phrase_complexity: note.r2d2_complexity.unwrap_or(2),
                    pitch_range: match &note.r2d2_pitch_range {
                        Some(range) if range.len() == 2 => (range[0], range[1]),
                        _ => (200.0, 800.0),
                    },
                    context: note.r2d2_context,
                };
                let params = r2d2_voice
                    .generate_expression_params(&expression)
                    .ok_or("Failed to generate R2D2 synthesis parameters")?;
                let mut samples = expressive_synth.generate_r2d2_samples_with_contour(
                    params.base_freq,
                    expression.intensity,
                    params.duration,
                    &params.pitch_contour,
                );
                let mut chain =
                    EffectsChain::new(SAMPLE_RATE as f32, note.effects.as_deref().unwrap_or(&[]));
                if !chain.is_empty() {
                    samples.resize(samples.len() + SAMPLE_RATE as usize, 0.0);
                    chain.process_buffer(&mut samples);
                }
                note_end = note_end.max(start + Duration::from_secs_f32(expression.duration));
                buffers.push((
                    seconds_to_frames(start),
                    samples.into_iter().map(|s| [s, s]).collect(),
                ));
            } else if note.synth_type.is_some() {
                note.validate_synthesis()
                    .map_err(|e| format!("Invalid synthesis note: {}", e))?;
                let params = Self::convert_simple_note_to_synth_params(&note)?;
                let mut samples = expressive_synth
                    .generate_synthesized_samples(&params)
                    .map_err(|e| format!("Failed to generate synthesis samples: {}", e))?;
                for s in &mut samples {
                    *s *= SYNTH_BUS_GAIN;
                }
                note_end = note_end
                    .max(start + Duration::from_secs_f64(note.duration.unwrap_or(1.0).max(0.0)));
                buffers.push((
                    seconds_to_frames(start),
                    samples.into_iter().map(|s| [s, s]).collect(),
                ));
            } else if let Some(key) = note.note {
                let duration = Duration::from_secs_f64(note.duration.unwrap_or(1.0).max(0.0));
                note_end = note_end.max(start + duration);
                midi_notes.push(MidiNote {
                    note: key,
                    velocity: note.velocity.unwrap_or(80),
                    channel: note.channel,
                    start_time: start,
                    duration,
                    instrument: note.instrument,
                    reverb: note.reverb,
                    chorus: note.chorus,
                    volume: note.volume,
                    pan: note.pan,
                    balance: note.balance,
                    expression: note.expression,
                    sustain: note.sustain,
                });
            }
        }

        // Render one stereo buffer per patch group, scheduled at the
        // group's earliest note.
        for (_, patch, timed_events) in patch_groups {
            let first = timed_events
                .iter()
                .map(|(abs, _)| *abs)
                .fold(f64::INFINITY, f64::min);
            let events: Vec<NoteEvent> = timed_events
                .iter()
                .map(|(abs, event)| NoteEvent {
                    start: (*abs - first) as f32,
                    ..*event
                })
                .collect();
            let mut samples = render_patch(&patch, &events, SAMPLE_RATE as f32);
            for frame in &mut samples {
                frame[0] *= SYNTH_BUS_GAIN;
                frame[1] *= SYNTH_BUS_GAIN;
            }
            buffers.push((seconds_to_frames(Duration::from_secs_f64(first)), samples));
            note_end = note_end.max(Duration::from_secs_f64(
                first + render_length_seconds(&patch, &events) as f64,
            ));
        }

        if !midi_notes.is_empty()
            && let Err(reason) = &self.midi_available
        {
            return Err(format!("MIDI notes need a SoundFont: {}", reason));
        }

        let duration = if midi_notes.is_empty() && buffers.is_empty() {
            Duration::ZERO
        } else {
            note_end + calculate_tail_time(&midi_notes)
        };
        tracing::info!(
            "Translated {} MIDI notes and {} buffers ({} mode), {:.2}s including tail",
            midi_notes.len(),
            buffers.len(),
            mode.as_str(),
            duration.as_secs_f64()
        );

        Ok(Translation {
            command: PlayCommand {
                events: midi_events(&midi_notes),
                buffers,
                midi_effects,
                mode,
            },
            duration,
        })
    }

    /// Apply preset configuration to a SimpleNote
    fn apply_preset_to_note(&self, note: &mut crate::midi::SimpleNote) -> Result<(), String> {
        // Skip if no preset parameters are specified
        if note.preset_name.is_none()
            && note.preset_category.is_none()
            && !note.preset_random.unwrap_or(false)
        {
            return Ok(());
        }

        // Load preset based on parameters
        let preset = if let Some(preset_name) = &note.preset_name {
            // Load specific preset by name
            self.preset_library
                .load_preset(preset_name)
                .ok_or_else(|| format!("Preset '{}' not found", preset_name))?
        } else if let Some(category_str) = &note.preset_category {
            // Load random preset from category
            let category = match category_str.as_str() {
                "bass" => crate::expressive::PresetCategory::Bass,
                "pad" => crate::expressive::PresetCategory::Pad,
                "lead" => crate::expressive::PresetCategory::Lead,
                "keys" => crate::expressive::PresetCategory::Keys,
                "drums" => crate::expressive::PresetCategory::Drums,
                "effects" => crate::expressive::PresetCategory::Effects,
                _ => return Err(format!("Unknown preset category: {}", category_str)),
            };

            self.preset_library
                .get_random_preset(Some(category))
                .ok_or_else(|| format!("No presets found in category '{}'", category_str))?
        } else if note.preset_random.unwrap_or(false) {
            // Load completely random preset
            self.preset_library
                .get_random_preset(None)
                .ok_or("No presets available for random selection")?
        } else {
            return Ok(()); // No valid preset selection
        };

        // Apply preset variation if specified
        let synth_params = if let Some(variation_name) = &note.preset_variation {
            self.preset_library
                .apply_variation(&preset.name, variation_name)
                .unwrap_or_else(|| preset.synth_params.clone())
        } else {
            preset.synth_params.clone()
        };

        // Apply preset parameters to the note (convert from SynthParams to SimpleNote fields)
        note.synth_type = Some(
            match &synth_params.synth_type {
                crate::expressive::SynthType::Sine => "sine",
                crate::expressive::SynthType::Square { .. } => "square",
                crate::expressive::SynthType::Sawtooth => "sawtooth",
                crate::expressive::SynthType::Triangle => "triangle",
                crate::expressive::SynthType::Noise { .. } => "noise",
                crate::expressive::SynthType::FM { .. } => "fm",
                crate::expressive::SynthType::DX7FM { .. } => "dx7fm",
                crate::expressive::SynthType::Granular { .. } => "granular",
                crate::expressive::SynthType::Wavetable { .. } => "wavetable",
                crate::expressive::SynthType::Kick { .. } => "kick",
                crate::expressive::SynthType::Snare { .. } => "snare",
                crate::expressive::SynthType::HiHat { .. } => "hihat",
                crate::expressive::SynthType::Cymbal { .. } => "cymbal",
                crate::expressive::SynthType::Swoosh { .. } => "swoosh",
                crate::expressive::SynthType::Zap { .. } => "zap",
                crate::expressive::SynthType::Chime { .. } => "chime",
                crate::expressive::SynthType::Burst { .. } => "burst",
                crate::expressive::SynthType::Pad { .. } => "pad",
                crate::expressive::SynthType::Texture { .. } => "texture",
                crate::expressive::SynthType::Drone { .. } => "drone",
            }
            .to_string(),
        );

        // Apply envelope parameters
        note.synth_attack = Some(synth_params.envelope.attack);
        note.synth_decay = Some(synth_params.envelope.decay);
        note.synth_sustain = Some(synth_params.envelope.sustain);
        note.synth_release = Some(synth_params.envelope.release);

        // Apply amplitude
        note.synth_amplitude = Some(synth_params.amplitude);

        // Apply filter parameters if present
        if let Some(filter) = &synth_params.filter {
            note.synth_filter_type = Some(
                match filter.filter_type {
                    crate::expressive::FilterType::LowPass => "lowpass",
                    crate::expressive::FilterType::HighPass => "highpass",
                    crate::expressive::FilterType::BandPass => "bandpass",
                }
                .to_string(),
            );
            note.synth_filter_cutoff = Some(filter.cutoff);
            note.synth_filter_resonance = Some(filter.resonance);
        }

        // Apply synthesis-specific parameters based on synth type
        match &synth_params.synth_type {
            crate::expressive::SynthType::Square { pulse_width } => {
                note.synth_pulse_width = Some(*pulse_width);
            }
            crate::expressive::SynthType::FM {
                modulator_freq,
                modulation_index,
            } => {
                note.synth_modulator_freq = Some(*modulator_freq);
                note.synth_modulation_index = Some(*modulation_index);
            }
            crate::expressive::SynthType::Granular { grain_size, .. } => {
                note.synth_grain_size = Some(*grain_size);
            }
            crate::expressive::SynthType::Texture { roughness, .. } => {
                note.synth_texture_roughness = Some(*roughness);
            }
            _ => {} // Other synth types don't have specific parameters to set
        }

        // Preset effects (built-in reverb/chorus) plus the preset's signature
        // chain apply unless the caller supplied an explicit effects list.
        if note.effects.is_none() {
            let mut effects = synth_params.effects.clone();
            effects.extend(preset.signature_effects.iter().cloned());
            if !effects.is_empty() {
                note.effects = Some(effects);
            }
        }

        tracing::info!("Applied preset '{}' to note", preset.name);

        // Apply effects preset if specified
        if let Some(effects_preset_name) = &note.effects_preset {
            if let Some(effects) = self.effects_library.get_preset(effects_preset_name) {
                // Merge with existing effects or replace
                if let Some(existing_effects) = &mut note.effects {
                    existing_effects.extend(effects.clone());
                } else {
                    note.effects = Some(effects.clone());
                }
                tracing::info!("Applied effects preset '{}' to note", effects_preset_name);
            } else {
                tracing::warn!("Effects preset '{}' not found", effects_preset_name);
            }
        }

        Ok(())
    }

    /// Convert SimpleNote to SynthParams for the ExpressiveSynth
    fn convert_simple_note_to_synth_params(
        note: &crate::midi::SimpleNote,
    ) -> Result<crate::expressive::SynthParams, String> {
        use crate::expressive::{
            EnvelopeParams, FilterParams, FilterType, NoiseColor, SynthParams, SynthType,
        };

        let synth_type_str = note
            .synth_type
            .as_ref()
            .ok_or("Synthesis type is required")?;

        // Parse synthesis type
        let synth_type = match synth_type_str.as_str() {
            "sine" => SynthType::Sine,
            "square" => SynthType::Square {
                pulse_width: note.synth_pulse_width.unwrap_or(0.5),
            },
            "sawtooth" => SynthType::Sawtooth,
            "triangle" => SynthType::Triangle,
            "noise" => SynthType::Noise {
                color: NoiseColor::White,
            },
            "fm" => SynthType::FM {
                modulator_freq: note.synth_modulator_freq.unwrap_or(440.0),
                modulation_index: note.synth_modulation_index.unwrap_or(1.0),
            },
            "dx7fm" => {
                // Import DX7Operator for default configuration
                use crate::expressive::DX7Operator;

                SynthType::DX7FM {
                    algorithm: 1, // Default algorithm
                    operators: [
                        // Default 2-operator FM configuration
                        DX7Operator {
                            frequency_ratio: 1.0,
                            output_level: 0.8,
                            detune: 0.0,
                            envelope: crate::expressive::EnvelopeParams {
                                attack: note.synth_attack.unwrap_or(0.01),
                                decay: note.synth_decay.unwrap_or(0.1),
                                sustain: note.synth_sustain.unwrap_or(0.7),
                                release: note.synth_release.unwrap_or(0.3),
                            },
                        },
                        DX7Operator {
                            frequency_ratio: note.synth_modulator_freq.unwrap_or(440.0) / 440.0, // Convert to ratio
                            output_level: note.synth_modulation_index.unwrap_or(1.0) * 0.5, // Scale modulation index
                            detune: 0.0,
                            envelope: crate::expressive::EnvelopeParams {
                                attack: 0.001,
                                decay: 0.1,
                                sustain: 0.3,
                                release: 0.2,
                            },
                        },
                        // Unused operators
                        DX7Operator::default(),
                        DX7Operator::default(),
                        DX7Operator::default(),
                        DX7Operator::default(),
                    ],
                }
            }
            "granular" => SynthType::Granular {
                grain_size: note.synth_grain_size.unwrap_or(0.1),
                overlap: 0.5,
                density: 1.0,
            },
            "wavetable" => SynthType::Wavetable {
                position: 0.0,
                morph_speed: 1.0,
            },
            "kick" => SynthType::Kick {
                punch: 0.8,
                sustain: 0.3,
                click_freq: 8000.0,
                body_freq: 60.0,
            },
            "snare" => SynthType::Snare {
                snap: 0.7,
                buzz: 0.6,
                tone_freq: 200.0,
                noise_amount: 0.8,
            },
            "hihat" => SynthType::HiHat {
                metallic: 0.8,
                decay: 0.15,
                brightness: 0.9,
            },
            "cymbal" => SynthType::Cymbal {
                size: 0.7,
                metallic: 0.9,
                strike_intensity: 0.8,
            },
            "swoosh" => SynthType::Swoosh {
                direction: 0.0,
                intensity: 0.7,
                frequency_sweep: (200.0, 2000.0),
            },
            "zap" => SynthType::Zap {
                energy: 0.8,
                decay: 0.3,
                harmonic_content: 0.7,
            },
            "chime" => SynthType::Chime {
                fundamental: note.synth_frequency.unwrap_or(440.0),
                harmonic_count: 5,
                decay: 0.5,
                inharmonicity: 0.1,
            },
            "burst" => SynthType::Burst {
                center_freq: note.synth_frequency.unwrap_or(1000.0),
                bandwidth: 500.0,
                intensity: 0.8,
                shape: 0.5,
            },
            "pad" => SynthType::Pad {
                warmth: 0.7,
                movement: 0.3,
                space: 0.6,
                harmonic_evolution: 0.4,
            },
            "texture" => SynthType::Texture {
                roughness: note.synth_texture_roughness.unwrap_or(0.5),
                evolution: 0.3,
                spectral_tilt: 0.0,
                modulation_depth: 0.4,
            },
            "drone" => SynthType::Drone {
                fundamental: note.synth_frequency.unwrap_or(110.0),
                overtone_spread: 0.5,
                modulation: 0.3,
            },
            _ => return Err(format!("Unknown synthesis type: {}", synth_type_str)),
        };

        // Determine frequency (synthesis frequency overrides MIDI note, drums need specific frequencies)
        let frequency = if let Some(synth_freq) = note.synth_frequency {
            synth_freq
        } else if let Some(midi_note) = note.note {
            // Convert MIDI note to frequency
            440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
        } else {
            // Use appropriate frequencies for drum types and other synthesis
            match synth_type_str.as_str() {
                "kick" => 60.0,     // Low fundamental for kick drum
                "snare" => 200.0,   // Mid-range for snare body
                "hihat" => 8000.0,  // High frequency for hi-hat metallic sound
                "cymbal" => 4000.0, // Upper-mid for cymbal brightness
                "swoosh" => 1000.0, // Mid-range for swoosh effects
                "zap" => 800.0,     // Upper-mid for zap energy
                "chime" => 880.0,   // Musical frequency for chimes
                "burst" => 1000.0,  // Mid-range for burst
                _ => 440.0,         // Fallback for other synthesis types
            }
        };

        // Create envelope
        let envelope = EnvelopeParams {
            attack: note.synth_attack.unwrap_or(0.01),
            decay: note.synth_decay.unwrap_or(0.1),
            sustain: note.synth_sustain.unwrap_or(0.7),
            release: note.synth_release.unwrap_or(0.3),
        };

        // Create filter if specified
        let filter = if note.synth_filter_type.is_some() || note.synth_filter_cutoff.is_some() {
            let filter_type = match note.synth_filter_type.as_deref().unwrap_or("lowpass") {
                "lowpass" => FilterType::LowPass,
                "highpass" => FilterType::HighPass,
                "bandpass" => FilterType::BandPass,
                _ => FilterType::LowPass,
            };

            Some(FilterParams {
                cutoff: note.synth_filter_cutoff.unwrap_or(1000.0),
                resonance: note.synth_filter_resonance.unwrap_or(0.1),
                filter_type,
            })
        } else {
            None
        };

        // Effects: shorthand synth_reverb/chorus/delay fields plus any explicit chain.
        let mut effects: Vec<crate::midi::EffectConfig> = Vec::new();
        if let Some(reverb) = note.synth_reverb
            && reverb > 0.0
        {
            effects.push(PresetLibrary::create_reverb(reverb));
        }
        if let Some(chorus) = note.synth_chorus
            && chorus > 0.0
        {
            effects.push(PresetLibrary::create_chorus(chorus));
        }
        if let Some(delay) = note.synth_delay
            && delay > 0.0
        {
            effects.push(crate::midi::EffectConfig {
                effect: crate::midi::EffectType::Delay {
                    delay_time: note.synth_delay_time.unwrap_or(0.25),
                    feedback: 0.35,
                    wet_level: 0.5,
                    sync_tempo: false,
                },
                intensity: delay,
                enabled: true,
            });
        }
        if let Some(chain) = &note.effects {
            effects.extend(chain.iter().filter(|e| e.enabled).cloned());
        }

        Ok(SynthParams {
            synth_type,
            frequency,
            amplitude: note.synth_amplitude.unwrap_or(0.7),
            duration: note.duration.unwrap_or(1.0) as f32,
            envelope,
            filter,
            effects,
        })
    }
}

fn parse_emotion(name: &str) -> Result<R2D2Emotion, String> {
    Ok(match name {
        "Happy" => R2D2Emotion::Happy,
        "Sad" => R2D2Emotion::Sad,
        "Excited" => R2D2Emotion::Excited,
        "Worried" => R2D2Emotion::Worried,
        "Curious" => R2D2Emotion::Curious,
        "Affirmative" => R2D2Emotion::Affirmative,
        "Negative" => R2D2Emotion::Negative,
        "Surprised" => R2D2Emotion::Surprised,
        "Thoughtful" => R2D2Emotion::Thoughtful,
        _ => return Err(format!("Unknown R2D2 emotion: {}", name)),
    })
}

/// Calculate additional tail time needed for effects like reverb, chorus, sustain, and natural decay
fn calculate_tail_time(notes: &[MidiNote]) -> Duration {
    let mut max_tail_seconds: f64 = 2.0; // Base tail time for natural instrument decay

    // Check for reverb effects
    let has_reverb = notes.iter().any(|note| note.reverb.is_some_and(|r| r > 0));
    if has_reverb {
        let max_reverb = notes
            .iter()
            .filter_map(|note| note.reverb)
            .max()
            .unwrap_or(0);
        // Reverb can add 1-6 seconds of tail depending on depth
        let reverb_tail = 1.0 + (max_reverb as f64 / 127.0) * 5.0;
        max_tail_seconds = max_tail_seconds.max(reverb_tail);
    }

    // Check for chorus effects
    let has_chorus = notes.iter().any(|note| note.chorus.is_some_and(|c| c > 0));
    if has_chorus {
        let max_chorus = notes
            .iter()
            .filter_map(|note| note.chorus)
            .max()
            .unwrap_or(0);
        // Chorus can add 0.5-2 seconds of tail
        let chorus_tail = 0.5 + (max_chorus as f64 / 127.0) * 1.5;
        max_tail_seconds = max_tail_seconds.max(chorus_tail);
    }

    // Check for sustain pedal
    let has_sustain = notes.iter().any(|note| note.sustain.is_some_and(|s| s > 0));
    if has_sustain {
        // Sustain pedal can significantly extend notes
        max_tail_seconds = max_tail_seconds.max(4.0);
    }

    // Check for instruments that naturally have long decay
    for note in notes {
        if let Some(instrument) = note.instrument {
            let additional_tail = match instrument {
                // Piano family - long sustain and decay
                0..=7 => 3.0,
                // Organ family - can sustain indefinitely
                16..=23 => 2.0,
                // Guitar family - natural sustain
                24..=31 => 2.5,
                // Strings - natural decay
                40..=47 => 2.0,
                // Choir/Voice - natural decay
                52..=55 => 1.5,
                // Brass - can have long release
                56..=63 => 1.5,
                // Woodwinds - shorter decay
                64..=71 => 1.0,
                // Synth pads - often have long release
                88..=95 => 3.0,
                // Sound effects - variable
                120..=127 => 2.0,
                _ => 0.5,
            };
            max_tail_seconds = max_tail_seconds.max(additional_tail);
        }
    }

    Duration::from_secs_f64(max_tail_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn note(start: f64, dur: f64, instrument: Option<u8>, reverb: Option<u8>) -> MidiNote {
        MidiNote {
            note: 60,
            velocity: 100,
            channel: 0,
            start_time: Duration::from_secs_f64(start),
            duration: Duration::from_secs_f64(dur),
            instrument,
            reverb,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
        }
    }

    #[test]
    fn setup_events_precede_note_on_and_are_deduplicated_per_channel() {
        let events = midi_events(&[
            note(0.25, 0.5, None, Some(40)),
            note(0.0, 0.5, Some(73), Some(40)),
        ]);
        assert_eq!(
            events,
            vec![
                (
                    0,
                    EventKind::ProgramChange {
                        channel: 0,
                        program: 73
                    }
                ),
                (
                    0,
                    EventKind::ControlChange {
                        channel: 0,
                        controller: 91,
                        value: 40
                    }
                ),
                (
                    0,
                    EventKind::NoteOn {
                        channel: 0,
                        key: 60,
                        velocity: 100
                    }
                ),
                (
                    11_025,
                    EventKind::NoteOn {
                        channel: 0,
                        key: 60,
                        velocity: 100
                    }
                ),
                (
                    22_050,
                    EventKind::NoteOff {
                        channel: 0,
                        key: 60
                    }
                ),
                (
                    33_075,
                    EventKind::NoteOff {
                        channel: 0,
                        key: 60
                    }
                ),
            ]
        );
    }

    #[test]
    fn an_unspecified_instrument_means_program_0_on_first_use_only() {
        let events = midi_events(&[note(0.0, 0.1, None, None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(
            programs,
            vec![&(
                0,
                EventKind::ProgramChange {
                    channel: 0,
                    program: 0
                }
            )]
        );

        let events = midi_events(&[note(0.0, 0.1, Some(48), None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(
            programs.len(),
            1,
            "a later note without instrument keeps the channel's program"
        );
    }

    use crate::midi::engine::{PlayMode, SYNTH_BUS_GAIN};
    use crate::midi::{SimpleNote, SimpleSequence};
    use serde_json::json;

    fn seq(notes: Vec<SimpleNote>) -> SimpleSequence {
        SimpleSequence {
            notes,
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    fn no_session() -> HashMap<String, crate::expressive::Patch> {
        HashMap::new()
    }

    fn patch_note(synth: serde_json::Value, note: u8, start: f64, dur: f64) -> SimpleNote {
        SimpleNote {
            note: Some(note),
            velocity: Some(100),
            start_time: Some(start),
            duration: Some(dur),
            synth: Some(serde_json::from_value(synth).unwrap()),
            ..Default::default()
        }
    }

    #[test]
    fn synthesis_notes_become_buffers_at_bus_level() {
        let translator = Translator::new(Ok(()));
        let t = translator
            .translate(
                seq(vec![SimpleNote {
                    synth_type: Some("sine".into()),
                    synth_frequency: Some(440.0),
                    synth_amplitude: Some(0.8),
                    start_time: Some(0.5),
                    duration: Some(0.2),
                    ..Default::default()
                }]),
                PlayMode::Layer,
                &no_session(),
            )
            .unwrap();
        assert_eq!(t.command.mode, PlayMode::Layer);
        assert!(t.command.events.is_empty());
        assert_eq!(t.command.buffers.len(), 1);
        let (offset, samples) = &t.command.buffers[0];
        assert_eq!(*offset, 22_050);
        let peak = samples
            .iter()
            .fold(0.0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()));
        assert!(
            peak <= 0.8 * SYNTH_BUS_GAIN + 0.01,
            "bus gain not applied: peak {peak}"
        );
        assert!(
            t.duration >= Duration::from_secs_f64(0.7),
            "duration must include the tail"
        );
    }

    #[test]
    fn midi_notes_need_a_soundfont_but_synthesis_does_not() {
        let translator = Translator::new(Err("SoundFont not found".into()));
        let midi = seq(vec![SimpleNote {
            note: Some(60),
            duration: Some(0.1),
            ..Default::default()
        }]);
        let err = translator
            .translate(midi, PlayMode::Replace, &no_session())
            .unwrap_err();
        assert!(err.contains("SoundFont not found"), "{err}");

        let synth = seq(vec![SimpleNote {
            synth_type: Some("sine".into()),
            synth_frequency: Some(440.0),
            duration: Some(0.1),
            ..Default::default()
        }]);
        assert!(
            translator
                .translate(synth, PlayMode::Replace, &no_session())
                .is_ok()
        );
    }

    #[test]
    fn presets_are_applied_and_unknown_presets_are_errors() {
        let translator = Translator::new(Ok(()));
        let ok = translator.translate(
            seq(vec![SimpleNote {
                preset_name: Some("Minimoog Bass".into()),
                note: Some(36),
                duration: Some(0.2),
                ..Default::default()
            }]),
            PlayMode::Replace,
            &no_session(),
        );
        assert_eq!(ok.unwrap().command.buffers.len(), 1);

        let err = translator
            .translate(
                seq(vec![SimpleNote {
                    preset_name: Some("Definitely Not A Preset".into()),
                    note: Some(36),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap_err();
        assert!(err.contains("Definitely Not A Preset"), "{err}");
    }

    #[test]
    fn musical_time_is_converted_with_the_sequence_tempo() {
        let translator = Translator::new(Ok(()));
        let mut s = seq(vec![SimpleNote {
            note: Some(60),
            // `SimpleNote::default()` fills these in; musical time only applies
            // when the caller left the seconds fields unset.
            start_time: None,
            duration: None,
            musical_time: Some(crate::midi::MusicalTime {
                bar: 2,
                beat: 1,
                tick: 0,
            }),
            musical_duration: Some(crate::midi::MusicalDuration::Bars(1.0)),
            ..Default::default()
        }]);
        s.tempo = 60;
        s.beats_per_bar = 4;
        let t = translator
            .translate(s, PlayMode::Replace, &no_session())
            .unwrap();
        let on = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOn { .. }))
            .unwrap();
        assert_eq!(on.0, 4 * 44_100, "bar 2 at 60 BPM in 4/4 starts at 4 s");
        let off = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOff { .. }))
            .unwrap();
        assert_eq!(off.0, 8 * 44_100);
    }

    #[test]
    fn an_empty_sequence_translates_to_nothing() {
        let t = Translator::new(Ok(()))
            .translate(seq(vec![]), PlayMode::Replace, &no_session())
            .unwrap();
        assert!(t.command.events.is_empty() && t.command.buffers.is_empty());
        assert_eq!(t.duration, Duration::ZERO);
    }

    #[test]
    fn a_zero_length_note_still_gets_its_note_off_after_note_on() {
        let events = midi_events(&[note(0.0, 0.0, Some(0), None)]);
        assert_eq!(
            events[1],
            (
                0,
                EventKind::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100
                }
            )
        );
        assert_eq!(
            events[2],
            (
                1,
                EventKind::NoteOff {
                    channel: 0,
                    key: 60
                }
            )
        );
    }

    fn reverb() -> EffectConfig {
        serde_json::from_value(serde_json::json!({
            "type": "reverb", "room_size": 0.6, "intensity": 0.4
        }))
        .unwrap()
    }

    #[test]
    fn a_negative_start_time_is_clamped_instead_of_panicking() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    velocity: Some(100),
                    duration: Some(0.5),
                    start_time: Some(-1.0),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert_eq!(
            t.command.events.first().map(|(at, _)| *at),
            Some(0),
            "a negative start time must land at offset 0"
        );
    }

    #[test]
    fn a_negative_duration_on_a_midi_note_is_clamped_instead_of_panicking() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    velocity: Some(100),
                    duration: Some(-1.0),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let note_off = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOff { .. }))
            .expect("a note off must still be scheduled");
        assert_eq!(
            note_off.0, 1,
            "a negative duration must clamp to 0 and hit the max(1) floor in midi_events"
        );
    }

    #[test]
    fn a_negative_duration_on_a_synthesis_note_does_not_panic() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    synth_type: Some("sine".into()),
                    synth_frequency: Some(440.0),
                    duration: Some(-1.0),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert!(t.command.buffers.len() <= 1);
        if let Some((_, samples)) = t.command.buffers.first() {
            assert!(
                samples.len() <= 44_100,
                "expected an empty or very short buffer, got {} samples",
                samples.len()
            );
        }
    }

    #[test]
    fn the_bus_chain_comes_from_the_first_midi_note_that_has_effects() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![
                    SimpleNote {
                        note: Some(60),
                        duration: Some(0.2),
                        ..Default::default()
                    },
                    SimpleNote {
                        note: Some(64),
                        duration: Some(0.2),
                        effects: Some(vec![reverb()]),
                        ..Default::default()
                    },
                    SimpleNote {
                        note: Some(67),
                        duration: Some(0.2),
                        effects: Some(vec![
                            serde_json::from_value(serde_json::json!({
                                "type": "delay", "delay_time": 0.25,
                                "feedback": 0.3, "intensity": 0.5
                            }))
                            .unwrap(),
                        ]),
                        ..Default::default()
                    },
                ]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let chain = t
            .command
            .midi_effects
            .expect("a MIDI note supplied effects");
        assert_eq!(chain.len(), 1, "later MIDI effects must not join the chain");
        assert!(matches!(
            chain[0].effect,
            crate::midi::EffectType::Reverb { .. }
        ));
    }

    #[test]
    fn effects_on_synthesis_notes_never_reach_the_midi_bus() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![
                    SimpleNote {
                        synth_type: Some("sine".into()),
                        synth_frequency: Some(440.0),
                        duration: Some(0.2),
                        effects: Some(vec![reverb()]),
                        ..Default::default()
                    },
                    SimpleNote {
                        note: Some(60),
                        duration: Some(0.2),
                        ..Default::default()
                    },
                ]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert!(
            t.command.midi_effects.is_none(),
            "synthesis effects are rendered into the buffer, not onto the MIDI bus"
        );
    }

    fn r2d2_buffer_len(effects: Option<Vec<EffectConfig>>) -> usize {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note_type: "r2d2".to_string(),
                    r2d2_emotion: Some("Happy".to_string()),
                    r2d2_intensity: Some(0.7),
                    r2d2_complexity: Some(2),
                    duration: Some(0.5),
                    effects,
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert_eq!(t.command.buffers.len(), 1);
        t.command.buffers[0].1.len()
    }

    #[test]
    fn an_r2d2_buffer_carries_its_own_effect_tail() {
        let tail = 0.5 * SAMPLE_RATE as f64 + SAMPLE_RATE as f64;
        let with_effects = r2d2_buffer_len(Some(vec![reverb()]));
        assert!(
            with_effects as f64 >= tail,
            "expected room for a one-second tail, got {with_effects} samples"
        );
        let without = r2d2_buffer_len(None);
        assert!(
            (without as f64) < tail,
            "a dry R2D2 note must not be padded, got {without} samples"
        );
    }

    #[test]
    fn notes_on_the_same_patch_share_one_buffer_scheduled_at_the_first_note() {
        let t = Translator::new(Err("no soundfont".into()));
        let seq = seq(vec![
            patch_note(json!("minimoog_bass"), 36, 0.5, 0.5),
            patch_note(json!("minimoog_bass"), 43, 1.0, 0.5),
            patch_note(json!("tr_808_kick"), 36, 0.0, 0.25),
        ]);
        let tr = t.translate(seq, PlayMode::Replace, &no_session()).unwrap();
        assert_eq!(tr.command.buffers.len(), 2, "one buffer per patch");
        let starts: Vec<u64> = tr.command.buffers.iter().map(|b| b.0).collect();
        assert!(starts.contains(&0));
        assert!(starts.contains(&seconds_to_frames(Duration::from_secs_f64(0.5))));
        // Bass buffer: two notes -> ends at 1.5 s + release; the reported duration covers it.
        assert!(tr.duration.as_secs_f64() >= 1.5);
    }

    #[test]
    fn inline_patches_render_and_identical_inline_patches_group() {
        let t = Translator::new(Err("no soundfont".into()));
        let inline = json!({"name": "blip", "subtractive": {"osc1": {"wave": "square"}}});
        let seq = seq(vec![
            patch_note(inline.clone(), 60, 0.0, 0.2),
            patch_note(inline, 64, 0.2, 0.2),
        ]);
        let tr = t.translate(seq, PlayMode::Replace, &no_session()).unwrap();
        assert_eq!(tr.command.buffers.len(), 1);
        assert!(tr.command.buffers[0].1.iter().any(|s| s[0].abs() > 0.01));
    }

    #[test]
    fn session_patches_shadow_builtins_and_unknown_names_list_what_exists() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut session = no_session();
        let mine: crate::expressive::Patch =
            serde_json::from_value(json!({"name": "Mine", "percussion": {"kind": "snare"}}))
                .unwrap();
        session.insert(mine.key(), mine);
        let ok = t.translate(
            seq(vec![patch_note(json!("mine"), 38, 0.0, 0.2)]),
            PlayMode::Replace,
            &session,
        );
        assert!(ok.is_ok());
        let err = t
            .translate(
                seq(vec![patch_note(json!("nope"), 38, 0.0, 0.2)]),
                PlayMode::Replace,
                &session,
            )
            .unwrap_err();
        assert!(
            err.contains("nope") && err.contains("Mine") && err.contains("list_sounds"),
            "{err}"
        );
    }

    #[test]
    fn a_pitched_patch_needs_a_note_but_percussion_does_not() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut n = patch_note(json!("minimoog_bass"), 36, 0.0, 0.2);
        n.note = None;
        let err = t
            .translate(seq(vec![n]), PlayMode::Replace, &no_session())
            .unwrap_err();
        assert!(err.contains("note"), "{err}");
        let mut k = patch_note(json!("tr_808_kick"), 36, 0.0, 0.2);
        k.note = None;
        assert!(
            t.translate(seq(vec![k]), PlayMode::Replace, &no_session())
                .is_ok()
        );
    }

    #[test]
    fn an_invalid_inline_patch_is_an_error_naming_the_field() {
        let t = Translator::new(Err("no soundfont".into()));
        let bad = json!({"name": "hot", "subtractive": {"filter": {"cutoff": 99999}}});
        let err = t
            .translate(
                seq(vec![patch_note(bad, 60, 0.0, 0.2)]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap_err();
        assert!(err.contains("subtractive.filter.cutoff"), "{err}");
        assert!(err.starts_with("Note 1:"), "{err}");
    }
}
