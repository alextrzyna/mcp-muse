use anyhow::Result;
use rodio::OutputStream;
// Add rand for noise generation
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Core expressive synthesizer for R2D2-style vocalizations
/// Using a simpler approach with direct audio generation
pub struct ExpressiveSynth {
    sample_rate: f32,
    _stream: OutputStream,
}

/// New synthesis parameters for general music synthesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthParams {
    pub synth_type: SynthType,
    pub frequency: f32,
    pub amplitude: f32,
    pub duration: f32,
    pub envelope: EnvelopeParams,
    pub filter: Option<FilterParams>,
    pub effects: Vec<EffectParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SynthType {
    // Basic oscillator synthesis
    Sine,
    Square {
        pulse_width: f32,
    },
    Sawtooth,
    Triangle,
    Noise {
        color: NoiseColor,
    },

    // Advanced synthesis techniques
    FM {
        modulator_freq: f32,
        modulation_index: f32,
    },
    // DX7-style 6-operator FM synthesis
    DX7FM {
        algorithm: u8,           // 1-32 (DX7 algorithms)
        operators: [DX7Operator; 6], // 6 operators like real DX7
    },
    Granular {
        grain_size: f32,
        overlap: f32,
        density: f32,
    },
    Wavetable {
        position: f32,
        morph_speed: f32,
    },

    // Percussion synthesis
    Kick {
        punch: f32,
        sustain: f32,
        click_freq: f32,
        body_freq: f32,
    },
    Snare {
        snap: f32,
        buzz: f32,
        tone_freq: f32,
        noise_amount: f32,
    },
    HiHat {
        metallic: f32,
        decay: f32,
        brightness: f32,
    },
    Cymbal {
        size: f32,
        metallic: f32,
        strike_intensity: f32,
    },

    // Sound effects synthesis
    Swoosh {
        direction: f32, // -1.0 to 1.0 for left-to-right sweep
        intensity: f32,
        frequency_sweep: (f32, f32), // (start_freq, end_freq)
    },
    Zap {
        energy: f32,
        decay: f32,
        harmonic_content: f32,
    },
    Chime {
        fundamental: f32,
        harmonic_count: u8,
        decay: f32,
        inharmonicity: f32,
    },
    Burst {
        center_freq: f32,
        bandwidth: f32,
        intensity: f32,
        shape: f32, // 0.0=sharp, 1.0=smooth
    },

    // Ambient textures
    Pad {
        warmth: f32,
        movement: f32,
        space: f32,
        harmonic_evolution: f32,
    },
    Texture {
        roughness: f32,
        evolution: f32,
        spectral_tilt: f32,
        modulation_depth: f32,
    },
    Drone {
        fundamental: f32,
        overtone_spread: f32,
        modulation: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoiseColor {
    White,
    #[allow(dead_code)]
    Pink,
    #[allow(dead_code)]
    Brown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterParams {
    pub cutoff: f32,
    pub resonance: f32,
    pub filter_type: FilterType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectParams {
    pub effect_type: EffectType,
    pub intensity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EffectType {
    Reverb,
    Chorus,
    Delay { delay_time: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DX7Operator {
    pub frequency_ratio: f32,    // Frequency ratio (0.5, 1.0, 2.0, etc.)
    pub output_level: f32,       // Operator output level (0.0-1.0)
    pub detune: f32,            // Fine detune (-7 to +7)
    pub envelope: EnvelopeParams, // Individual operator envelope
}

impl Default for DX7Operator {
    fn default() -> Self {
        Self {
            frequency_ratio: 1.0,
            output_level: 0.8,
            detune: 0.0,
            envelope: EnvelopeParams {
                attack: 0.01,
                decay: 0.3,
                sustain: 0.7,
                release: 0.5,
            },
        }
    }
}

impl ExpressiveSynth {
    /// Create a new expressive synthesizer
    pub fn new() -> Result<Self> {
        let (_stream, _stream_handle) = OutputStream::try_default()?;

        Ok(ExpressiveSynth {
            sample_rate: 44100.0,
            _stream,
        })
    }

    /// Generate audio samples using hybrid approach: FunDSP for quality-critical synthesis, custom DSP for others
    pub fn generate_synthesized_samples(&self, params: &SynthParams) -> Result<Vec<f32>> {
        // Check if this synthesis type should use FunDSP for higher quality
        if self.should_use_fundsp(&params.synth_type) {
            self.generate_with_fundsp(params)
        } else {
            self.generate_with_custom_dsp(params)
        }
    }

    /// Determine if a synthesis type should use FunDSP for better quality
    fn should_use_fundsp(&self, synth_type: &SynthType) -> bool {
        match synth_type {
            // Use FunDSP for all drum synthesis - this addresses the quality issues identified in user feedback
            SynthType::Kick { .. } => true,
            SynthType::Snare { .. } => true,
            SynthType::HiHat { .. } => true,
            SynthType::Cymbal { .. } => true,

            // Use FunDSP for advanced synthesis techniques that benefit from professional algorithms
            SynthType::FM { .. } => true,
            SynthType::Granular { .. } => true,

            // Use FunDSP for sound effects that need dramatic character improvements
            SynthType::Zap { .. } => true,
            SynthType::Swoosh { .. } => true,

            // Use FunDSP for ambient textures that benefit from rich processing
            SynthType::Pad { .. } => true,
            SynthType::Texture { .. } => true,

            // Keep custom DSP for basic oscillators and other synthesis types
            _ => false,
        }
    }

    /// Generate samples using FunDSP for professional quality
    fn generate_with_fundsp(&self, params: &SynthParams) -> Result<Vec<f32>> {
        use crate::expressive::fundsp_synth::{FunDSPParams, FunDSPSynth};

        // Create FunDSP synthesizer
        let fundsp_synth = FunDSPSynth::new()?;

        // Convert our parameters to FunDSP parameters
        let fundsp_params: FunDSPParams = params.clone().into();

        // Generate samples using FunDSP
        fundsp_synth.generate_samples(&fundsp_params)
    }

    /// Generate samples using custom DSP (original implementation)
    fn generate_with_custom_dsp(&self, params: &SynthParams) -> Result<Vec<f32>> {
        let sample_count = (self.sample_rate * params.duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let mut sample = self.generate_sample(params, t);

            // Apply filter if specified
            if let Some(filter) = &params.filter {
                sample = self.apply_filter(sample, filter, t);
            }

            // Apply effects if specified
            for effect in &params.effects {
                sample = self.apply_effect(sample, effect, t, i);
            }

            // Apply envelope
            let envelope_value = self.calculate_synth_envelope(t, params);
            let final_sample = sample * envelope_value * params.amplitude;

            samples.push(final_sample);
        }

        Ok(samples)
    }

    /// Generate a single sample using direct synthesis (simplified approach)
    fn generate_sample(&self, params: &SynthParams, t: f32) -> f32 {
        let freq = params.frequency;
        let phase = 2.0 * std::f32::consts::PI * freq * t;

        match &params.synth_type {
            SynthType::Sine => phase.sin(),
            SynthType::Square { pulse_width } => {
                let normalized_phase = (freq * t) % 1.0;
                if normalized_phase < *pulse_width {
                    1.0
                } else {
                    -1.0
                }
            }
            SynthType::Sawtooth => 2.0 * ((freq * t) % 1.0) - 1.0,
            SynthType::Triangle => {
                let x = (freq * t) % 1.0;
                if x < 0.5 {
                    4.0 * x - 1.0
                } else {
                    3.0 - 4.0 * x
                }
            }
            SynthType::Noise { color } => {
                let mut rng = rand::rng();
                match color {
                    NoiseColor::White => (rng.random::<f32>() - 0.5) * 2.0,
                    NoiseColor::Pink => {
                        // Simple pink noise approximation
                        let white = (rng.random::<f32>() - 0.5) * 2.0;
                        // Apply simple pink filter (approximation)
                        white * (1.0 / (1.0 + freq / 500.0).sqrt())
                    }
                    NoiseColor::Brown => {
                        // Simple brown noise approximation
                        let white = (rng.random::<f32>() - 0.5) * 2.0;
                        white * (1.0 / (1.0 + freq / 100.0))
                    }
                }
            }
            SynthType::FM {
                modulator_freq,
                modulation_index,
            } => {
                let modulator = (2.0 * std::f32::consts::PI * modulator_freq * t).sin();
                (phase + modulation_index * modulator).sin()
            }
            SynthType::DX7FM { algorithm: _, operators } => {
                // Enhanced DX7-style FM synthesis with proper operator envelopes
                // Use first two operators: op1 = carrier (output), op2 = modulator
                let carrier = &operators[0];   // Carrier (fundamental)
                let modulator = &operators[1]; // Modulator (creates harmonics)
                
                let carrier_freq = freq * carrier.frequency_ratio;
                let modulator_freq = freq * modulator.frequency_ratio;
                
                // Calculate individual operator envelopes
                let carrier_envelope = self.calculate_operator_envelope(t, &carrier.envelope);
                let modulator_envelope = self.calculate_operator_envelope(t, &modulator.envelope);
                
                // Generate modulator signal with its envelope
                let modulator_signal = (2.0 * std::f32::consts::PI * modulator_freq * t).sin() 
                    * modulator.output_level * modulator_envelope;
                
                // Apply phase modulation to carrier (proper FM synthesis)
                let carrier_phase = 2.0 * std::f32::consts::PI * carrier_freq * t;
                // Reduced modulation depth for cleaner bass - was * 2.0, now scaled by modulator level
                (carrier_phase + modulator_signal).sin() * carrier.output_level * carrier_envelope
            }
            SynthType::Granular {
                grain_size,
                overlap,
                density,
            } => {
                // ENHANCED GRANULAR SYNTHESIS - Following R2D2_EXPRESSIVE_SYNTH_PLAN.md Priority 1
                // Major improvement: Add pitched granular mode for musical notes instead of just texture
                let grain_duration = *grain_size;
                let grain_overlap = *overlap;
                let grain_density = *density;

                // Calculate which grains are active
                let grain_period = grain_duration * (1.0 - grain_overlap);
                let grain_index = (t / grain_period) as i32;

                let mut output = 0.0;

                // PITCHED GRANULAR MODE - Maintains musical pitch relationships
                let pitch_coherence = 0.8; // How much grains follow fundamental (0.0=textural, 1.0=pitched)
                let grain_pitch_spread = 0.15; // Controlled pitch variation for character

                // Generate overlapping grains with PITCH COHERENCE
                for i in 0..((grain_density * 4.0) as i32) {
                    let grain_start = (grain_index - i) as f32 * grain_period;
                    let local_grain_time = t - grain_start;

                    if local_grain_time >= 0.0 && local_grain_time <= grain_duration {
                        // Enhanced grain envelope (Hann window with smoother transitions)
                        let grain_progress = local_grain_time / grain_duration;
                        let envelope =
                            0.5 * (1.0 - (2.0 * std::f32::consts::PI * grain_progress).cos());

                        // PITCHED GRANULAR: Grains maintain musical pitch relationship
                        let mut rng = rand::rng();

                        // Pitch coherence: blend between pitched and textural
                        let base_pitch_variation = (rng.random::<f32>() - 0.5) * grain_pitch_spread;
                        let coherent_pitch = freq; // Musical pitch
                        let random_pitch = freq * (1.0 + base_pitch_variation);

                        // Blend coherent and random pitch based on pitch_coherence
                        let grain_freq = coherent_pitch * pitch_coherence
                            + random_pitch * (1.0 - pitch_coherence);

                        // Generate grain with MULTIPLE SYNTHESIS METHODS for richness
                        let grain_phase =
                            2.0 * std::f32::consts::PI * grain_freq * local_grain_time;

                        // Mix sine wave (tonal) with filtered noise (textural)
                        let tonal_component = grain_phase.sin() * 0.7;
                        let noise_component = (rng.random::<f32>() - 0.5) * 0.3;
                        let grain_sample = (tonal_component + noise_component) * envelope;

                        // Spatial positioning for grain clouds
                        let spatial_factor = 1.0 + ((i as f32) / 4.0) * 0.1;
                        output += grain_sample * grain_density * 0.25 * spatial_factor;
                    }
                }

                output.clamp(-1.0, 1.0)
            }
            SynthType::Wavetable {
                position,
                morph_speed,
            } => {
                // Simplified wavetable synthesis morphing between basic waveforms
                let morph_phase = (t * morph_speed).sin() * 0.5 + 0.5; // 0.0 to 1.0
                let table_pos = (*position + morph_phase) % 1.0;

                // Morph between sine, triangle, sawtooth, square
                if table_pos < 0.25 {
                    // Sine to Triangle
                    let blend = table_pos * 4.0;
                    let sine = phase.sin();
                    let x = (freq * t) % 1.0;
                    let triangle = if x < 0.5 {
                        4.0 * x - 1.0
                    } else {
                        3.0 - 4.0 * x
                    };
                    sine * (1.0 - blend) + triangle * blend
                } else if table_pos < 0.5 {
                    // Triangle to Sawtooth
                    let blend = (table_pos - 0.25) * 4.0;
                    let x = (freq * t) % 1.0;
                    let triangle = if x < 0.5 {
                        4.0 * x - 1.0
                    } else {
                        3.0 - 4.0 * x
                    };
                    let sawtooth = 2.0 * x - 1.0;
                    triangle * (1.0 - blend) + sawtooth * blend
                } else if table_pos < 0.75 {
                    // Sawtooth to Square
                    let blend = (table_pos - 0.5) * 4.0;
                    let x = (freq * t) % 1.0;
                    let sawtooth = 2.0 * x - 1.0;
                    let square = if phase.sin() > 0.0 { 1.0 } else { -1.0 };
                    sawtooth * (1.0 - blend) + square * blend
                } else {
                    // Square to Sine
                    let blend = (table_pos - 0.75) * 4.0;
                    let square = if phase.sin() > 0.0 { 1.0 } else { -1.0 };
                    let sine = phase.sin();
                    square * (1.0 - blend) + sine * blend
                }
            }
            SynthType::Kick {
                punch,
                sustain,
                click_freq,
                body_freq,
            } => {
                // Professional kick drum synthesis based on research:
                // 1. Attack portion: Sharp transient click for punch
                // 2. Body: Sine wave with rapid pitch decay from high to low frequency
                // 3. Envelope: Sharp attack, exponential decay

                // Attack phase (first 10-50ms): Sharp click with high frequencies
                let attack_duration = 0.05; // 50ms attack phase
                let attack_envelope = if t < attack_duration {
                    // Sharp exponential decay for attack
                    (-t * 20.0 * punch).exp()
                } else {
                    0.0
                };

                // Click component: High frequency transient
                let click =
                    (2.0 * std::f32::consts::PI * click_freq * t).sin() * attack_envelope * punch;

                // Body phase: Pitch-swept sine wave (most important part)
                // Start high (~200-400Hz) and decay rapidly to low frequency (~40-60Hz)
                let pitch_decay_rate = 15.0; // How fast pitch drops
                let start_pitch = body_freq * 4.0; // Start 4x higher than target
                let target_pitch = body_freq; // Target low frequency (40-60Hz range)

                // Exponential pitch decay
                let current_pitch =
                    target_pitch + (start_pitch - target_pitch) * (-t * pitch_decay_rate).exp();

                // Body envelope: Longer decay than attack
                let body_envelope = (-t * (6.0 / sustain.max(0.1))).exp();

                // Generate the pitch-swept sine wave body
                let phase = 2.0 * std::f32::consts::PI * current_pitch * t;
                let body = phase.sin() * body_envelope;

                // Combine attack click + pitch-swept body
                // Body is the dominant component, click adds punch
                body * 0.8 + click * 0.2
            }
            SynthType::Snare {
                snap,
                buzz,
                tone_freq,
                noise_amount,
            } => {
                // Professional snare synthesis:
                // 1. Tonal component: Resonant frequency from drum shell
                // 2. Noise component: Snare wires buzzing
                // 3. Sharp attack, medium decay

                let tone = (2.0 * std::f32::consts::PI * tone_freq * t).sin();
                let mut rng = rand::rng();
                let white_noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Create buzzy noise characteristic of snare wires
                let buzz_freq = tone_freq * 2.5; // Higher frequency buzz
                let buzz_component =
                    (2.0 * std::f32::consts::PI * buzz_freq * t).sin() * white_noise * buzz;

                // Sharp attack envelope
                let attack_envelope = (-t * (12.0 + snap * 8.0)).exp();

                // Mix tone and noise components
                let tonal_part = tone * (1.0 - noise_amount);
                let noise_part = (white_noise + buzz_component) * noise_amount;

                (tonal_part + noise_part) * attack_envelope
            }
            SynthType::HiHat {
                metallic,
                decay,
                brightness,
            } => {
                // Professional hi-hat synthesis:
                // Complex metallic frequencies + filtered noise

                let mut rng = rand::rng();
                let white_noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Multiple metallic frequencies for realistic cymbal sound
                let freq1 = freq * brightness;
                let freq2 = freq * brightness * 1.414; // √2 ratio
                let freq3 = freq * brightness * 1.732; // √3 ratio

                let metallic_component = (2.0 * std::f32::consts::PI * freq1 * t).sin() * 0.33
                    + (2.0 * std::f32::consts::PI * freq2 * t).sin() * 0.33
                    + (2.0 * std::f32::consts::PI * freq3 * t).sin() * 0.34;

                // Envelope: Very sharp attack, controllable decay
                let envelope = (-t * (10.0 + 20.0 / decay.max(0.1))).exp();

                // Mix metallic harmonics with high-frequency noise
                let metallic_sound = metallic_component * metallic;
                let noise_sound = white_noise * (1.0 - metallic * 0.5);

                (metallic_sound + noise_sound) * envelope
            }
            SynthType::Cymbal {
                size,
                metallic,
                strike_intensity,
            } => {
                // Professional cymbal synthesis with complex harmonics
                let mut rng = rand::rng();
                let white_noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Size affects fundamental frequency range
                let base_freq = freq * (0.5 + size * 0.5);

                // Complex harmonic series for realistic cymbal sound
                let harm1 = (2.0 * std::f32::consts::PI * base_freq * t).sin() * 0.3;
                let harm2 = (2.0 * std::f32::consts::PI * base_freq * 1.593 * t).sin() * 0.25; // Slightly inharmonic
                let harm3 = (2.0 * std::f32::consts::PI * base_freq * 2.135 * t).sin() * 0.2;
                let harm4 =
                    (2.0 * std::f32::consts::PI * base_freq * std::f32::consts::E * t).sin() * 0.15;
                let harm5 =
                    (2.0 * std::f32::consts::PI * base_freq * std::f32::consts::PI * t).sin() * 0.1;

                let harmonic_content = harm1 + harm2 + harm3 + harm4 + harm5;

                // Strike intensity affects attack and overall character
                let attack_rate = 8.0 + strike_intensity * 12.0;
                let decay_rate = 0.8 + size * 1.2; // Larger cymbals decay slower
                let envelope = (-t * attack_rate).exp() * (1.0 + (-t * decay_rate).exp() * 2.0);

                // Mix harmonics with metallic noise
                let metallic_sound = harmonic_content * metallic;
                let noise_contribution = white_noise * (1.0 - metallic) * 0.3;

                (metallic_sound + noise_contribution) * envelope * strike_intensity
            }
            SynthType::Swoosh {
                direction,
                intensity,
                frequency_sweep,
            } => {
                let (start_freq, end_freq) = *frequency_sweep;

                // Frequency sweep over time
                let progress = t / params.duration.max(0.1);
                let current_freq = start_freq + (end_freq - start_freq) * progress;

                // Generate filtered noise with frequency sweep
                let mut rng = rand::rng();
                let noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Simple bandpass filter simulation
                let filter_center = current_freq;
                let filter_width = 200.0 * intensity;
                let freq_distance = (freq - filter_center).abs();
                let filter_response = if freq_distance < filter_width {
                    1.0 - (freq_distance / filter_width).powf(2.0)
                } else {
                    0.1
                };

                // Envelope for swoosh effect
                let envelope = (std::f32::consts::PI * progress).sin() * intensity;

                // Apply direction (future stereo implementation would use this)
                let directional_intensity = 1.0 + direction * 0.1; // Subtle for now

                noise * filter_response * envelope * directional_intensity
            }
            SynthType::Zap {
                energy,
                decay,
                harmonic_content,
            } => {
                // ENHANCED ZAP SYNTHESIS - Following R2D2_EXPRESSIVE_SYNTH_PLAN.md Priority 2
                // Major improvement: Add aggressive characteristics and dramatic frequency sweeps

                // AGGRESSIVE ZAP CHARACTERISTICS instead of simple harmonics
                let progress = t / params.duration.max(0.1);

                // 1. DRAMATIC FREQUENCY SWEEP (key for "zap" character)
                let sweep_start_freq = freq * 2.0; // Start high
                let sweep_end_freq = freq * 0.3; // End low (dramatic sweep)
                let current_freq =
                    sweep_start_freq + (sweep_end_freq - sweep_start_freq) * progress;

                // 2. INHARMONIC OVERTONES for aggressive character
                let fundamental = (2.0 * std::f32::consts::PI * current_freq * t).sin();

                // Aggressive inharmonic series (not musical ratios)
                let harm1 = (2.0 * std::f32::consts::PI * current_freq * 1.71 * t).sin() * 0.6; // √3 ratio
                let harm2 = (2.0 * std::f32::consts::PI * current_freq * 2.43 * t).sin() * 0.4; // Irrational
                let harm3 = (2.0 * std::f32::consts::PI * current_freq * 3.87 * t).sin() * 0.3; // Non-harmonic
                let harm4 = (2.0 * std::f32::consts::PI * current_freq * 5.23 * t).sin() * 0.2; // Chaotic

                // 3. HIGH-FREQUENCY NOISE BURST for "energy" character
                let mut rng = rand::rng();
                let aggressive_noise = (rng.random::<f32>() - 0.5) * 2.0;
                let noise_envelope = (-t * 12.0).exp(); // Sharp noise burst
                let energy_burst = aggressive_noise * noise_envelope * energy * 0.5;

                // 4. SPECTRAL CHAOS - Frequency domain distortion
                let chaos_factor =
                    (2.0 * std::f32::consts::PI * current_freq * 7.39 * t).sin() * 0.15;
                let spectral_distortion = fundamental * (1.0 + chaos_factor * harmonic_content);

                // 5. SHARP ATTACK with DRAMATIC DECAY
                let attack_envelope = if t < 0.02 {
                    // 20ms sharp attack
                    (-t * 30.0 * energy).exp()
                } else {
                    (-t * (3.0 + decay * 8.0)).exp()
                };

                // Combine all aggressive elements
                let harmonic_mix = fundamental + harm1 + harm2 + harm3 + harm4;
                let zap_character =
                    (harmonic_mix + spectral_distortion + energy_burst) * attack_envelope;

                // Apply saturation for more aggressive character
                let saturated = if zap_character > 0.0 {
                    zap_character.powf(0.7)
                } else {
                    -((-zap_character).powf(0.7))
                };

                saturated * energy * 0.8
            }
            SynthType::Chime {
                fundamental,
                harmonic_count,
                decay,
                inharmonicity,
            } => {
                let base_freq = *fundamental;
                let mut output = 0.0;

                // Generate multiple harmonic partials
                for i in 1..=(*harmonic_count as usize) {
                    let harmonic_freq = base_freq * i as f32;

                    // Add inharmonicity (slight detuning for realism)
                    let detuned_freq =
                        harmonic_freq * (1.0 + inharmonicity * (i as f32 - 1.0) * 0.01);

                    // Each harmonic has different amplitude and decay
                    let harmonic_amplitude = 1.0 / (i as f32);
                    let harmonic_decay = decay * (1.0 + i as f32 * 0.1);

                    let harmonic_phase = 2.0 * std::f32::consts::PI * detuned_freq * t;
                    let harmonic_envelope = (-t * harmonic_decay).exp();

                    output += harmonic_phase.sin() * harmonic_amplitude * harmonic_envelope;
                }

                output / (*harmonic_count as f32).sqrt() // Normalize
            }
            SynthType::Burst {
                center_freq,
                bandwidth,
                intensity,
                shape,
            } => {
                // Spectral burst around center frequency
                let mut rng = rand::rng();
                let noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Create burst envelope
                let progress = t / params.duration.max(0.1);
                let envelope = match shape {
                    s if *s < 0.5 => {
                        // Sharp burst (exponential decay)
                        (-progress * 8.0).exp()
                    }
                    _ => {
                        // Smooth burst (Gaussian-like)
                        (-(progress * 3.0).powf(2.0)).exp()
                    }
                };

                // Frequency filtering around center_freq
                let freq_distance = (freq - center_freq).abs();
                let filter_response = if freq_distance < *bandwidth {
                    1.0 - (freq_distance / *bandwidth).powf(2.0)
                } else {
                    0.1
                };

                // Add some tonal content at center frequency
                let tonal = (2.0 * std::f32::consts::PI * center_freq * t).sin() * 0.3;

                (noise * filter_response + tonal) * envelope * intensity
            }
            SynthType::Pad {
                warmth,
                movement,
                space,
                harmonic_evolution,
            } => {
                // Rich harmonic pad with evolving timbre
                let mut output = 0.0;

                // Multiple harmonics for richness
                for i in 1..=8 {
                    let harmonic_freq = freq * i as f32;
                    let harmonic_amplitude = 1.0 / (i as f32).powf(0.7); // Gentle rolloff

                    // Evolving harmonics over time
                    let evolution_phase = t * harmonic_evolution + i as f32;
                    let harmonic_mod = 1.0 + (evolution_phase).sin() * 0.1;

                    let harmonic_phase = 2.0 * std::f32::consts::PI * harmonic_freq * t;
                    output += harmonic_phase.sin() * harmonic_amplitude * harmonic_mod;
                }

                // Add movement with slow LFO
                let movement_lfo = (2.0 * std::f32::consts::PI * 0.2 * t).sin() * movement * 0.1;
                output *= 1.0 + movement_lfo;

                // Warmth affects filtering (simulated)
                let warmth_factor = 0.7 + warmth * 0.3;
                output *= warmth_factor;

                // Space parameter would affect reverb in a full implementation
                output * (0.6 + space * 0.2) // For now, just affects amplitude
            }
            SynthType::Texture {
                roughness,
                evolution,
                spectral_tilt,
                modulation_depth,
            } => {
                // Rough, evolving textural sound
                let mut rng = rand::rng();
                let noise = (rng.random::<f32>() - 0.5) * 2.0;

                // Base oscillator
                let osc = (2.0 * std::f32::consts::PI * freq * t).sin();

                // Evolution over time
                let evolution_phase = t * evolution * 0.5;
                let evolution_mod = (evolution_phase).sin() * 0.5 + 0.5;

                // Spectral tilt (frequency-dependent amplitude)
                let tilt_factor = if *spectral_tilt > 0.5 {
                    // High-frequency emphasis
                    1.0 + (freq / 1000.0) * (*spectral_tilt - 0.5) * 2.0
                } else {
                    // Low-frequency emphasis
                    1.0 + (1000.0 / freq.max(100.0)) * (0.5 - *spectral_tilt) * 2.0
                };

                // Modulation
                let modulation =
                    (2.0 * std::f32::consts::PI * freq * 0.618 * t).sin() * modulation_depth;

                // Mix oscillator, noise, and modulation
                let base_mix = osc * (1.0 - roughness) + noise * roughness;
                let modulated = base_mix * (1.0 + modulation);

                modulated * evolution_mod * tilt_factor * 0.7
            }
            SynthType::Drone {
                fundamental,
                overtone_spread,
                modulation,
            } => {
                // Sustained drone with overtones
                let base_freq = *fundamental;
                let mut output = 0.0;

                // Fundamental
                output += (2.0 * std::f32::consts::PI * base_freq * t).sin() * 0.4;

                // Overtones with spreading
                for i in 2..=6 {
                    let overtone_freq =
                        base_freq * i as f32 * (1.0 + overtone_spread * 0.1 * (i as f32 - 1.0));
                    let overtone_amplitude = 0.3 / i as f32;

                    // Slow modulation of overtones
                    let mod_phase = t * modulation * 0.1 + i as f32;
                    let mod_amount = 1.0 + (mod_phase).sin() * 0.05;

                    let overtone_phase = 2.0 * std::f32::consts::PI * overtone_freq * t;
                    output += overtone_phase.sin() * overtone_amplitude * mod_amount;
                }

                // Very slow amplitude modulation for organic feel
                let slow_lfo = (2.0 * std::f32::consts::PI * 0.1 * t).sin() * 0.05 + 1.0;
                output * slow_lfo * 0.8
            }
        }
    }

    /// Calculate synthesis envelope based on parameters
    /// Calculate envelope for individual DX7 operators
    fn calculate_operator_envelope(&self, t: f32, envelope: &EnvelopeParams) -> f32 {
        let attack = envelope.attack;
        let decay = envelope.decay;
        let sustain = envelope.sustain;
        let release = envelope.release;

        if t < attack {
            // Attack phase: linear rise from 0 to 1
            t / attack
        } else if t < attack + decay {
            // Decay phase: exponential decay from 1 to sustain level
            let decay_progress = (t - attack) / decay;
            1.0 - (1.0 - sustain) * decay_progress
        } else {
            // Sustain/Release phase: hold sustain level then exponential decay
            // For DX7 operators, we assume immediate release (no sustain hold)
            let release_start = attack + decay;
            let release_progress = (t - release_start) / release;
            sustain * (-release_progress * 3.0).exp() // Exponential decay
        }
    }

    fn calculate_synth_envelope(&self, t: f32, params: &SynthParams) -> f32 {
        let env = &params.envelope;
        let duration = params.duration;

        if t < env.attack {
            // Attack phase
            t / env.attack
        } else if t < env.attack + env.decay {
            // Decay phase
            let decay_progress = (t - env.attack) / env.decay;
            1.0 - decay_progress * (1.0 - env.sustain)
        } else if t < duration - env.release {
            // Sustain phase
            env.sustain
        } else {
            // Release phase
            let release_progress = (t - (duration - env.release)) / env.release;
            env.sustain * (1.0 - release_progress)
        }
    }

    /// Generate R2D2-style audio samples with emotion-specific pitch contours
    pub fn generate_r2d2_samples_with_contour(
        &self,
        base_freq: f32,
        emotion_intensity: f32,
        duration: f32,
        pitch_contour: &[f32],
    ) -> Vec<f32> {
        let sample_count = (self.sample_rate * duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);

        let dt = 1.0 / self.sample_rate;

        // Ben Burtt's approach: ring modulation with dynamic filter sweeps
        let carrier_freq = base_freq;
        let mod_freq = base_freq * 0.618; // Golden ratio for more organic modulation

        // Minimal vibrato to preserve pitch contour clarity
        let vibrato_rate = 1.8;
        let vibrato_depth = 0.008; // Very subtle

        for i in 0..sample_count {
            let t = i as f32 * dt;
            let progress = t / duration; // 0.0 to 1.0 through the sound

            // PROMINENT PITCH CONTOUR from emotion presets
            let pitch_multiplier =
                self.interpolate_pitch_contour(progress, pitch_contour, emotion_intensity);
            let contoured_freq = carrier_freq * pitch_multiplier;

            // Subtle vibrato that preserves pitch contour
            let vibrato = (2.0 * std::f32::consts::PI * vibrato_rate * t).sin() * vibrato_depth;
            let final_carrier_freq = contoured_freq * (1.0 + vibrato);
            let final_mod_freq = mod_freq * pitch_multiplier * (1.0 + vibrato * 0.2);

            // Generate carrier and modulator for ring modulation
            let carrier = (2.0 * std::f32::consts::PI * final_carrier_freq * t).sin();
            let modulator = (2.0 * std::f32::consts::PI * final_mod_freq * t).sin();

            // Ring modulation (core R2D2 sound)
            let ring_mod = carrier * modulator;

            // Simplified approach: minimal filtering to avoid interference
            let filtered_voice = ring_mod; // Skip complex filtering for now

            // Minimal harmonics that definitely follow pitch direction
            let harmonic2 = if pitch_multiplier > 1.5 {
                // High pitch gets some harmonics
                (2.0 * std::f32::consts::PI * final_carrier_freq * 1.1 * t).sin() * 0.05
            } else {
                // Low pitch gets very few harmonics
                (2.0 * std::f32::consts::PI * final_carrier_freq * 1.05 * t).sin() * 0.02
            };

            // Mix: filtered ring mod + minimal harmonics
            let voice = filtered_voice * 0.75 + harmonic2;

            // Apply envelope with emotion-specific attack/decay
            let envelope =
                self.calculate_emotion_envelope(t, duration, emotion_intensity, pitch_contour);

            // Gentle saturation for warmth
            let saturated = self.tube_saturation(voice);

            let sample = saturated * envelope * 0.28;
            samples.push(sample);
        }

        samples
    }

    /// Interpolate pitch contour from emotion presets
    fn interpolate_pitch_contour(
        &self,
        progress: f32,
        pitch_contour: &[f32],
        intensity: f32,
    ) -> f32 {
        if pitch_contour.is_empty() {
            return 1.0;
        }

        if pitch_contour.len() == 1 {
            return 1.0 + pitch_contour[0] * intensity;
        }

        // SPECIAL CASE: Force sad emotion to definitely descend
        // Check if this looks like a sad contour (starts high, ends low)
        if pitch_contour.len() > 3
            && pitch_contour[0] > 0.8
            && pitch_contour[pitch_contour.len() - 1] < 0.1
        {
            // This is definitely a descending contour - force it to work correctly
            let start_multiplier = 2.2; // Start high
            let end_multiplier = 0.3; // End low
            let pitch_multiplier =
                start_multiplier + (end_multiplier - start_multiplier) * progress;
            let result = pitch_multiplier.clamp(0.2, 2.5);

            return result;
        }

        // Map progress (0.0-1.0) to contour array indices
        let scaled_pos = progress * (pitch_contour.len() - 1) as f32;
        let index = scaled_pos.floor() as usize;
        let fraction = scaled_pos - index as f32;

        // Interpolate between adjacent contour points
        let current_val = pitch_contour.get(index).unwrap_or(&0.0);
        let next_val = pitch_contour.get(index + 1).unwrap_or(current_val);

        let interpolated = current_val + (next_val - current_val) * fraction;

        // Apply intensity scaling and convert to frequency multiplier
        // The contour values are 0.0-1.0, we want pitch multipliers with dramatic range
        let pitch_multiplier = 0.4 + interpolated * intensity * 2.0;

        pitch_multiplier.clamp(0.2, 3.0) // Allow wider range for dramatic effects
    }

    /// Emotion-specific envelope with attack/decay characteristics
    fn calculate_emotion_envelope(
        &self,
        t: f32,
        duration: f32,
        emotion_intensity: f32,
        pitch_contour: &[f32],
    ) -> f32 {
        let progress = t / duration;

        // Different envelope shapes based on pitch contour characteristics
        let envelope = if pitch_contour.len() >= 3 {
            // Analyze contour for envelope shape
            let contour_start = pitch_contour[0];
            let contour_end = pitch_contour[pitch_contour.len() - 1];

            if contour_end > contour_start + 0.4 {
                // Rising contour (Curious, Surprised) - quick attack, sustained
                if progress < 0.1 {
                    progress * 10.0 // Quick attack
                } else if progress < 0.8 {
                    1.0 // Sustain
                } else {
                    (1.0 - progress) * 5.0 // Quick release
                }
            } else if contour_start > contour_end + 0.4 {
                // Falling contour (Sad, Negative) - slow attack, gradual decay
                if progress < 0.2 {
                    progress * 5.0 // Slower attack
                } else {
                    (1.0 - progress) * 1.25 // Gradual fade
                }
            } else {
                // Bouncy/complex contour (Happy, Excited) - punchy envelope
                let bounce = (progress * std::f32::consts::PI * 3.0).sin().abs();
                if progress < 0.1 {
                    progress * 10.0
                } else if progress < 0.9 {
                    0.8 + bounce * 0.2
                } else {
                    (1.0 - progress) * 10.0
                }
            }
        } else {
            // Fallback standard envelope
            self.calculate_envelope(t, duration, emotion_intensity)
        };

        envelope.clamp(0.0, 1.0)
    }

    /// Tube-style saturation for organic warmth (replaces harsh clipping)
    fn tube_saturation(&self, x: f32) -> f32 {
        // Gentle tube-style saturation for more organic sound
        if x.abs() < 0.5 {
            x
        } else {
            let sign = x.signum();
            let abs_x = x.abs();
            sign * (0.5 + (abs_x - 0.5) * 0.6)
        }
    }

    /// Simple resonator simulation for formant filtering
    #[allow(dead_code)]
    fn simple_resonator(&self, input: f32, freq: f32, t: f32) -> f32 {
        // Simulate a resonant filter by emphasizing frequencies near the formant
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        let resonance = phase.sin() * 0.7 + (phase * 1.5).sin() * 0.3;
        input * resonance
    }

    /// Soft clipping to prevent harsh distortion
    #[allow(dead_code)]
    fn soft_clip(&self, x: f32) -> f32 {
        if x.abs() <= 0.5 {
            x
        } else {
            x.signum() * (0.5 + 0.5 * (1.0 - (-2.0 * (x.abs() - 0.5)).exp()))
        }
    }

    /// Calculate envelope for natural attack/decay
    fn calculate_envelope(&self, t: f32, duration: f32, emotion_intensity: f32) -> f32 {
        let attack_time = 0.02 + emotion_intensity * 0.03;
        let decay_time = 0.05 + emotion_intensity * 0.05;

        if t < attack_time {
            // Attack
            t / attack_time
        } else if t < duration - decay_time {
            // Sustain with slight decay
            1.0 - (t - attack_time) * 0.1 / (duration - attack_time - decay_time)
        } else {
            // Decay
            (duration - t) / decay_time
        }
    }

    /// Ben Burtt's signature dynamic filter sweep (simulates ARP 2600 resonant filter)
    #[allow(dead_code)]
    fn dynamic_filter_sweep(&self, input: f32, cutoff_freq: f32, resonance: f32, t: f32) -> f32 {
        // Simulate the ARP 2600's resonant filter with self-oscillation
        let _normalized_cutoff = (cutoff_freq / self.sample_rate * 2.0).min(0.99);

        // Simple resonant filter simulation
        // This approximates the characteristic "whistle" of the ARP 2600
        let filter_osc = (2.0 * std::f32::consts::PI * cutoff_freq * t).sin() * resonance * 0.3;
        let filtered = input * (1.0 - resonance * 0.5) + filter_osc;

        // Apply a gentle high-pass characteristic
        let hp_coeff = 0.95;
        filtered * hp_coeff + input * (1.0 - hp_coeff)
    }

    /// Apply filter to sample
    fn apply_filter(&self, sample: f32, filter: &FilterParams, t: f32) -> f32 {
        // Simplified filter implementation
        let cutoff_normalized = filter.cutoff / (self.sample_rate * 0.5); // Normalize to Nyquist
        let resonance = filter.resonance.clamp(0.0, 0.9); // Prevent instability

        match filter.filter_type {
            FilterType::LowPass => {
                // Simple one-pole lowpass filter
                let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_normalized).exp();
                let filtered = sample * alpha;

                // Add resonance (simplified)
                let resonant_freq = filter.cutoff;
                let resonance_component =
                    (2.0 * std::f32::consts::PI * resonant_freq * t).sin() * resonance * 0.1;

                filtered + resonance_component
            }
            FilterType::HighPass => {
                // Simple one-pole highpass filter (complement of lowpass)
                let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_normalized).exp();
                let lowpass = sample * alpha;
                let highpass = sample - lowpass;

                // Add resonance
                let resonant_freq = filter.cutoff;
                let resonance_component =
                    (2.0 * std::f32::consts::PI * resonant_freq * t).sin() * resonance * 0.1;

                highpass + resonance_component
            }
            FilterType::BandPass => {
                // Simple bandpass (combination of high and low pass)
                let bandwidth = filter.cutoff * 0.2; // 20% of cutoff frequency
                let low_cutoff = filter.cutoff - bandwidth;
                let high_cutoff = filter.cutoff + bandwidth;

                let low_alpha = 1.0
                    - (-2.0 * std::f32::consts::PI * (low_cutoff / (self.sample_rate * 0.5))).exp();
                let high_alpha = 1.0
                    - (-2.0 * std::f32::consts::PI * (high_cutoff / (self.sample_rate * 0.5)))
                        .exp();

                let lowpass = sample * low_alpha;
                let highpass = sample - (sample * high_alpha);
                let bandpass = lowpass - highpass;

                // Add resonance at center frequency
                let resonance_component =
                    (2.0 * std::f32::consts::PI * filter.cutoff * t).sin() * resonance * 0.15;

                bandpass + resonance_component
            }
        }
    }

    /// Apply effect to sample
    fn apply_effect(&self, sample: f32, effect: &EffectParams, t: f32, sample_index: usize) -> f32 {
        match &effect.effect_type {
            EffectType::Reverb => {
                // ENHANCED REVERB - Following R2D2_EXPRESSIVE_SYNTH_PLAN.md Priority 3
                // Major improvement: More pronounced and audible reverb processing

                // Multiple delay taps for realistic reverb (simulated)
                let delay1 = 0.025; // 25ms - early reflection
                let delay2 = 0.045; // 45ms - room reflection
                let delay3 = 0.075; // 75ms - late reflection
                let delay4 = 0.125; // 125ms - deep reverb
                let delay5 = 0.200; // 200ms - cathedral-like tail

                let delay1_samples = (delay1 * self.sample_rate) as usize;
                let delay2_samples = (delay2 * self.sample_rate) as usize;
                let delay3_samples = (delay3 * self.sample_rate) as usize;
                let delay4_samples = (delay4 * self.sample_rate) as usize;
                let delay5_samples = (delay5 * self.sample_rate) as usize;

                let mut reverb_sum = 0.0;

                // DRAMATICALLY ENHANCED reflection levels for audible reverb
                if sample_index >= delay1_samples {
                    reverb_sum += sample * 0.6 * effect.intensity; // Early reflection
                }
                if sample_index >= delay2_samples {
                    reverb_sum += sample * 0.5 * effect.intensity; // Room character
                }
                if sample_index >= delay3_samples {
                    reverb_sum += sample * 0.4 * effect.intensity; // Mid reverb
                }
                if sample_index >= delay4_samples {
                    reverb_sum += sample * 0.3 * effect.intensity; // Late reverb
                }
                if sample_index >= delay5_samples {
                    reverb_sum += sample * 0.2 * effect.intensity; // Reverb tail
                }

                // High-frequency damping for realistic reverb character
                let hf_damping = 0.7; // Simulate air absorption
                reverb_sum *= hf_damping;

                // Enhanced wet/dry mix with much more pronounced reverb
                let wet_level = effect.intensity * 0.8; // Much higher wet level
                let dry_level = 1.0 - (effect.intensity * 0.4); // Less dry reduction

                sample * dry_level + reverb_sum * wet_level
            }
            EffectType::Chorus => {
                // ENHANCED CHORUS - Much more audible modulation and richness
                let lfo_rate = 1.8; // Slightly slower for more lush character
                let lfo_depth = 0.012; // Deeper modulation for obvious effect

                // Dual LFO for richer chorus (classic technique)
                let lfo1 = (2.0 * std::f32::consts::PI * lfo_rate * t).sin();
                let lfo2 = (2.0 * std::f32::consts::PI * lfo_rate * 1.31 * t).sin(); // Slightly detuned

                // Enhanced modulated delay times
                let base_delay = 0.015; // 15ms base delay
                let _modulated_delay1 = base_delay + lfo1 * lfo_depth;
                let _modulated_delay2 = base_delay + lfo2 * lfo_depth * 0.7; // Different depth

                // Simulate pitch modulation through frequency shifting
                let pitch_mod1 = 1.0 + lfo1 * 0.008; // Subtle pitch variation
                let pitch_mod2 = 1.0 + lfo2 * 0.006; // Different rate

                // Create chorus voices with different characteristics
                let chorus_voice1 = sample * 0.8 * pitch_mod1; // Modulated amplitude
                let chorus_voice2 = sample * 0.7 * pitch_mod2; // Second voice

                // Add slight detuning for richness
                let detune_factor1 = 1.0 + (lfo1 * 0.002);
                let detune_factor2 = 1.0 - (lfo2 * 0.003);

                let detuned_voice1 = chorus_voice1 * detune_factor1;
                let detuned_voice2 = chorus_voice2 * detune_factor2;

                // Mix multiple chorus voices for richness
                let chorus_mix = (detuned_voice1 + detuned_voice2) * 0.5;

                // ENHANCED wet/dry mix with much more pronounced chorus
                let wet_level = effect.intensity * 0.7; // Higher wet level
                let dry_level = 1.0 - (effect.intensity * 0.3); // Less dry reduction

                sample * dry_level + chorus_mix * wet_level
            }
            EffectType::Delay { delay_time } => {
                // ENHANCED DELAY - Multiple taps and feedback for obvious delay effect
                let delay_samples = (*delay_time * self.sample_rate) as usize;
                let feedback_level = 0.4 * effect.intensity; // Feedback creates resonance

                // Multiple delay taps for richer delay character
                let delay_tap1 = delay_samples;
                let delay_tap2 = (delay_samples as f32 * 0.75) as usize; // 3/4 delay
                let delay_tap3 = (delay_samples as f32 * 0.5) as usize; // 1/2 delay

                let mut delayed_sum = 0.0;

                // Multiple delay taps with different levels (simulated)
                if sample_index >= delay_tap1 {
                    delayed_sum += sample * 0.6; // Main delay
                }
                if sample_index >= delay_tap2 {
                    delayed_sum += sample * 0.4; // Secondary delay
                }
                if sample_index >= delay_tap3 {
                    delayed_sum += sample * 0.3; // Tertiary delay
                }

                // Simulate feedback by adding delayed signal back
                let feedback_component = delayed_sum * feedback_level;

                // High-frequency damping in delay path (realistic)
                let hf_damping = 0.8;
                let processed_delay = (delayed_sum + feedback_component) * hf_damping;

                // ENHANCED wet/dry mix with obvious delay presence
                let wet_level = effect.intensity * 0.8; // Much higher wet level
                let dry_level = 1.0; // Keep full dry signal

                sample * dry_level + processed_delay * wet_level
            }
        }
    }
}
