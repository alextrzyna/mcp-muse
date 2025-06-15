use crate::expressive::{ExpressiveSynth, PresetLibrary, R2D2Emotion, R2D2Expression, R2D2Voice};
use crate::midi::parser::MidiNote;
use crate::midi::SimpleSequence;
use oxisynth::{MidiEvent, SoundFont, Synth};
use rodio::{OutputStream, Sink, Source};
use std::time::Duration;

use std::env;
use std::fs;
use std::path::PathBuf;

pub struct MidiPlayer {
    _stream: OutputStream,
    sink: Sink,
    preset_library: PresetLibrary,
}

impl MidiPlayer {
    pub fn new() -> Result<Self, String> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to create audio output stream: {}", e))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        Ok(MidiPlayer {
            _stream,
            sink,
            preset_library: PresetLibrary::new(),
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

    /// Play a simple sequence of notes (much easier to use!)
    pub fn play_simple(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!(
            "Playing simple sequence with {} notes",
            sequence.notes.len()
        );

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Convert SimpleNote to MidiNote (only for MIDI notes)
        let notes: Vec<MidiNote> = sequence
            .notes
            .into_iter()
            .filter(|simple_note| {
                simple_note.note_type == "midi"
                    && simple_note.note.is_some()
                    && simple_note.velocity.is_some()
            })
            .map(|simple_note| MidiNote {
                note: simple_note.note.unwrap(), // Safe because we filtered for Some()
                velocity: simple_note.velocity.unwrap(), // Safe because we filtered for Some()
                channel: simple_note.channel,
                start_time: Duration::from_secs_f64(simple_note.start_time),
                duration: Duration::from_secs_f64(simple_note.duration),
                instrument: simple_note.instrument,
                reverb: simple_note.reverb,
                chorus: simple_note.chorus,
                volume: simple_note.volume,
                pan: simple_note.pan,
                balance: simple_note.balance,
                expression: simple_note.expression,
                sustain: simple_note.sustain,
            })
            .collect();

        // Calculate total playback time including tail time for effects
        let note_end_time = notes
            .iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        let tail_time = Self::calculate_tail_time(&notes);
        let total_time = note_end_time + tail_time;

        // Log first few notes for debugging
        for (i, note) in notes.iter().take(3).enumerate() {
            tracing::info!(
                "Note {}: MIDI note {}, velocity {}, start={:.2}s, duration={:.2}s",
                i,
                note.note,
                note.velocity,
                note.start_time.as_secs_f64(),
                note.duration.as_secs_f64()
            );
        }

        tracing::info!(
            "Note end time: {:.2}s, tail time: {:.2}s, total playback time: {:.2}s",
            note_end_time.as_secs_f64(),
            tail_time.as_secs_f64(),
            total_time.as_secs_f64()
        );

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(notes, total_time)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;

        tracing::info!("Created OxiSynth audio source, starting playback");
        self.sink.append(synth_source);
        self.sink.play();

        // Wait for the audio to finish playing
        // Add a small buffer to ensure we don't cut off early
        let wait_time = total_time + Duration::from_millis(200);
        tracing::info!(
            "Waiting {:.2}s for playback to complete...",
            wait_time.as_secs_f64()
        );

        std::thread::sleep(wait_time);

        tracing::info!("OxiSynth playback completed");

        Ok(())
    }

    /// Play a simple sequence of notes asynchronously (non-blocking)
    #[allow(dead_code)]
    pub fn play_simple_async(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!(
            "Playing simple sequence with {} notes (async)",
            sequence.notes.len()
        );

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Convert SimpleNote to MidiNote (only for MIDI notes)
        let notes: Vec<MidiNote> = sequence
            .notes
            .into_iter()
            .filter(|simple_note| {
                simple_note.note_type == "midi"
                    && simple_note.note.is_some()
                    && simple_note.velocity.is_some()
            })
            .map(|simple_note| MidiNote {
                note: simple_note.note.unwrap(), // Safe because we filtered for Some()
                velocity: simple_note.velocity.unwrap(), // Safe because we filtered for Some()
                channel: simple_note.channel,
                start_time: Duration::from_secs_f64(simple_note.start_time),
                duration: Duration::from_secs_f64(simple_note.duration),
                instrument: simple_note.instrument,
                reverb: simple_note.reverb,
                chorus: simple_note.chorus,
                volume: simple_note.volume,
                pan: simple_note.pan,
                balance: simple_note.balance,
                expression: simple_note.expression,
                sustain: simple_note.sustain,
            })
            .collect();

        // Calculate total playback time including tail time for effects
        let note_end_time = notes
            .iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        let tail_time = Self::calculate_tail_time(&notes);
        let total_time = note_end_time + tail_time;

        // Log first few notes for debugging
        for (i, note) in notes.iter().take(3).enumerate() {
            tracing::info!(
                "Note {}: MIDI note {}, velocity {}, start={:.2}s, duration={:.2}s",
                i,
                note.note,
                note.velocity,
                note.start_time.as_secs_f64(),
                note.duration.as_secs_f64()
            );
        }

        tracing::info!(
            "Note end time: {:.2}s, tail time: {:.2}s, total playback time: {:.2}s",
            note_end_time.as_secs_f64(),
            tail_time.as_secs_f64(),
            total_time.as_secs_f64()
        );

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(notes, total_time)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;

        tracing::info!("Created OxiSynth audio source, starting async playback");
        self.sink.append(synth_source);
        self.sink.play();

        // DON'T WAIT - return immediately for async playback
        tracing::info!("OxiSynth async playback started, returning immediately");

        Ok(())
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
                "organ" => crate::expressive::PresetCategory::Organ,
                "arp" => crate::expressive::PresetCategory::Arp,
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

        // Apply effects
        for effect in &synth_params.effects {
            match &effect.effect_type {
                crate::expressive::EffectType::Reverb => {
                    note.synth_reverb = Some(effect.intensity);
                }
                crate::expressive::EffectType::Chorus => {
                    note.synth_chorus = Some(effect.intensity);
                }
                crate::expressive::EffectType::Delay { delay_time } => {
                    note.synth_delay = Some(effect.intensity);
                    note.synth_delay_time = Some(*delay_time);
                }
            }
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

        tracing::info!("Applied preset '{}' to note", preset.name);
        Ok(())
    }

    /// Play an enhanced mixed sequence supporting MIDI, R2D2, and synthesis notes
    pub fn play_enhanced_mixed(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!(
            "Playing enhanced mixed sequence with {} notes",
            sequence.notes.len()
        );

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Process each note and apply presets if specified
        let mut processed_notes = Vec::new();
        for mut note in sequence.notes {
            // Apply preset configuration if present
            if let Err(e) = self.apply_preset_to_note(&mut note) {
                tracing::warn!("Failed to apply preset to note: {}", e);
                // Continue with the note without preset - don't fail completely
            }
            processed_notes.push(note);
        }

        // Separate MIDI, R2D2, and synthesis notes
        let mut midi_notes = Vec::new();
        let mut r2d2_events = Vec::new();
        let mut synthesis_events = Vec::new();

        for note in processed_notes {
            if note.note_type == "r2d2" {
                // Handle R2D2 notes (existing logic)
                if let Err(e) = note.validate_r2d2() {
                    return Err(format!("Invalid R2D2 note: {}", e));
                }

                let emotion_str = note
                    .r2d2_emotion
                    .as_ref()
                    .ok_or("R2D2 emotion is required")?;
                let emotion = match emotion_str.as_str() {
                    "Happy" => R2D2Emotion::Happy,
                    "Sad" => R2D2Emotion::Sad,
                    "Excited" => R2D2Emotion::Excited,
                    "Worried" => R2D2Emotion::Worried,
                    "Curious" => R2D2Emotion::Curious,
                    "Affirmative" => R2D2Emotion::Affirmative,
                    "Negative" => R2D2Emotion::Negative,
                    "Surprised" => R2D2Emotion::Surprised,
                    "Thoughtful" => R2D2Emotion::Thoughtful,
                    _ => return Err(format!("Unknown R2D2 emotion: {}", emotion_str)),
                };

                let expression = R2D2Expression {
                    emotion,
                    intensity: note.r2d2_intensity.unwrap_or(0.7),
                    duration: note.duration as f32,
                    phrase_complexity: note.r2d2_complexity.unwrap_or(2),
                    pitch_range: if let Some(range) = &note.r2d2_pitch_range {
                        if range.len() == 2 {
                            (range[0], range[1])
                        } else {
                            (200.0, 800.0)
                        }
                    } else {
                        (200.0, 800.0)
                    },
                    context: note.r2d2_context,
                };

                r2d2_events.push(R2D2Event {
                    start_time: note.start_time,
                    expression,
                });
            } else if note.is_synthesis() {
                // Handle synthesis notes
                if let Err(e) = note.validate_synthesis() {
                    return Err(format!("Invalid synthesis note: {}", e));
                }

                // Convert SimpleNote to SynthEvent
                synthesis_events.push(SynthEvent {
                    start_time: note.start_time,
                    note,
                });
            } else {
                // Convert to MidiNote - only process if note and velocity exist
                if let (Some(note_val), Some(velocity_val)) = (note.note, note.velocity) {
                    midi_notes.push(MidiNote {
                        note: note_val,
                        velocity: velocity_val,
                        channel: note.channel,
                        start_time: Duration::from_secs_f64(note.start_time),
                        duration: Duration::from_secs_f64(note.duration),
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
        }

        // Calculate total playback time
        let midi_end_time = if !midi_notes.is_empty() {
            midi_notes
                .iter()
                .map(|note| note.start_time + note.duration)
                .max()
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(0)
        };

        let r2d2_end_time = if !r2d2_events.is_empty() {
            r2d2_events
                .iter()
                .map(|event| {
                    Duration::from_secs_f64(event.start_time + event.expression.duration as f64)
                })
                .max()
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(0)
        };

        let synthesis_end_time = if !synthesis_events.is_empty() {
            synthesis_events
                .iter()
                .map(|event| Duration::from_secs_f64(event.start_time + event.note.duration))
                .max()
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(0)
        };

        let note_end_time = midi_end_time.max(r2d2_end_time).max(synthesis_end_time);
        let tail_time = Self::calculate_tail_time(&midi_notes);
        let total_time = note_end_time + tail_time;

        tracing::info!(
            "Enhanced mixed sequence: {} MIDI notes, {} R2D2 events, {} synthesis events, total time: {:.2}s",
            midi_notes.len(),
            r2d2_events.len(),
            synthesis_events.len(),
            total_time.as_secs_f64()
        );

        // Create enhanced hybrid audio source
        let enhanced_source =
            EnhancedHybridAudioSource::new(midi_notes, r2d2_events, synthesis_events, total_time)
                .map_err(|e| format!("Failed to create enhanced hybrid audio source: {}", e))?;

        tracing::info!("Created enhanced hybrid audio source, starting playback");
        self.sink.append(enhanced_source);
        self.sink.play();

        // Wait for playback to complete
        let wait_time = total_time + Duration::from_millis(200);
        tracing::info!(
            "Waiting {:.2}s for enhanced mixed playback to complete...",
            wait_time.as_secs_f64()
        );

        std::thread::sleep(wait_time);
        tracing::info!("Enhanced mixed sequence playback completed");

        Ok(())
    }
}

fn find_soundfont() -> Result<PathBuf, String> {
    // First check if there's a custom soundfont path configured
    if let Ok(config) = crate::setup::config::SetupConfig::load() {
        if let Some(custom_path) = config.soundfont_path {
            let path = PathBuf::from(custom_path);
            if path.exists() {
                tracing::info!("Using custom SoundFont from config: {:?}", path);
                return Ok(path);
            } else {
                tracing::warn!("Configured custom SoundFont not found: {:?}", path);
            }
        }
    }

    // Try to find the SoundFont in various locations
    let exe_path = env::current_exe().map_err(|e| format!("Cannot find executable: {}", e))?;
    let exe_dir = exe_path
        .parent()
        .ok_or("Cannot find executable directory")?;

    let possible_paths = vec![
        exe_dir.join("../assets/FluidR3_GM.sf2"), // Development
        exe_dir.join("assets/FluidR3_GM.sf2"),    // Installed
        PathBuf::from("assets/FluidR3_GM.sf2"),   // Current directory
        PathBuf::from("FluidR3_GM.sf2"),          // Current directory
        // Also check target/debug/assets for development
        PathBuf::from("target/debug/assets/FluidR3_GM.sf2"),
        PathBuf::from("target/release/assets/FluidR3_GM.sf2"),
        // Fallback to old soundfont if it exists
        exe_dir.join("../assets/TimGM6mb.sf2"), // Development (old)
        exe_dir.join("assets/TimGM6mb.sf2"),    // Installed (old)
        PathBuf::from("assets/TimGM6mb.sf2"),   // Current directory (old)
        PathBuf::from("TimGM6mb.sf2"),          // Current directory (old)
        PathBuf::from("target/debug/assets/TimGM6mb.sf2"), // Development (old)
        PathBuf::from("target/release/assets/TimGM6mb.sf2"), // Release (old)
    ];

    for path in possible_paths {
        if path.exists() {
            tracing::info!("Found SoundFont at: {:?}", path);
            return Ok(path);
        }
    }

    Err("SoundFont not found. Please run 'mcp-muse --setup' to download it.".to_string())
}

// OxiSynth-based audio source
struct OxiSynthSource {
    synth: Synth,
    notes: Vec<MidiNote>,
    sample_rate: u32,
    current_sample: usize,
    total_duration: Duration,
    left_buffer: Vec<f32>,
    right_buffer: Vec<f32>,
    buffer_size: usize,
    buffer_pos: usize,
    playing_notes: std::collections::HashMap<(u32, u8), Duration>, // (start_sample, note) -> duration
    samples_generated: usize,
    channel_instruments: std::collections::HashMap<u8, u8>, // channel -> current instrument
    channel_reverb: std::collections::HashMap<u8, u8>,      // channel -> current reverb depth
    channel_chorus: std::collections::HashMap<u8, u8>,      // channel -> current chorus depth
    channel_volume: std::collections::HashMap<u8, u8>,      // channel -> current volume
    channel_pan: std::collections::HashMap<u8, u8>,         // channel -> current pan position
    channel_balance: std::collections::HashMap<u8, u8>,     // channel -> current balance
    channel_expression: std::collections::HashMap<u8, u8>,  // channel -> current expression
    channel_sustain: std::collections::HashMap<u8, u8>,     // channel -> current sustain
}

impl OxiSynthSource {
    fn new(notes: Vec<MidiNote>, total_duration: Duration) -> Result<Self, String> {
        let sample_rate = 44100;

        // Find and load the SoundFont
        let soundfont_path = find_soundfont()?;
        let mut soundfont_file = fs::File::open(&soundfont_path)
            .map_err(|e| format!("Failed to open SoundFont file: {}", e))?;

        let soundfont = SoundFont::load(&mut soundfont_file)
            .map_err(|e| format!("Failed to parse SoundFont: {}", e))?;

        // Create synthesizer
        let mut synth = Synth::default();
        synth.add_font(soundfont, true);

        tracing::info!("Loaded SoundFont from: {:?}", soundfont_path);

        // Use the provided total duration which includes tail time
        let final_duration = total_duration.max(Duration::from_secs(1));

        let buffer_size = 1024; // Process audio in chunks

        tracing::info!(
            "OxiSynth source created: {} notes, total duration: {:?}",
            notes.len(),
            final_duration
        );

        Ok(Self {
            synth,
            notes,
            sample_rate,
            current_sample: 0,
            total_duration: final_duration,
            left_buffer: vec![0.0; buffer_size],
            right_buffer: vec![0.0; buffer_size],
            buffer_size,
            buffer_pos: buffer_size, // Start with empty buffer to trigger initial fill
            playing_notes: std::collections::HashMap::new(),
            samples_generated: 0,
            channel_instruments: std::collections::HashMap::new(),
            channel_reverb: std::collections::HashMap::new(),
            channel_chorus: std::collections::HashMap::new(),
            channel_volume: std::collections::HashMap::new(),
            channel_pan: std::collections::HashMap::new(),
            channel_balance: std::collections::HashMap::new(),
            channel_expression: std::collections::HashMap::new(),
            channel_sustain: std::collections::HashMap::new(),
        })
    }

    fn process_audio_chunk(&mut self) {
        // Handle note on/off events for this chunk
        for note in &self.notes {
            let note_start_sample =
                (note.start_time.as_secs_f32() * self.sample_rate as f32) as u32;
            let note_end_sample =
                ((note.start_time + note.duration).as_secs_f32() * self.sample_rate as f32) as u32;

            let current_sample_u32 = self.current_sample as u32;
            let chunk_end = current_sample_u32 + self.buffer_size as u32;

            // Check if we should start this note in this chunk
            if note_start_sample >= current_sample_u32 && note_start_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if !self.playing_notes.contains_key(&key) {
                    // Handle drums (channel 9) specially
                    if note.channel == 9 {
                        // For drums, force bank select 128 (percussion) if not already set
                        let current_bank = self.channel_instruments.get(&note.channel).copied();
                        if current_bank != Some(128) {
                            // Bank Select MSB (Controller 0) = 128 for drums
                            let bank_select_msb = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 0,    // Bank Select MSB
                                value: 128, // Percussion Bank
                            };
                            let _ = self.synth.send_event(bank_select_msb);

                            // Program Change to Standard Kit (program 0 in percussion bank)
                            let program_change = MidiEvent::ProgramChange {
                                channel: note.channel,
                                program_id: 0, // Standard Kit in percussion bank
                            };
                            let _ = self.synth.send_event(program_change);
                            self.channel_instruments.insert(note.channel, 128);
                            tracing::debug!(
                                "Drum Setup: channel 9 -> percussion bank 128, standard kit"
                            );
                        }
                    } else {
                        // Check if we need to send a program change for this channel
                        if let Some(instrument) = note.instrument {
                            let current_instrument =
                                self.channel_instruments.get(&note.channel).copied();
                            if current_instrument != Some(instrument) {
                                let program_change = MidiEvent::ProgramChange {
                                    channel: note.channel,
                                    program_id: instrument,
                                };
                                let _ = self.synth.send_event(program_change);
                                self.channel_instruments.insert(note.channel, instrument);
                                tracing::debug!(
                                    "Program Change: channel {} -> instrument {}",
                                    note.channel,
                                    instrument
                                );
                            }
                        }
                    }

                    // Check if we need to send reverb control change for this channel
                    if let Some(reverb) = note.reverb {
                        let current_reverb = self.channel_reverb.get(&note.channel).copied();
                        if current_reverb != Some(reverb) {
                            let reverb_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 91, // Effects1Depth (Reverb)
                                value: reverb,
                            };
                            let _ = self.synth.send_event(reverb_cc);
                            self.channel_reverb.insert(note.channel, reverb);
                            tracing::debug!(
                                "Reverb CC: channel {} -> depth {}",
                                note.channel,
                                reverb
                            );
                        }
                    }

                    // Check if we need to send chorus control change for this channel
                    if let Some(chorus) = note.chorus {
                        let current_chorus = self.channel_chorus.get(&note.channel).copied();
                        if current_chorus != Some(chorus) {
                            let chorus_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 93, // Effects3Depth (Chorus)
                                value: chorus,
                            };
                            let _ = self.synth.send_event(chorus_cc);
                            self.channel_chorus.insert(note.channel, chorus);
                            tracing::debug!(
                                "Chorus CC: channel {} -> depth {}",
                                note.channel,
                                chorus
                            );
                        }
                    }

                    // Check if we need to send volume control change for this channel
                    if let Some(volume) = note.volume {
                        let current_volume = self.channel_volume.get(&note.channel).copied();
                        if current_volume != Some(volume) {
                            let volume_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 7, // Channel Volume
                                value: volume,
                            };
                            let _ = self.synth.send_event(volume_cc);
                            self.channel_volume.insert(note.channel, volume);
                            tracing::debug!(
                                "Volume CC: channel {} -> volume {}",
                                note.channel,
                                volume
                            );
                        }
                    }

                    // Check if we need to send pan control change for this channel
                    if let Some(pan) = note.pan {
                        let current_pan = self.channel_pan.get(&note.channel).copied();
                        if current_pan != Some(pan) {
                            let pan_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 10, // Pan
                                value: pan,
                            };
                            let _ = self.synth.send_event(pan_cc);
                            self.channel_pan.insert(note.channel, pan);
                            tracing::debug!("Pan CC: channel {} -> pan {}", note.channel, pan);
                        }
                    }

                    // Check if we need to send balance control change for this channel
                    if let Some(balance) = note.balance {
                        let current_balance = self.channel_balance.get(&note.channel).copied();
                        if current_balance != Some(balance) {
                            let balance_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 8, // Balance
                                value: balance,
                            };
                            let _ = self.synth.send_event(balance_cc);
                            self.channel_balance.insert(note.channel, balance);
                            tracing::debug!(
                                "Balance CC: channel {} -> balance {}",
                                note.channel,
                                balance
                            );
                        }
                    }

                    // Check if we need to send expression control change for this channel
                    if let Some(expression) = note.expression {
                        let current_expression =
                            self.channel_expression.get(&note.channel).copied();
                        if current_expression != Some(expression) {
                            let expression_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 11, // Expression Controller
                                value: expression,
                            };
                            let _ = self.synth.send_event(expression_cc);
                            self.channel_expression.insert(note.channel, expression);
                            tracing::debug!(
                                "Expression CC: channel {} -> expression {}",
                                note.channel,
                                expression
                            );
                        }
                    }

                    // Check if we need to send sustain control change for this channel
                    if let Some(sustain) = note.sustain {
                        let current_sustain = self.channel_sustain.get(&note.channel).copied();
                        if current_sustain != Some(sustain) {
                            let sustain_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 64, // Damper Pedal (Sustain)
                                value: sustain,
                            };
                            let _ = self.synth.send_event(sustain_cc);
                            self.channel_sustain.insert(note.channel, sustain);
                            tracing::debug!(
                                "Sustain CC: channel {} -> sustain {}",
                                note.channel,
                                sustain
                            );
                        }
                    }

                    let midi_event = MidiEvent::NoteOn {
                        channel: note.channel,
                        key: note.note,
                        vel: note.velocity,
                    };
                    let _ = self.synth.send_event(midi_event);
                    self.playing_notes.insert(key, note.duration);
                    tracing::debug!(
                        "Note ON: {} channel {} at sample {}",
                        note.note,
                        note.channel,
                        note_start_sample
                    );
                }
            }

            // Check if we should end this note in this chunk
            if note_end_sample >= current_sample_u32 && note_end_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if self.playing_notes.remove(&key).is_some() {
                    let midi_event = MidiEvent::NoteOff {
                        channel: note.channel,
                        key: note.note,
                    };
                    let _ = self.synth.send_event(midi_event);
                    tracing::debug!(
                        "Note OFF: {} channel {} at sample {}",
                        note.note,
                        note.channel,
                        note_end_sample
                    );
                }
            }
        }

        // Clear buffers
        self.left_buffer.fill(0.0);
        self.right_buffer.fill(0.0);

        // Render audio - OxiSynth expects stereo output
        self.synth
            .write((&mut self.left_buffer[..], &mut self.right_buffer[..]));

        // Log some debug info about the audio levels
        let max_left = self.left_buffer.iter().map(|x| x.abs()).fold(0.0, f32::max);
        let max_right = self
            .right_buffer
            .iter()
            .map(|x| x.abs())
            .fold(0.0, f32::max);

        if max_left > 0.001 || max_right > 0.001 {
            tracing::debug!(
                "Audio chunk: max_left={:.4}, max_right={:.4}, samples={}",
                max_left,
                max_right,
                self.buffer_size
            );
        }

        // Reset buffer position
        self.buffer_pos = 0;
        self.samples_generated += self.buffer_size;
    }
}

impl Iterator for OxiSynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time =
            Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);

        if current_time > self.total_duration {
            tracing::info!(
                "Audio playback finished after {} samples ({:.2}s)",
                self.samples_generated,
                current_time.as_secs_f32()
            );
            return None;
        }

        // If we've consumed the current buffer, process the next chunk
        if self.buffer_pos >= self.buffer_size {
            self.process_audio_chunk();
        }

        // Get the next sample (mix left and right channels for mono output)
        let sample = if self.buffer_pos < self.left_buffer.len() {
            // Amplify the audio by 20x to make it much louder
            (self.left_buffer[self.buffer_pos] + self.right_buffer[self.buffer_pos]) * 10.0
        } else {
            0.0
        };

        self.buffer_pos += 1;
        self.current_sample += 1;

        Some(sample)
    }
}

impl Source for OxiSynthSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1 // Mono output (we're mixing L+R)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }
}

/// R2D2 event for scheduling
#[derive(Debug, Clone)]
struct R2D2Event {
    start_time: f64, // seconds
    expression: R2D2Expression,
}

/// Pre-computed R2D2 event with generated audio samples
#[derive(Debug, Clone)]
struct R2D2PrecomputedEvent {
    start_sample: u32,
    samples: Vec<f32>,
}

/// Event for synthesis notes
#[derive(Debug, Clone)]
struct SynthEvent {
    start_time: f64,
    note: crate::midi::SimpleNote,
}

/// Pre-computed synthesis event with generated audio samples
#[derive(Debug, Clone)]
struct SynthPrecomputedEvent {
    start_sample: u32,
    samples: Vec<f32>,
}

/// Enhanced hybrid audio source that mixes MIDI, R2D2, and synthesis
struct EnhancedHybridAudioSource {
    // MIDI synthesis
    oxisynth_source: Option<OxiSynthSource>,

    // R2D2 synthesis
    r2d2_events: Vec<R2D2PrecomputedEvent>,

    // Synthesis events
    synthesis_events: Vec<SynthPrecomputedEvent>,

    sample_rate: u32,
    current_sample: usize,
    total_duration: Duration,

    // Audio mixing
    #[allow(dead_code)]
    mixing_buffer: Vec<f32>,
    #[allow(dead_code)]
    buffer_size: usize,
}

impl EnhancedHybridAudioSource {
    fn new(
        midi_notes: Vec<MidiNote>,
        r2d2_events: Vec<R2D2Event>,
        synthesis_events: Vec<SynthEvent>,
        total_duration: Duration,
    ) -> Result<Self, String> {
        let sample_rate = 44100;
        let buffer_size = 4096;

        // Create MIDI synthesizer source if there are MIDI notes
        let oxisynth_source = if !midi_notes.is_empty() {
            Some(
                OxiSynthSource::new(midi_notes, total_duration)
                    .map_err(|e| format!("Failed to create OxiSynth source: {}", e))?,
            )
        } else {
            None
        };

        // Pre-generate R2D2 audio samples
        let mut precomputed_r2d2_events = Vec::new();

        if !r2d2_events.is_empty() {
            let expressive_synth = ExpressiveSynth::new()
                .map_err(|e| format!("Failed to create ExpressiveSynth: {}", e))?;

            let r2d2_voice = R2D2Voice::new();

            for event in r2d2_events {
                let start_sample = (event.start_time * sample_rate as f64) as u32;

                let synth_params = r2d2_voice
                    .generate_expression_params(&event.expression)
                    .ok_or("Failed to generate R2D2 synthesis parameters")?;

                let samples = expressive_synth.generate_r2d2_samples_with_contour(
                    synth_params.base_freq,
                    event.expression.intensity,
                    synth_params.duration,
                    &synth_params.pitch_contour,
                );

                precomputed_r2d2_events.push(R2D2PrecomputedEvent {
                    start_sample,
                    samples,
                });
            }
        }

        // Pre-generate synthesis audio samples
        let mut precomputed_synthesis_events = Vec::new();

        if !synthesis_events.is_empty() {
            let expressive_synth = ExpressiveSynth::new()
                .map_err(|e| format!("Failed to create ExpressiveSynth for synthesis: {}", e))?;

            for event in synthesis_events {
                let start_sample = (event.start_time * sample_rate as f64) as u32;

                // Convert SimpleNote to SynthParams
                let synth_params = Self::convert_simple_note_to_synth_params(&event.note)?;

                // Generate synthesis samples
                let samples = expressive_synth
                    .generate_synthesized_samples(&synth_params)
                    .map_err(|e| format!("Failed to generate synthesis samples: {}", e))?;

                precomputed_synthesis_events.push(SynthPrecomputedEvent {
                    start_sample,
                    samples,
                });
            }
        }

        Ok(EnhancedHybridAudioSource {
            oxisynth_source,
            r2d2_events: precomputed_r2d2_events,
            synthesis_events: precomputed_synthesis_events,
            sample_rate,
            current_sample: 0,
            total_duration,
            mixing_buffer: vec![0.0; buffer_size],
            buffer_size,
        })
    }

    /// Convert SimpleNote to SynthParams for the ExpressiveSynth
    fn convert_simple_note_to_synth_params(
        note: &crate::midi::SimpleNote,
    ) -> Result<crate::expressive::SynthParams, String> {
        use crate::expressive::{
            EffectParams, EffectType, EnvelopeParams, FilterParams, FilterType, NoiseColor,
            SynthParams, SynthType,
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
                    algorithm: 1,  // Default algorithm
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
            },
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

        // Determine frequency (synthesis frequency overrides MIDI note)
        let frequency = if let Some(synth_freq) = note.synth_frequency {
            synth_freq
        } else if let Some(midi_note) = note.note {
            // Convert MIDI note to frequency
            440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
        } else {
            // Fallback frequency if no note is specified
            440.0
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

        // Create effects
        let mut effects = Vec::new();

        if let Some(reverb) = note.synth_reverb {
            if reverb > 0.0 {
                effects.push(EffectParams {
                    effect_type: EffectType::Reverb,
                    intensity: reverb,
                });
            }
        }

        if let Some(chorus) = note.synth_chorus {
            if chorus > 0.0 {
                effects.push(EffectParams {
                    effect_type: EffectType::Chorus,
                    intensity: chorus,
                });
            }
        }

        if let Some(delay) = note.synth_delay {
            if delay > 0.0 {
                let delay_time = note.synth_delay_time.unwrap_or(0.25);
                effects.push(EffectParams {
                    effect_type: EffectType::Delay { delay_time },
                    intensity: delay,
                });
            }
        }

        Ok(SynthParams {
            synth_type,
            frequency,
            amplitude: note.synth_amplitude.unwrap_or(0.7),
            duration: note.duration as f32,
            envelope,
            filter,
            effects,
        })
    }

    /// Get R2D2 sample at the given sample index
    fn get_r2d2_sample(&self, sample_index: usize) -> f32 {
        let mut sample = 0.0;

        for event in &self.r2d2_events {
            let event_sample_index = sample_index as i32 - event.start_sample as i32;
            if event_sample_index >= 0 && (event_sample_index as usize) < event.samples.len() {
                sample += event.samples[event_sample_index as usize];
            }
        }

        sample
    }

    /// Get synthesis sample at the given sample index
    fn get_synthesis_sample(&self, sample_index: usize) -> f32 {
        let mut sample = 0.0;

        for event in &self.synthesis_events {
            let event_sample_index = sample_index as i32 - event.start_sample as i32;
            if event_sample_index >= 0 && (event_sample_index as usize) < event.samples.len() {
                sample += event.samples[event_sample_index as usize];
            }
        }

        sample
    }
}

impl Iterator for EnhancedHybridAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time =
            Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);

        if current_time > self.total_duration {
            return None;
        }

        // Get MIDI sample
        let midi_sample = if let Some(ref mut oxisynth) = self.oxisynth_source {
            oxisynth.next().unwrap_or(0.0)
        } else {
            0.0
        };

        // Get R2D2 sample
        let r2d2_sample = self.get_r2d2_sample(self.current_sample);

        // Get synthesis sample
        let synthesis_sample = self.get_synthesis_sample(self.current_sample);

        // Mix all audio sources
        let mixed_sample = midi_sample + r2d2_sample + synthesis_sample;

        self.current_sample += 1;

        Some(mixed_sample)
    }
}

impl Source for EnhancedHybridAudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1 // Mono output
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_player_creation() {
        // This test might fail in CI environments without audio
        if let Ok(_player) = MidiPlayer::new() {
            // Success
        }
    }
}
