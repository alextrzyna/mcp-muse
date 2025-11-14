use anyhow::Result;
use rodio::Source;
use std::time::Duration;

use crate::expressive::{PolyphonicVoiceManager, R2D2Expression, R2D2Voice, SynthParams};
use crate::midi::SimpleNote;

/// Event for scheduling synthesis notes in real-time
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RealtimeSynthEvent {
    pub start_time: f64,
    pub note: SimpleNote,
    pub voice_id: Option<usize>, // Will be assigned when voice is allocated
}

/// Event for scheduling R2D2 expressions in real-time
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RealtimeR2D2Event {
    pub start_time: f64,
    pub expression: R2D2Expression,
    pub voice_id: Option<usize>, // Will be assigned when voice is allocated
}

/// Real-time polyphonic audio source with proper voice management
#[allow(dead_code)]
pub struct RealtimePolyphonicAudioSource {
    /// Voice manager for synthesis
    voice_manager: PolyphonicVoiceManager,

    /// MIDI synthesizer (OxiSynth) - keep existing polyphonic support
    oxisynth_source: Option<super::player::OxiSynthSource>,

    /// Scheduled synthesis events
    synthesis_events: Vec<RealtimeSynthEvent>,

    /// Scheduled R2D2 events
    r2d2_events: Vec<RealtimeR2D2Event>,

    /// R2D2 voice for generating parameters
    r2d2_voice: R2D2Voice,

    /// Current R2D2 samples being played
    current_r2d2_samples: Option<Vec<f32>>,

    /// Current position in R2D2 samples
    r2d2_sample_position: usize,

    /// Sample rate
    sample_rate: u32,

    /// Current sample index
    current_sample: usize,

    /// Total duration
    total_duration: Duration,

    /// Delta time per sample
    dt: f32,
}

#[allow(dead_code)]
impl RealtimePolyphonicAudioSource {
    /// Process events that should start at the current time
    fn process_scheduled_events(&mut self) {
        let current_time = self.current_sample as f64 / self.sample_rate as f64;

        // Process synthesis events
        for event in &mut self.synthesis_events {
            if event.voice_id.is_none() && current_time >= event.start_time {
                // Convert SimpleNote to SynthParams
                if let Ok(synth_params) = Self::convert_simple_note_to_synth_params(&event.note) {
                    // Determine priority based on note characteristics
                    let priority = if event.note.preset_name.is_some() {
                        100
                    } else {
                        50
                    };

                    // Extract note and channel info
                    let note = event.note.note;
                    let channel = event.note.channel;

                    // Allocate voice
                    match self.voice_manager.allocate_voice(
                        synth_params,
                        event.start_time,
                        note,
                        channel,
                        priority,
                    ) {
                        Ok(voice_id) => {
                            event.voice_id = Some(voice_id);
                            tracing::debug!(
                                "Allocated voice {} for synthesis event at {:.3}s",
                                voice_id,
                                event.start_time
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to allocate voice for synthesis event: {}", e);
                        }
                    }
                }
            }
        }

        // Process R2D2 events using the sophisticated existing synthesis
        for event in &mut self.r2d2_events {
            if event.voice_id.is_none() && current_time >= event.start_time {
                // Generate R2D2 expression using static synthesis method (avoids audio stream conflicts)
                if let Some(r2d2_params) = self
                    .r2d2_voice
                    .generate_expression_params(&event.expression)
                {
                    // Use the static R2D2 sample generation method for authentic sound without audio stream conflicts
                    let r2d2_samples =
                        crate::expressive::ExpressiveSynth::generate_r2d2_samples_static(
                            r2d2_params.base_freq,
                            event.expression.intensity,
                            r2d2_params.duration,
                            &r2d2_params.pitch_contour,
                        );

                    tracing::debug!(
                        "Generated {} R2D2 samples for event at {:.3}s",
                        r2d2_samples.len(),
                        event.start_time
                    );

                    // Store the pre-computed samples
                    self.current_r2d2_samples = Some(r2d2_samples);
                    self.r2d2_sample_position = 0;

                    // Mark event as processed
                    event.voice_id = Some(999); // Dummy voice ID to indicate processing
                }
            }
        }
    }

    /// Convert SimpleNote to SynthParams (moved from player.rs)
    fn convert_simple_note_to_synth_params(note: &SimpleNote) -> Result<SynthParams> {
        use crate::expressive::{
            EffectParams, EffectType, EnvelopeParams, FilterParams, FilterType, NoiseColor,
            SynthParams, SynthType,
        };

        let synth_type_str = note
            .synth_type
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Synthesis type is required"))?;

        // Parse synthesis type (comprehensive version)
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
            // Drum synthesis types
            "kick" => SynthType::Kick {
                punch: 0.8,
                sustain: 0.3,
                click_freq: 60.0,
                body_freq: 40.0,
            },
            "snare" => SynthType::Snare {
                snap: 0.7,
                buzz: 0.5,
                tone_freq: 200.0,
                noise_amount: 0.8,
            },
            "hihat" => SynthType::HiHat {
                metallic: 0.7,
                decay: 0.1,
                brightness: 0.8,
            },
            "cymbal" => SynthType::Cymbal {
                size: 0.5,
                metallic: 0.8,
                strike_intensity: 0.7,
            },
            // Sound effects synthesis types
            "swoosh" => SynthType::Swoosh {
                direction: 0.0,
                intensity: 0.7,
                frequency_sweep: (200.0, 2000.0),
            },
            "zap" => SynthType::Zap {
                energy: 0.8,
                decay: 0.2,
                harmonic_content: 0.6,
            },
            "chime" => SynthType::Chime {
                fundamental: 440.0,
                harmonic_count: 8,
                decay: 0.3,
                inharmonicity: 0.1,
            },
            "burst" => SynthType::Burst {
                center_freq: 1000.0,
                bandwidth: 500.0,
                intensity: 0.7,
                shape: 0.5,
            },
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown synthesis type: {}",
                    synth_type_str
                ));
            }
        };

        // Determine frequency (drums need specific frequencies, not musical pitches)
        let frequency = if let Some(synth_freq) = note.synth_frequency {
            synth_freq
        } else if let Some(midi_note) = note.note {
            // Convert MIDI note to frequency
            440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
        } else {
            // Use appropriate frequencies for drum types
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

        // Create effects
        let mut effects = Vec::new();

        if let Some(reverb) = note.synth_reverb
            && reverb > 0.0
        {
            effects.push(EffectParams {
                effect_type: EffectType::Reverb,
                intensity: reverb,
            });
        }

        if let Some(chorus) = note.synth_chorus
            && chorus > 0.0
        {
            effects.push(EffectParams {
                effect_type: EffectType::Chorus,
                intensity: chorus,
            });
        }

        if let Some(delay) = note.synth_delay
            && delay > 0.0
        {
            let delay_time = note.synth_delay_time.unwrap_or(0.25);
            effects.push(EffectParams {
                effect_type: EffectType::Delay { delay_time },
                intensity: delay,
            });
        }

        // Process universal effects from the new effects system
        if let Some(universal_effects) = &note.effects {
            for effect_config in universal_effects {
                if effect_config.enabled {
                    // Convert EffectConfig to EffectParams for audio processing
                    match &effect_config.effect {
                        crate::midi::EffectType::Reverb {
                            room_size: _,
                            dampening: _,
                            wet_level: _,
                            pre_delay: _,
                        } => {
                            effects.push(EffectParams {
                                effect_type: EffectType::Reverb,
                                intensity: effect_config.intensity,
                            });
                        }
                        crate::midi::EffectType::Delay {
                            delay_time,
                            feedback: _,
                            wet_level: _,
                            sync_tempo: _,
                        } => {
                            effects.push(EffectParams {
                                effect_type: EffectType::Delay {
                                    delay_time: *delay_time,
                                },
                                intensity: effect_config.intensity,
                            });
                        }
                        crate::midi::EffectType::Chorus {
                            rate: _,
                            depth: _,
                            feedback: _,
                            stereo_width: _,
                        } => {
                            effects.push(EffectParams {
                                effect_type: EffectType::Chorus,
                                intensity: effect_config.intensity,
                            });
                        }
                        // Note: Filter, Compressor, Distortion are not yet implemented in EffectParams
                        // They would need to be added to the EffectType enum in the expressive module
                        _ => {
                            // For now, skip unsupported effect types
                            // In the future, these would be implemented in the audio processing chain
                        }
                    }
                }
            }
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

impl Iterator for RealtimePolyphonicAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time =
            Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);

        if current_time > self.total_duration {
            return None;
        }

        // Process any events that should start now
        self.process_scheduled_events();

        // Get MIDI sample
        let midi_sample = if let Some(ref mut oxisynth) = self.oxisynth_source {
            oxisynth.next().unwrap_or(0.0)
        } else {
            0.0
        };

        // Process all voices and get synthesis sample
        let synthesis_sample = self.voice_manager.process_voices(self.dt);

        // Get R2D2 sample if available
        let r2d2_sample = if let Some(ref r2d2_samples) = self.current_r2d2_samples {
            if self.r2d2_sample_position < r2d2_samples.len() {
                let sample = r2d2_samples[self.r2d2_sample_position];
                self.r2d2_sample_position += 1;
                sample
            } else {
                // Finished playing R2D2 samples
                0.0
            }
        } else {
            0.0
        };

        // Clear finished R2D2 samples
        if let Some(ref r2d2_samples) = self.current_r2d2_samples.clone()
            && self.r2d2_sample_position >= r2d2_samples.len()
        {
            self.current_r2d2_samples = None;
            self.r2d2_sample_position = 0;
        }

        // Mix all audio sources
        let mixed_sample = midi_sample + synthesis_sample + r2d2_sample;

        self.current_sample += 1;

        Some(mixed_sample)
    }
}

impl Source for RealtimePolyphonicAudioSource {
    fn current_span_len(&self) -> Option<usize> {
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
