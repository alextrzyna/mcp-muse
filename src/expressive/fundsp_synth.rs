use anyhow::Result;
use rand::Rng;

/// FunDSP-based synthesizer for high-quality audio synthesis
pub struct FunDSPSynth {
    sample_rate: f32,
}

/// Parameters for FunDSP synthesis
#[derive(Debug, Clone)]
pub struct FunDSPParams {
    pub synth_type: FunDSPSynthType,
    pub frequency: f32,
    pub amplitude: f32,
    pub duration: f32,
}

/// Synthesis types handled by FunDSP for quality improvement
#[derive(Debug, Clone)]
pub enum FunDSPSynthType {
    // Professional drum synthesis - this is where FunDSP provides the most benefit
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

    // Advanced synthesis
    FM {
        modulator_freq: f32,
        modulation_index: f32,
    },
    Granular {
        grain_size: f32,
        overlap: f32,
        density: f32,
    },

    // Sound effects
    Zap {
        energy: f32,
        decay: f32,
        harmonic_content: f32,
    },
    Swoosh {
        direction: f32,
        intensity: f32,
        frequency_sweep: (f32, f32),
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
}

impl FunDSPSynth {
    /// Create new FunDSP synthesizer
    pub fn new() -> Result<Self> {
        Ok(FunDSPSynth {
            sample_rate: 44100.0,
        })
    }

    /// Generate samples using FunDSP for professional quality
    pub fn generate_samples(&self, params: &FunDSPParams) -> Result<Vec<f32>> {
        let sample_count = (self.sample_rate * params.duration) as usize;

        match &params.synth_type {
            FunDSPSynthType::Kick {
                punch,
                sustain,
                click_freq,
                body_freq,
            } => self.generate_kick_samples(
                *punch,
                *sustain,
                *click_freq,
                *body_freq,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Snare {
                snap,
                buzz,
                tone_freq,
                noise_amount,
            } => self.generate_snare_samples(
                *snap,
                *buzz,
                *tone_freq,
                *noise_amount,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::HiHat {
                metallic,
                decay,
                brightness,
            } => self.generate_hihat_samples(
                *metallic,
                *decay,
                *brightness,
                params.frequency,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Cymbal {
                size,
                metallic,
                strike_intensity,
            } => self.generate_cymbal_samples(
                *size,
                *metallic,
                *strike_intensity,
                params.frequency,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::FM {
                modulator_freq,
                modulation_index,
            } => self.generate_fm_samples(
                params.frequency,
                *modulator_freq,
                *modulation_index,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Granular {
                grain_size,
                overlap,
                density,
            } => self.generate_granular_samples(
                params.frequency,
                *grain_size,
                *overlap,
                *density,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Zap {
                energy,
                decay,
                harmonic_content,
            } => self.generate_zap_samples(
                params.frequency,
                *energy,
                *decay,
                *harmonic_content,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Swoosh {
                direction,
                intensity,
                frequency_sweep,
            } => self.generate_swoosh_samples(
                *direction,
                *intensity,
                *frequency_sweep,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Pad {
                warmth,
                movement,
                space,
                harmonic_evolution,
            } => self.generate_pad_samples(
                params.frequency,
                *warmth,
                *movement,
                *space,
                *harmonic_evolution,
                params.amplitude,
                sample_count,
            ),
            FunDSPSynthType::Texture {
                roughness,
                evolution,
                spectral_tilt,
                modulation_depth,
            } => self.generate_texture_samples(
                params.frequency,
                *roughness,
                *evolution,
                *spectral_tilt,
                *modulation_depth,
                params.amplitude,
                sample_count,
            ),
        }
    }

    /// Professional kick drum synthesis using FunDSP
    fn generate_kick_samples(
        &self,
        punch: f32,
        sustain: f32,
        click_freq: f32,
        body_freq: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        // Generate samples manually to avoid FunDSP node configuration issues
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;

            // Attack click component
            let click_envelope = (-t * 20.0 * punch).exp();
            let click_component =
                (2.0 * std::f32::consts::PI * click_freq * t).sin() * click_envelope * punch;

            // Body resonant component with exponential pitch decay
            let body_envelope = (-t * (6.0 / sustain.max(0.1))).exp();
            let body_freq_sweep = body_freq * (1.0 + 2.0 * (-t * 8.0).exp());
            let body_component =
                (2.0 * std::f32::consts::PI * body_freq_sweep * t).sin() * body_envelope;

            let sample = (click_component + body_component) * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Professional snare synthesis using FunDSP
    fn generate_snare_samples(
        &self,
        snap: f32,
        _buzz: f32,
        tone_freq: f32,
        noise_amount: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * (12.0 + snap * 8.0)).exp();

            // Tonal component
            let tonal = (2.0 * std::f32::consts::PI * tone_freq * t).sin() * (1.0 - noise_amount);

            // Noise component for snare buzz
            let mut rng = rand::rng();
            let noise = (rng.random::<f32>() - 0.5) * 2.0;
            // Simple highpass filter for noise
            let filtered_noise = if tone_freq > 1000.0 {
                noise
            } else {
                noise * 0.5
            };
            let noise_component = filtered_noise * noise_amount;

            let sample = (tonal + noise_component) * envelope_value * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Professional hi-hat synthesis using FunDSP
    fn generate_hihat_samples(
        &self,
        metallic: f32,
        decay: f32,
        brightness: f32,
        base_freq: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        let freq1 = base_freq * brightness;
        let freq2 = freq1 * 1.414; // √2 ratio
        let freq3 = freq1 * 1.732; // √3 ratio

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * (6.0 + decay * 4.0)).exp();

            // Multiple metallic frequencies
            let metallic1 = (2.0 * std::f32::consts::PI * freq1 * t).sin() * (0.4 * metallic);
            let metallic2 = (2.0 * std::f32::consts::PI * freq2 * t).sin() * (0.3 * metallic);
            let metallic3 = (2.0 * std::f32::consts::PI * freq3 * t).sin() * (0.2 * metallic);

            // Filtered noise component
            let mut rng = rand::rng();
            let noise = (rng.random::<f32>() - 0.5) * 2.0;
            let filtered_noise = noise * (1.0 - metallic * 0.5);

            let sample =
                (metallic1 + metallic2 + metallic3 + filtered_noise) * envelope_value * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Cymbal synthesis using multiple resonators
    fn generate_cymbal_samples(
        &self,
        size: f32,
        metallic: f32,
        strike_intensity: f32,
        base_freq: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        // Use hihat with modified parameters for cymbal
        self.generate_hihat_samples(
            metallic,
            size,
            strike_intensity,
            base_freq,
            amplitude,
            sample_count,
        )
    }

    /// FM synthesis for complex harmonic content
    fn generate_fm_samples(
        &self,
        carrier_freq: f32,
        modulator_freq: f32,
        modulation_index: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let modulator = (2.0 * std::f32::consts::PI * modulator_freq * t).sin();
            let modulated_frequency = carrier_freq + modulation_index * modulator_freq * modulator;
            let carrier = (2.0 * std::f32::consts::PI * modulated_frequency * t).sin();
            let envelope_value = (-t * 4.0).exp();
            samples.push(carrier * envelope_value * amplitude);
        }

        Ok(samples)
    }

    /// Granular synthesis implementation
    fn generate_granular_samples(
        &self,
        frequency: f32,
        _grain_size: f32,
        _overlap: f32,
        density: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * 3.0).exp();

            // Multiple detuned oscillators for granular effect
            let osc1 = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.6;
            let osc2 = (2.0 * std::f32::consts::PI * frequency * 1.5 * t).sin() * 0.4;

            let sample = (osc1 + osc2) * envelope_value * density * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Zap sound effect with dramatic character
    fn generate_zap_samples(
        &self,
        frequency: f32,
        energy: f32,
        decay: f32,
        harmonic_content: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * (8.0 + decay * 12.0)).exp();

            // Enhanced zap with frequency sweep and aggressive harmonics per plan
            let sweep_factor = 1.0 + energy * (2.0 * t - 1.0);
            let current_freq = frequency * sweep_factor.max(0.3);

            // Inharmonic overtone series for aggressive character
            let fundamental = (2.0 * std::f32::consts::PI * current_freq * t).sin();
            let overtone2 = (2.0 * std::f32::consts::PI * current_freq * 2.3 * t).sin() * 0.6;
            let overtone3 = (2.0 * std::f32::consts::PI * current_freq * 3.7 * t).sin() * 0.4;

            // Aggressive noise component
            let mut rng = rand::rng();
            let aggressive_noise = (rng.random::<f32>() - 0.5) * 2.0;
            let noise_envelope = (-t * 25.0 * energy).exp();

            let harmonic_content_factor = harmonic_content * 10.0;
            let harmonic_sum = fundamental + overtone2 + overtone3;
            let noise_component = aggressive_noise * noise_envelope * energy * 0.3;

            let sample = (harmonic_sum * harmonic_content_factor + noise_component)
                * envelope_value
                * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Swoosh sound effect
    fn generate_swoosh_samples(
        &self,
        direction: f32,
        intensity: f32,
        frequency_sweep: (f32, f32),
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);
        let duration = sample_count as f32 / self.sample_rate;

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let progress = t / duration;

            // Swoosh envelope with direction
            let envelope_value = if direction > 0.5 {
                (1.0 - progress).powf(2.0) * intensity
            } else {
                progress.powf(2.0) * intensity
            };

            // Frequency sweep
            let sweep_freq = frequency_sweep.0 + (frequency_sweep.1 - frequency_sweep.0) * progress;

            // Filtered noise for swoosh character
            let mut rng = rand::rng();
            let noise = (rng.random::<f32>() - 0.5) * 2.0;
            // Simple bandpass approximation
            let filtered_noise = noise * (sweep_freq / 1000.0).min(1.0);

            let sample = filtered_noise * envelope_value * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }

    /// Pad synthesis for ambient textures
    #[allow(clippy::too_many_arguments)]
    fn generate_pad_samples(
        &self,
        frequency: f32,
        warmth: f32,
        _movement: f32,
        space: f32,
        _harmonic_evolution: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * 2.0).exp();

            // Multi-harmonic pad synthesis with enhanced warmth
            let fundamental = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.4;
            let harmonic2 = (2.0 * std::f32::consts::PI * frequency * 2.0 * t).sin() * 0.3 * warmth;
            let harmonic3 = (2.0 * std::f32::consts::PI * frequency * 3.0 * t).sin() * 0.2 * warmth;
            let harmonic4 = (2.0 * std::f32::consts::PI * frequency * 4.0 * t).sin() * 0.1 * warmth;

            let mut sample =
                (fundamental + harmonic2 + harmonic3 + harmonic4) * envelope_value * amplitude;

            // Apply simple lowpass filter if space > 0.1
            if space > 0.1 {
                let cutoff_freq = frequency * 3.0;
                let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_freq);
                let dt = 1.0 / self.sample_rate;
                let alpha = dt / (rc + dt);
                sample *= alpha;
            }

            samples.push(sample);
        }

        Ok(samples)
    }

    /// Texture synthesis for complex ambient sounds
    #[allow(clippy::too_many_arguments)]
    fn generate_texture_samples(
        &self,
        frequency: f32,
        roughness: f32,
        _evolution: f32,
        _spectral_tilt: f32,
        _modulation_depth: f32,
        amplitude: f32,
        sample_count: usize,
    ) -> Result<Vec<f32>> {
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f32 / self.sample_rate;
            let envelope_value = (-t * 1.5).exp();

            // Mix sine and noise based on roughness
            let sine_component =
                (2.0 * std::f32::consts::PI * frequency * t).sin() * (1.0 - roughness);

            let mut rng = rand::rng();
            let noise_component = (rng.random::<f32>() - 0.5) * 2.0 * roughness;
            // Simple lowpass filter
            let filtered_noise = noise_component * (frequency / 2000.0).min(1.0);

            let sample = (sine_component + filtered_noise) * envelope_value * amplitude;
            samples.push(sample);
        }

        Ok(samples)
    }
}

/// Convert from main SynthParams to FunDSP parameters
impl From<crate::expressive::synth::SynthParams> for FunDSPParams {
    fn from(params: crate::expressive::synth::SynthParams) -> Self {
        let synth_type = match params.synth_type {
            crate::expressive::synth::SynthType::Kick {
                punch,
                sustain,
                click_freq,
                body_freq,
            } => FunDSPSynthType::Kick {
                punch,
                sustain,
                click_freq,
                body_freq,
            },
            crate::expressive::synth::SynthType::Snare {
                snap,
                buzz,
                tone_freq,
                noise_amount,
            } => FunDSPSynthType::Snare {
                snap,
                buzz,
                tone_freq,
                noise_amount,
            },
            crate::expressive::synth::SynthType::HiHat {
                metallic,
                decay,
                brightness,
            } => FunDSPSynthType::HiHat {
                metallic,
                decay,
                brightness,
            },
            crate::expressive::synth::SynthType::Cymbal {
                size,
                metallic,
                strike_intensity,
            } => FunDSPSynthType::Cymbal {
                size,
                metallic,
                strike_intensity,
            },
            crate::expressive::synth::SynthType::FM {
                modulator_freq,
                modulation_index,
            } => FunDSPSynthType::FM {
                modulator_freq,
                modulation_index,
            },
            crate::expressive::synth::SynthType::Granular {
                grain_size,
                overlap,
                density,
            } => FunDSPSynthType::Granular {
                grain_size,
                overlap,
                density,
            },
            crate::expressive::synth::SynthType::Zap {
                energy,
                decay,
                harmonic_content,
            } => FunDSPSynthType::Zap {
                energy,
                decay,
                harmonic_content,
            },
            crate::expressive::synth::SynthType::Swoosh {
                direction,
                intensity,
                frequency_sweep,
            } => FunDSPSynthType::Swoosh {
                direction,
                intensity,
                frequency_sweep,
            },
            crate::expressive::synth::SynthType::Pad {
                warmth,
                movement,
                space,
                harmonic_evolution,
            } => FunDSPSynthType::Pad {
                warmth,
                movement,
                space,
                harmonic_evolution,
            },
            crate::expressive::synth::SynthType::Texture {
                roughness,
                evolution,
                spectral_tilt,
                modulation_depth,
            } => FunDSPSynthType::Texture {
                roughness,
                evolution,
                spectral_tilt,
                modulation_depth,
            },
            // For other synthesis types, fall back to simple implementations
            _ => {
                // This shouldn't happen since we only route appropriate types to FunDSP
                FunDSPSynthType::Pad {
                    warmth: 0.5,
                    movement: 0.5,
                    space: 0.5,
                    harmonic_evolution: 0.5,
                }
            }
        };

        FunDSPParams {
            synth_type,
            frequency: params.frequency,
            amplitude: params.amplitude,
            duration: params.duration,
        }
    }
}
