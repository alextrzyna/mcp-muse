use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::expressive::{SynthParams, SynthType, EnvelopeParams, FilterParams, EffectParams};

/// Maximum number of simultaneous voices
pub const MAX_VOICES: usize = 32;

/// Voice state management
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceState {
    /// Voice is idle and available for allocation
    Idle,
    /// Voice is in attack phase
    Attack,
    /// Voice is in decay phase
    Decay,
    /// Voice is in sustain phase
    Sustain,
    /// Voice is in release phase
    Release,
}

/// Individual synthesis voice with real-time parameter control
#[derive(Debug, Clone)]
pub struct SynthVoice {
    /// Unique voice ID
    pub id: usize,
    /// Current voice state
    pub state: VoiceState,
    /// Synthesis parameters for this voice
    pub params: SynthParams,
    /// Current time position in the voice
    pub time: f32,
    /// Voice start time (for proper timing)
    pub start_time: f64,
    /// Voice duration (can be extended for sustain)
    pub duration: f32,
    /// Current envelope value
    pub envelope_value: f32,
    /// Priority for voice stealing (higher = more important)
    pub priority: u8,
    /// MIDI note number if applicable (for voice stealing logic)
    pub note: Option<u8>,
    /// Channel assignment
    pub channel: u8,
    /// Current oscillator phase (for oscillator continuity)
    pub oscillator_phase: f32,
    /// Filter state for stateful filters
    pub filter_state: FilterState,
    /// Effect state for stateful effects
    pub effect_state: EffectState,
}

/// Filter state for maintaining filter memory
#[derive(Debug, Clone)]
pub struct FilterState {
    pub lowpass_history: f32,
    pub highpass_history: f32,
    pub bandpass_history: f32,
}

/// Effect state for maintaining effect memory
#[derive(Debug, Clone)]
pub struct EffectState {
    pub reverb_buffer: Vec<f32>,
    pub chorus_buffer: Vec<f32>,
    pub delay_buffer: Vec<f32>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            lowpass_history: 0.0,
            highpass_history: 0.0,
            bandpass_history: 0.0,
        }
    }
}

impl Default for EffectState {
    fn default() -> Self {
        Self {
            reverb_buffer: Vec::new(),
            chorus_buffer: Vec::new(),
            delay_buffer: Vec::new(),
        }
    }
}

/// Real-time polyphonic voice manager
pub struct PolyphonicVoiceManager {
    /// Active voices
    voices: Vec<SynthVoice>,
    /// Sample rate for timing calculations
    sample_rate: f32,
    /// Next voice ID to assign
    next_voice_id: usize,
    /// Current global time
    current_time: f64,
    /// Voice allocation strategy
    allocation_strategy: VoiceAllocationStrategy,
}

#[derive(Debug, Clone)]
pub enum VoiceAllocationStrategy {
    /// Steal oldest voice when full
    OldestFirst,
    /// Steal voice with lowest priority
    LowestPriority,
    /// Steal voice with lowest volume
    LowestVolume,
}

impl PolyphonicVoiceManager {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            sample_rate,
            next_voice_id: 0,
            current_time: 0.0,
            allocation_strategy: VoiceAllocationStrategy::OldestFirst,
        }
    }

    /// Allocate a new voice for the given parameters
    pub fn allocate_voice(
        &mut self,
        params: SynthParams,
        start_time: f64,
        note: Option<u8>,
        channel: u8,
        priority: u8,
    ) -> Result<usize> {
        let voice_id = self.next_voice_id;
        self.next_voice_id += 1;

        // If we're at max voices, steal a voice
        if self.voices.len() >= MAX_VOICES {
            self.steal_voice(priority)?;
        }

        let voice = SynthVoice {
            id: voice_id,
            state: VoiceState::Attack,
            params: params.clone(),
            time: 0.0,
            start_time,
            duration: params.duration,
            envelope_value: 0.0,
            priority,
            note,
            channel,
            oscillator_phase: 0.0,
            filter_state: FilterState::default(),
            effect_state: EffectState::default(),
        };

        self.voices.push(voice);
        tracing::debug!("Allocated voice {} for note {:?} on channel {}", voice_id, note, channel);
        Ok(voice_id)
    }

    /// Release a voice (trigger release phase)
    pub fn release_voice(&mut self, voice_id: usize) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == voice_id) {
            if voice.state != VoiceState::Release && voice.state != VoiceState::Idle {
                voice.state = VoiceState::Release;
                tracing::debug!("Released voice {}", voice_id);
            }
        }
    }

    /// Release all voices for a specific note (for note-off events)
    pub fn release_note(&mut self, note: u8, channel: u8) {
        for voice in &mut self.voices {
            if voice.note == Some(note) && voice.channel == channel {
                if voice.state != VoiceState::Release && voice.state != VoiceState::Idle {
                    voice.state = VoiceState::Release;
                    tracing::debug!("Released voice {} for note {} on channel {}", voice.id, note, channel);
                }
            }
        }
    }

    /// Process all voices and generate audio samples
    pub fn process_voices(&mut self, dt: f32) -> f32 {
        self.current_time += dt as f64;
        let mut output = 0.0;

        // Process each active voice
        for voice in &mut self.voices {
            // Skip voices that haven't started yet
            if self.current_time < voice.start_time {
                continue;
            }

            // Update voice time
            voice.time += dt;

            // Calculate envelope
            voice.envelope_value = self.calculate_envelope(voice);

            // Update voice state based on envelope
            self.update_voice_state(voice);

            // Skip idle voices
            if voice.state == VoiceState::Idle {
                continue;
            }

            // Generate sample for this voice
            let sample = self.generate_voice_sample(voice, dt);
            output += sample;
        }

        // Clean up idle voices
        self.voices.retain(|v| v.state != VoiceState::Idle);

        output
    }

    /// Steal a voice when all voices are in use
    fn steal_voice(&mut self, new_priority: u8) -> Result<()> {
        let steal_index = match self.allocation_strategy {
            VoiceAllocationStrategy::OldestFirst => {
                // Find the oldest voice (lowest start_time)
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.start_time.partial_cmp(&b.start_time).unwrap())
                    .map(|(i, _)| i)
            }
            VoiceAllocationStrategy::LowestPriority => {
                // Find voice with lowest priority
                self.voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.priority < new_priority) // Only steal lower priority
                    .min_by_key(|(_, v)| v.priority)
                    .map(|(i, _)| i)
            }
            VoiceAllocationStrategy::LowestVolume => {
                // Find voice with lowest envelope value
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.envelope_value.partial_cmp(&b.envelope_value).unwrap())
                    .map(|(i, _)| i)
            }
        };

        if let Some(index) = steal_index {
            let stolen_voice = self.voices.remove(index);
            tracing::debug!("Stole voice {} (priority {})", stolen_voice.id, stolen_voice.priority);
        }

        Ok(())
    }

    /// Calculate envelope value for a voice
    fn calculate_envelope(&self, voice: &SynthVoice) -> f32 {
        let env = &voice.params.envelope;
        let t = voice.time;

        match voice.state {
            VoiceState::Idle => 0.0,
            VoiceState::Attack => {
                if env.attack > 0.0 {
                    (t / env.attack).min(1.0)
                } else {
                    1.0
                }
            }
            VoiceState::Decay => {
                let attack_time = env.attack;
                let decay_progress = (t - attack_time) / env.decay.max(0.001);
                1.0 - decay_progress.min(1.0) * (1.0 - env.sustain)
            }
            VoiceState::Sustain => env.sustain,
            VoiceState::Release => {
                // Find when release started
                let total_duration = voice.duration;
                let release_start_time = total_duration - env.release;
                let release_progress = (t - release_start_time) / env.release.max(0.001);
                env.sustain * (1.0 - release_progress.min(1.0))
            }
        }
    }

    /// Update voice state based on current time and envelope
    fn update_voice_state(&self, voice: &mut SynthVoice) {
        let env = &voice.params.envelope;
        let t = voice.time;

        match voice.state {
            VoiceState::Attack => {
                if t >= env.attack {
                    voice.state = VoiceState::Decay;
                }
            }
            VoiceState::Decay => {
                if t >= env.attack + env.decay {
                    voice.state = VoiceState::Sustain;
                }
            }
            VoiceState::Sustain => {
                // Stay in sustain until released or duration expires
                if t >= voice.duration - env.release {
                    voice.state = VoiceState::Release;
                }
            }
            VoiceState::Release => {
                if voice.envelope_value <= 0.001 || t >= voice.duration {
                    voice.state = VoiceState::Idle;
                }
            }
            VoiceState::Idle => {
                // Already idle, nothing to do
            }
        }
    }

    /// Generate a single sample for a voice
    fn generate_voice_sample(&mut self, voice: &mut SynthVoice, dt: f32) -> f32 {
        // Generate basic oscillator sample
        let mut sample = self.generate_oscillator_sample(voice, dt);

        // Apply filter if present
        if let Some(filter) = &voice.params.filter {
            sample = self.apply_voice_filter(sample, filter, &mut voice.filter_state, dt);
        }

        // Apply effects
        for effect in &voice.params.effects {
            sample = self.apply_voice_effect(sample, effect, &mut voice.effect_state, dt);
        }

        // Apply envelope and amplitude
        sample * voice.envelope_value * voice.params.amplitude
    }

    /// Generate oscillator sample for a voice
    fn generate_oscillator_sample(&mut self, voice: &mut SynthVoice, dt: f32) -> f32 {
        let freq = voice.params.frequency;
        let phase_increment = 2.0 * std::f32::consts::PI * freq * dt;
        voice.oscillator_phase += phase_increment;
        
        // Keep phase in range [0, 2π]
        while voice.oscillator_phase >= 2.0 * std::f32::consts::PI {
            voice.oscillator_phase -= 2.0 * std::f32::consts::PI;
        }

        // Generate sample based on synthesis type
        match &voice.params.synth_type {
            SynthType::Sine => voice.oscillator_phase.sin(),
            SynthType::Square { pulse_width } => {
                let normalized_phase = voice.oscillator_phase / (2.0 * std::f32::consts::PI);
                if normalized_phase < *pulse_width { 1.0 } else { -1.0 }
            }
            SynthType::Sawtooth => {
                let normalized_phase = voice.oscillator_phase / (2.0 * std::f32::consts::PI);
                2.0 * normalized_phase - 1.0
            }
            SynthType::Triangle => {
                let normalized_phase = voice.oscillator_phase / (2.0 * std::f32::consts::PI);
                if normalized_phase < 0.5 {
                    4.0 * normalized_phase - 1.0
                } else {
                    3.0 - 4.0 * normalized_phase
                }
            }
            // Add more synthesis types as needed
            _ => {
                // For complex synthesis types, we'll need to implement separate methods
                // For now, default to sine wave
                voice.oscillator_phase.sin()
            }
        }
    }

    /// Apply filter to voice sample (stateful)
    fn apply_voice_filter(&self, sample: f32, filter: &FilterParams, filter_state: &mut FilterState, _dt: f32) -> f32 {
        // Simple one-pole filter implementation with state
        let alpha = 1.0 - (-2.0 * std::f32::consts::PI * filter.cutoff / self.sample_rate).exp();
        
        match filter.filter_type {
            crate::expressive::FilterType::LowPass => {
                filter_state.lowpass_history = filter_state.lowpass_history + alpha * (sample - filter_state.lowpass_history);
                filter_state.lowpass_history
            }
            crate::expressive::FilterType::HighPass => {
                filter_state.highpass_history = filter_state.highpass_history + alpha * (sample - filter_state.highpass_history);
                sample - filter_state.highpass_history
            }
            crate::expressive::FilterType::BandPass => {
                // Implement bandpass as series lowpass + highpass
                let low_cutoff = filter.cutoff - filter.cutoff * 0.2;
                let high_cutoff = filter.cutoff + filter.cutoff * 0.2;
                
                let low_alpha = 1.0 - (-2.0 * std::f32::consts::PI * low_cutoff / self.sample_rate).exp();
                let high_alpha = 1.0 - (-2.0 * std::f32::consts::PI * high_cutoff / self.sample_rate).exp();
                
                filter_state.lowpass_history = filter_state.lowpass_history + low_alpha * (sample - filter_state.lowpass_history);
                filter_state.highpass_history = filter_state.highpass_history + high_alpha * (filter_state.lowpass_history - filter_state.highpass_history);
                
                filter_state.lowpass_history - filter_state.highpass_history
            }
        }
    }

    /// Apply effect to voice sample (stateful)
    fn apply_voice_effect(&self, sample: f32, effect: &EffectParams, _effect_state: &mut EffectState, _dt: f32) -> f32 {
        // Simplified effects for now - in full implementation, these would use the effect_state
        match &effect.effect_type {
            crate::expressive::EffectType::Reverb => {
                // Simple reverb approximation
                sample * (1.0 + effect.intensity * 0.3)
            }
            crate::expressive::EffectType::Chorus => {
                // Simple chorus approximation
                sample * (1.0 + effect.intensity * 0.2)
            }
            crate::expressive::EffectType::Delay { .. } => {
                // Simple delay approximation
                sample * (1.0 + effect.intensity * 0.4)
            }
        }
    }

    /// Get number of active voices
    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.state != VoiceState::Idle).count()
    }

    /// Get voice information for debugging
    pub fn get_voice_info(&self) -> Vec<(usize, VoiceState, Option<u8>, u8)> {
        self.voices
            .iter()
            .map(|v| (v.id, v.state.clone(), v.note, v.channel))
            .collect()
    }
} 