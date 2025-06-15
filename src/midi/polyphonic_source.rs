use anyhow::Result;
use rodio::Source;
use std::time::Duration;

use crate::expressive::{PolyphonicVoiceManager, SynthParams, ExpressiveSynth, R2D2Voice, R2D2Expression};
use crate::midi::{SimpleNote, parser::MidiNote};

/// Event for scheduling synthesis notes in real-time
#[derive(Debug, Clone)]
pub struct RealtimeSynthEvent {
    pub start_time: f64,
    pub note: SimpleNote,
    pub voice_id: Option<usize>, // Will be assigned when voice is allocated
}

/// Event for scheduling R2D2 expressions in real-time
#[derive(Debug, Clone)]
pub struct RealtimeR2D2Event {
    pub start_time: f64,
    pub expression: R2D2Expression,
    pub voice_id: Option<usize>, // Will be assigned when voice is allocated
}

/// Real-time polyphonic audio source with proper voice management
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
    
    /// Sample rate
    sample_rate: u32,
    
    /// Current sample index
    current_sample: usize,
    
    /// Total duration
    total_duration: Duration,
    
    /// Delta time per sample
    dt: f32,
}

impl RealtimePolyphonicAudioSource {
    pub fn new(
        midi_notes: Vec<MidiNote>,
        synthesis_events: Vec<RealtimeSynthEvent>,
        r2d2_events: Vec<RealtimeR2D2Event>,
        total_duration: Duration,
    ) -> Result<Self> {
        let sample_rate = 44100;
        let dt = 1.0 / sample_rate as f32;
        
        // Create MIDI synthesizer source if there are MIDI notes
        let oxisynth_source = if !midi_notes.is_empty() {
            Some(
                super::player::OxiSynthSource::new(midi_notes, total_duration)
                    .map_err(|e| anyhow::anyhow!("Failed to create OxiSynth source: {}", e))?,
            )
        } else {
            None
        };
        
        // Create voice manager
        let voice_manager = PolyphonicVoiceManager::new(sample_rate as f32);
        
        // Create R2D2 voice
        let r2d2_voice = R2D2Voice::new();
        
        tracing::info!("Created RealtimePolyphonicAudioSource with {} synthesis events, {} R2D2 events",
                      synthesis_events.len(), r2d2_events.len());
        
        Ok(Self {
            voice_manager,
            oxisynth_source,
            synthesis_events,
            r2d2_events,
            r2d2_voice,
            sample_rate,
            current_sample: 0,
            total_duration,
            dt,
        })
    }
    
    /// Process events that should start at the current time
    fn process_scheduled_events(&mut self) {
        let current_time = self.current_sample as f64 / self.sample_rate as f64;
        
        // Process synthesis events
        for event in &mut self.synthesis_events {
            if event.voice_id.is_none() && current_time >= event.start_time {
                // Convert SimpleNote to SynthParams
                if let Ok(synth_params) = Self::convert_simple_note_to_synth_params(&event.note) {
                    // Determine priority based on note characteristics
                    let priority = if event.note.preset_name.is_some() { 100 } else { 50 };
                    
                    // Extract note and channel info
                    let note = event.note.note;
                    let channel = event.note.channel.unwrap_or(0);
                    
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
                            tracing::debug!("Allocated voice {} for synthesis event at {:.3}s", 
                                          voice_id, event.start_time);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to allocate voice for synthesis event: {}", e);
                        }
                    }
                }
            }
        }
        
        // Process R2D2 events
        for event in &mut self.r2d2_events {
            if event.voice_id.is_none() && current_time >= event.start_time {
                // Generate R2D2 synthesis parameters
                if let Some(r2d2_params) = self.r2d2_voice.generate_expression_params(&event.expression) {
                    // Convert R2D2 parameters to SynthParams
                    let synth_params = SynthParams {
                        synth_type: crate::expressive::SynthType::Sine, // R2D2 uses ring modulation, simplified to sine for now
                        frequency: r2d2_params.base_freq,
                        amplitude: 0.3,
                        duration: r2d2_params.duration,
                        envelope: crate::expressive::EnvelopeParams {
                            attack: 0.02,
                            decay: 0.1,
                            sustain: 0.7,
                            release: 0.3,
                        },
                        filter: None,
                        effects: Vec::new(),
                    };
                    
                    // High priority for R2D2 expressions
                    let priority = 200;
                    
                    // Allocate voice
                    match self.voice_manager.allocate_voice(
                        synth_params,
                        event.start_time,
                        None, // R2D2 doesn't have MIDI notes
                        0,    // Default channel
                        priority,
                    ) {
                        Ok(voice_id) => {
                            event.voice_id = Some(voice_id);
                            tracing::debug!("Allocated voice {} for R2D2 event at {:.3}s", 
                                          voice_id, event.start_time);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to allocate voice for R2D2 event: {}", e);
                        }
                    }
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

        // Parse synthesis type (simplified version)
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
            // Add more types as needed
            _ => return Err(anyhow::anyhow!("Unknown synthesis type: {}", synth_type_str)),
        };

        // Determine frequency
        let frequency = if let Some(synth_freq) = note.synth_frequency {
            synth_freq
        } else if let Some(midi_note) = note.note {
            // Convert MIDI note to frequency
            440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
        } else {
            // Fallback frequency
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
}

impl Iterator for RealtimePolyphonicAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time = Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);

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

        // Mix all audio sources
        let mixed_sample = midi_sample + synthesis_sample;

        self.current_sample += 1;

        Some(mixed_sample)
    }
}

impl Source for RealtimePolyphonicAudioSource {
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