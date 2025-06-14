use anyhow::Result;
use rodio::{OutputStream, Sink, Source};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
// Add rand for noise generation
use rand::Rng;

/// Core expressive synthesizer for R2D2-style vocalizations
/// Using a simpler approach with direct audio generation
pub struct ExpressiveSynth {
    sample_rate: f32,
    _stream: OutputStream,
    sink: Arc<Mutex<Sink>>,
}

/// New synthesis parameters for general music synthesis
#[derive(Debug, Clone)]
pub struct SynthParams {
    pub synth_type: SynthType,
    pub frequency: f32,
    pub amplitude: f32,
    pub duration: f32,
    pub envelope: EnvelopeParams,
    pub filter: Option<FilterParams>,
    pub effects: Vec<EffectParams>,
}

#[derive(Debug, Clone)]
pub enum SynthType {
    // Basic oscillator synthesis
    Sine,
    Square { pulse_width: f32 },
    Sawtooth,
    Triangle,
    Noise { color: NoiseColor },
    
    // Advanced synthesis techniques
    FM { modulator_freq: f32, modulation_index: f32 },
    Granular { 
        grain_size: f32, 
        overlap: f32, 
        density: f32 
    },
    Wavetable { 
        position: f32,
        morph_speed: f32 
    },
    
    // Percussion synthesis
    Kick { 
        punch: f32, 
        sustain: f32, 
        click_freq: f32,
        body_freq: f32 
    },
    Snare { 
        snap: f32, 
        buzz: f32, 
        tone_freq: f32,
        noise_amount: f32 
    },
    HiHat { 
        metallic: f32, 
        decay: f32,
        brightness: f32 
    },
    Cymbal {
        size: f32,
        metallic: f32,
        strike_intensity: f32
    },
    
    // Sound effects synthesis
    Swoosh { 
        direction: f32, // -1.0 to 1.0 for left-to-right sweep
        intensity: f32,
        frequency_sweep: (f32, f32)  // (start_freq, end_freq)
    },
    Zap { 
        energy: f32, 
        decay: f32,
        harmonic_content: f32 
    },
    Chime { 
        fundamental: f32,
        harmonic_count: u8, 
        decay: f32,
        inharmonicity: f32 
    },
    Burst {
        center_freq: f32,
        bandwidth: f32,
        intensity: f32,
        shape: f32  // 0.0=sharp, 1.0=smooth
    },
    
    // Ambient textures
    Pad { 
        warmth: f32, 
        movement: f32, 
        space: f32,
        harmonic_evolution: f32 
    },
    Texture { 
        roughness: f32, 
        evolution: f32,
        spectral_tilt: f32,
        modulation_depth: f32 
    },
    Drone {
        fundamental: f32,
        overtone_spread: f32,
        modulation: f32
    },
}

#[derive(Debug, Clone)]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
}

#[derive(Debug, Clone)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Debug, Clone)]
pub struct FilterParams {
    pub cutoff: f32,
    pub resonance: f32,
    pub filter_type: FilterType,
}

#[derive(Debug, Clone)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone)]
pub struct EffectParams {
    pub effect_type: EffectType,
    pub intensity: f32,
}

#[derive(Debug, Clone)]
pub enum EffectType {
    Reverb,
    Chorus,
    Delay { delay_time: f32 },
}

impl ExpressiveSynth {
    /// Create a new expressive synthesizer
    pub fn new() -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        Ok(ExpressiveSynth {
            sample_rate: 44100.0,
            _stream,
            sink: Arc::new(Mutex::new(sink)),
        })
    }

    /// Play a synthesized note using simplified synthesis
    pub fn play_synthesized_note(&self, params: SynthParams) -> Result<()> {
        let samples = self.generate_synthesized_samples(&params)?;
        let source = SynthAudioSource::new(samples, self.sample_rate);
        
        let sink = self.sink.lock().unwrap();
        sink.append(source);
        
        // Wait for playback to complete
        thread::sleep(Duration::from_secs_f32(params.duration + 0.1));
        
        Ok(())
    }

    /// Play a sequence of synthesized sounds with precise timing
    pub fn play_synthesized_sequence(&self, sounds: Vec<SynthParams>) -> Result<()> {
        if sounds.is_empty() {
            return Ok(());
        }

        // Calculate total sequence duration
        let total_duration = sounds.iter()
            .map(|s| s.duration)
            .fold(0.0, f32::max);
        
        // Pre-generate all audio samples
        let mut mixed_samples = vec![0.0; (self.sample_rate * total_duration) as usize];
        
        for sound in sounds {
            let samples = self.generate_synthesized_samples(&sound)?;
            
            // Mix into the output buffer
            for (i, sample) in samples.iter().enumerate() {
                if i < mixed_samples.len() {
                    mixed_samples[i] += sample;
                }
            }
        }
        
        // Play the mixed result
        let source = SynthAudioSource::new(mixed_samples, self.sample_rate);
        let sink = self.sink.lock().unwrap();
        sink.append(source);
        
        // Wait for playback to complete
        thread::sleep(Duration::from_secs_f32(total_duration + 0.1));
        
        Ok(())
    }

    /// Generate audio samples using simplified synthesis (not FunDSP for now)
    fn generate_synthesized_samples(&self, params: &SynthParams) -> Result<Vec<f32>> {
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
            SynthType::Sine => {
                phase.sin()
            },
            SynthType::Square { pulse_width } => {
                let normalized_phase = (freq * t) % 1.0;
                if normalized_phase < *pulse_width { 1.0 } else { -1.0 }
            },
            SynthType::Sawtooth => {
                2.0 * ((freq * t) % 1.0) - 1.0
            },
            SynthType::Triangle => {
                let x = (freq * t) % 1.0;
                if x < 0.5 { 4.0 * x - 1.0 } else { 3.0 - 4.0 * x }
            },
            SynthType::Noise { color } => {
                let mut rng = rand::thread_rng();
                match color {
                    NoiseColor::White => (rng.gen::<f32>() - 0.5) * 2.0,
                    NoiseColor::Pink => {
                        // Simple pink noise approximation
                        let white = (rng.gen::<f32>() - 0.5) * 2.0;
                        // Apply simple pink filter (approximation)
                        white * (1.0 / (1.0 + freq / 500.0).sqrt())
                    },
                    NoiseColor::Brown => {
                        // Simple brown noise approximation
                        let white = (rng.gen::<f32>() - 0.5) * 2.0;
                        white * (1.0 / (1.0 + freq / 100.0))
                    }
                }
            },
            SynthType::FM { modulator_freq, modulation_index } => {
                let modulator = (2.0 * std::f32::consts::PI * modulator_freq * t).sin();
                (phase + modulation_index * modulator).sin()
            },
            SynthType::Granular { grain_size, overlap, density } => {
                // Simplified granular synthesis with grain clouds
                let grain_duration = *grain_size;
                let grain_overlap = *overlap;
                let grain_density = *density;
                
                // Calculate which grains are active
                let grain_period = grain_duration * (1.0 - grain_overlap);
                let grain_index = (t / grain_period) as i32;
                let grain_time = t % grain_period;
                
                let mut output = 0.0;
                
                // Generate overlapping grains
                for i in 0..((grain_density * 4.0) as i32) {
                    let grain_start = (grain_index - i) as f32 * grain_period;
                    let local_grain_time = t - grain_start;
                    
                    if local_grain_time >= 0.0 && local_grain_time <= grain_duration {
                        // Grain envelope (Hann window)
                        let grain_progress = local_grain_time / grain_duration;
                        let envelope = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * grain_progress).cos());
                        
                        // Grain content (sine wave with slight pitch variation)
                        let mut rng = rand::thread_rng();
                        let pitch_variation = 1.0 + (rng.gen::<f32>() - 0.5) * 0.1;
                        let grain_freq = freq * pitch_variation;
                        let grain_phase = 2.0 * std::f32::consts::PI * grain_freq * local_grain_time;
                        let grain_sample = grain_phase.sin() * envelope;
                        
                        output += grain_sample * grain_density * 0.25;
                    }
                }
                
                output.clamp(-1.0, 1.0)
            },
            SynthType::Wavetable { position, morph_speed } => {
                // Simplified wavetable synthesis morphing between basic waveforms
                let morph_phase = (t * morph_speed).sin() * 0.5 + 0.5; // 0.0 to 1.0
                let table_pos = (*position + morph_phase) % 1.0;
                
                // Morph between sine, triangle, sawtooth, square
                if table_pos < 0.25 {
                    // Sine to Triangle
                    let blend = table_pos * 4.0;
                    let sine = phase.sin();
                    let x = (freq * t) % 1.0;
                    let triangle = if x < 0.5 { 4.0 * x - 1.0 } else { 3.0 - 4.0 * x };
                    sine * (1.0 - blend) + triangle * blend
                } else if table_pos < 0.5 {
                    // Triangle to Sawtooth
                    let blend = (table_pos - 0.25) * 4.0;
                    let x = (freq * t) % 1.0;
                    let triangle = if x < 0.5 { 4.0 * x - 1.0 } else { 3.0 - 4.0 * x };
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
            },
            SynthType::Kick { punch, sustain, click_freq, body_freq } => {
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
                let click = (2.0 * std::f32::consts::PI * click_freq * t).sin() * attack_envelope * punch;
                
                // Body phase: Pitch-swept sine wave (most important part)
                // Start high (~200-400Hz) and decay rapidly to low frequency (~40-60Hz)
                let pitch_decay_rate = 15.0; // How fast pitch drops
                let start_pitch = body_freq * 4.0; // Start 4x higher than target
                let target_pitch = body_freq; // Target low frequency (40-60Hz range)
                
                // Exponential pitch decay
                let current_pitch = target_pitch + (start_pitch - target_pitch) * (-t * pitch_decay_rate).exp();
                
                // Body envelope: Longer decay than attack
                let body_envelope = (-t * (6.0 / sustain.max(0.1))).exp();
                
                // Generate the pitch-swept sine wave body
                let phase = 2.0 * std::f32::consts::PI * current_pitch * t;
                let body = phase.sin() * body_envelope;
                
                // Combine attack click + pitch-swept body
                // Body is the dominant component, click adds punch
                body * 0.8 + click * 0.2
            },
            SynthType::Snare { snap, buzz, tone_freq, noise_amount } => {
                // Professional snare synthesis:
                // 1. Tonal component: Resonant frequency from drum shell
                // 2. Noise component: Snare wires buzzing
                // 3. Sharp attack, medium decay
                
                let tone = (2.0 * std::f32::consts::PI * tone_freq * t).sin();
                let mut rng = rand::thread_rng();
                let white_noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
                // Create buzzy noise characteristic of snare wires
                let buzz_freq = tone_freq * 2.5; // Higher frequency buzz
                let buzz_component = (2.0 * std::f32::consts::PI * buzz_freq * t).sin() * white_noise * buzz;
                
                // Sharp attack envelope
                let attack_envelope = (-t * (12.0 + snap * 8.0)).exp();
                
                // Mix tone and noise components
                let tonal_part = tone * (1.0 - noise_amount);
                let noise_part = (white_noise + buzz_component) * noise_amount;
                
                (tonal_part + noise_part) * attack_envelope
            },
            SynthType::HiHat { metallic, decay, brightness } => {
                // Professional hi-hat synthesis:
                // Complex metallic frequencies + filtered noise
                
                let mut rng = rand::thread_rng();
                let white_noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
                // Multiple metallic frequencies for realistic cymbal sound
                let freq1 = freq * brightness;
                let freq2 = freq * brightness * 1.414; // √2 ratio
                let freq3 = freq * brightness * 1.732; // √3 ratio
                
                let metallic_component = 
                    (2.0 * std::f32::consts::PI * freq1 * t).sin() * 0.33 +
                    (2.0 * std::f32::consts::PI * freq2 * t).sin() * 0.33 +
                    (2.0 * std::f32::consts::PI * freq3 * t).sin() * 0.34;
                
                // Envelope: Very sharp attack, controllable decay
                let envelope = (-t * (10.0 + 20.0 / decay.max(0.1))).exp();
                
                // Mix metallic harmonics with high-frequency noise
                let metallic_sound = metallic_component * metallic;
                let noise_sound = white_noise * (1.0 - metallic * 0.5);
                
                (metallic_sound + noise_sound) * envelope
            },
            SynthType::Cymbal { size, metallic, strike_intensity } => {
                // Professional cymbal synthesis with complex harmonics
                let mut rng = rand::thread_rng();
                let white_noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
                // Size affects fundamental frequency range
                let base_freq = freq * (0.5 + size * 0.5);
                
                // Complex harmonic series for realistic cymbal sound
                let harm1 = (2.0 * std::f32::consts::PI * base_freq * t).sin() * 0.3;
                let harm2 = (2.0 * std::f32::consts::PI * base_freq * 1.593 * t).sin() * 0.25; // Slightly inharmonic
                let harm3 = (2.0 * std::f32::consts::PI * base_freq * 2.135 * t).sin() * 0.2;
                let harm4 = (2.0 * std::f32::consts::PI * base_freq * 2.718 * t).sin() * 0.15;
                let harm5 = (2.0 * std::f32::consts::PI * base_freq * 3.141 * t).sin() * 0.1;
                
                let harmonic_content = harm1 + harm2 + harm3 + harm4 + harm5;
                
                // Strike intensity affects attack and overall character
                let attack_rate = 8.0 + strike_intensity * 12.0;
                let decay_rate = 0.8 + size * 1.2; // Larger cymbals decay slower
                let envelope = (-t * attack_rate).exp() * (1.0 + (-t * decay_rate).exp() * 2.0);
                
                // Mix harmonics with metallic noise
                let metallic_sound = harmonic_content * metallic;
                let noise_contribution = white_noise * (1.0 - metallic) * 0.3;
                
                (metallic_sound + noise_contribution) * envelope * strike_intensity
            },
            SynthType::Swoosh { direction, intensity, frequency_sweep } => {
                let (start_freq, end_freq) = *frequency_sweep;
                
                // Frequency sweep over time
                let progress = t / params.duration.max(0.1);
                let current_freq = start_freq + (end_freq - start_freq) * progress;
                
                // Generate filtered noise with frequency sweep
                let mut rng = rand::thread_rng();
                let noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
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
            },
            SynthType::Zap { energy, decay, harmonic_content } => {
                // Energy burst with harmonics
                let fundamental = (2.0 * std::f32::consts::PI * freq * t).sin();
                
                // Add harmonics based on harmonic_content parameter
                let harm2 = (2.0 * std::f32::consts::PI * freq * 2.0 * t).sin() * harmonic_content * 0.5;
                let harm3 = (2.0 * std::f32::consts::PI * freq * 3.0 * t).sin() * harmonic_content * 0.33;
                let harm4 = (2.0 * std::f32::consts::PI * freq * 4.0 * t).sin() * harmonic_content * 0.25;
                
                let harmonic_mix = fundamental + harm2 + harm3 + harm4;
                
                // Sharp attack with controllable decay
                let envelope = (-t * (5.0 + decay * 10.0)).exp();
                
                // Add some high-frequency noise for "zap" character
                let mut rng = rand::thread_rng();
                let noise = (rng.gen::<f32>() - 0.5) * 0.2 * energy;
                
                (harmonic_mix + noise) * envelope * energy
            },
            SynthType::Chime { fundamental, harmonic_count, decay, inharmonicity } => {
                let base_freq = *fundamental;
                let mut output = 0.0;
                
                // Generate multiple harmonic partials
                for i in 1..=(*harmonic_count as usize) {
                    let harmonic_freq = base_freq * i as f32;
                    
                    // Add inharmonicity (slight detuning for realism)
                    let detuned_freq = harmonic_freq * (1.0 + inharmonicity * (i as f32 - 1.0) * 0.01);
                    
                    // Each harmonic has different amplitude and decay
                    let harmonic_amplitude = 1.0 / (i as f32);
                    let harmonic_decay = decay * (1.0 + i as f32 * 0.1);
                    
                    let harmonic_phase = 2.0 * std::f32::consts::PI * detuned_freq * t;
                    let harmonic_envelope = (-t * harmonic_decay).exp();
                    
                    output += harmonic_phase.sin() * harmonic_amplitude * harmonic_envelope;
                }
                
                output / (*harmonic_count as f32).sqrt() // Normalize
            },
            SynthType::Burst { center_freq, bandwidth, intensity, shape } => {
                // Spectral burst around center frequency
                let mut rng = rand::thread_rng();
                let noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
                // Create burst envelope
                let progress = t / params.duration.max(0.1);
                let envelope = match shape {
                    s if *s < 0.5 => {
                        // Sharp burst (exponential decay)
                        (-progress * 8.0).exp()
                    },
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
            },
            SynthType::Pad { warmth, movement, space, harmonic_evolution } => {
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
            },
            SynthType::Texture { roughness, evolution, spectral_tilt, modulation_depth } => {
                // Rough, evolving textural sound
                let mut rng = rand::thread_rng();
                let noise = (rng.gen::<f32>() - 0.5) * 2.0;
                
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
                let modulation = (2.0 * std::f32::consts::PI * freq * 0.618 * t).sin() * modulation_depth;
                
                // Mix oscillator, noise, and modulation
                let base_mix = osc * (1.0 - roughness) + noise * roughness;
                let modulated = base_mix * (1.0 + modulation);
                
                modulated * evolution_mod * tilt_factor * 0.7
            },
            SynthType::Drone { fundamental, overtone_spread, modulation } => {
                // Sustained drone with overtones
                let base_freq = *fundamental;
                let mut output = 0.0;
                
                // Fundamental
                output += (2.0 * std::f32::consts::PI * base_freq * t).sin() * 0.4;
                
                // Overtones with spreading
                for i in 2..=6 {
                    let overtone_freq = base_freq * i as f32 * (1.0 + overtone_spread * 0.1 * (i as f32 - 1.0));
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
            },
        }
    }

    /// Calculate synthesis envelope based on parameters
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

    /// Play an expressive R2D2 sound
    pub fn play_r2d2_expression(
        &self,
        base_freq: f32,
        emotion_intensity: f32,
        pitch_contour: Vec<f32>,
        duration: f32,
    ) -> Result<()> {
        // Generate R2D2-style audio samples with emotion-specific pitch contour
        let samples = self.generate_r2d2_samples_with_contour(
            base_freq,
            emotion_intensity,
            duration,
            &pitch_contour,
        );

        // Create audio source
        let source = R2D2AudioSource::new(samples, self.sample_rate);

        // Play the sound
        let sink = self.sink.lock().unwrap();
        sink.append(source);

        // Wait for playback to complete
        thread::sleep(Duration::from_secs_f32(duration + 0.1));

        Ok(())
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
                let resonance_component = (2.0 * std::f32::consts::PI * resonant_freq * t).sin() * resonance * 0.1;
                
                filtered + resonance_component
            },
            FilterType::HighPass => {
                // Simple one-pole highpass filter (complement of lowpass)
                let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_normalized).exp();
                let lowpass = sample * alpha;
                let highpass = sample - lowpass;
                
                // Add resonance
                let resonant_freq = filter.cutoff;
                let resonance_component = (2.0 * std::f32::consts::PI * resonant_freq * t).sin() * resonance * 0.1;
                
                highpass + resonance_component
            },
            FilterType::BandPass => {
                // Simple bandpass (combination of high and low pass)
                let bandwidth = filter.cutoff * 0.2; // 20% of cutoff frequency
                let low_cutoff = filter.cutoff - bandwidth;
                let high_cutoff = filter.cutoff + bandwidth;
                
                let low_alpha = 1.0 - (-2.0 * std::f32::consts::PI * (low_cutoff / (self.sample_rate * 0.5))).exp();
                let high_alpha = 1.0 - (-2.0 * std::f32::consts::PI * (high_cutoff / (self.sample_rate * 0.5))).exp();
                
                let lowpass = sample * low_alpha;
                let highpass = sample - (sample * high_alpha);
                let bandpass = lowpass - highpass;
                
                // Add resonance at center frequency
                let resonance_component = (2.0 * std::f32::consts::PI * filter.cutoff * t).sin() * resonance * 0.15;
                
                bandpass + resonance_component
            }
        }
    }

    /// Apply effect to sample
    fn apply_effect(&self, sample: f32, effect: &EffectParams, t: f32, sample_index: usize) -> f32 {
        match &effect.effect_type {
            EffectType::Reverb => {
                // Simple reverb using multiple delays
                let delay1 = 0.03; // 30ms
                let delay2 = 0.05; // 50ms
                let delay3 = 0.08; // 80ms
                
                let delay1_samples = (delay1 * self.sample_rate) as usize;
                let delay2_samples = (delay2 * self.sample_rate) as usize;
                let delay3_samples = (delay3 * self.sample_rate) as usize;
                
                let mut reverb_sum = sample;
                
                // Add delayed versions (simplified - would need delay buffers for real implementation)
                if sample_index >= delay1_samples {
                    reverb_sum += sample * 0.3 * effect.intensity;
                }
                if sample_index >= delay2_samples {
                    reverb_sum += sample * 0.2 * effect.intensity;
                }
                if sample_index >= delay3_samples {
                    reverb_sum += sample * 0.1 * effect.intensity;
                }
                
                // Mix dry and wet
                sample * (1.0 - effect.intensity * 0.5) + reverb_sum * effect.intensity * 0.5
            },
            EffectType::Chorus => {
                // Simple chorus using modulated delay
                let lfo_rate = 2.0; // 2 Hz
                let lfo = (2.0 * std::f32::consts::PI * lfo_rate * t).sin();
                let modulated_delay = 0.01 + lfo * 0.005; // 10ms ± 5ms
                
                // Simplified chorus (would need delay buffer for real implementation)
                let chorus_component = sample * 0.7; // Simplified modulated version
                let modulated_sample = chorus_component * (1.0 + lfo * 0.1);
                
                // Mix dry and wet
                sample * (1.0 - effect.intensity * 0.5) + modulated_sample * effect.intensity * 0.5
            },
            EffectType::Delay { delay_time } => {
                // Simple delay effect
                let delay_samples = (*delay_time * self.sample_rate) as usize;
                
                // Simplified delay (would need delay buffer for real implementation)
                let delayed_sample = if sample_index >= delay_samples {
                    sample * 0.6 // Simplified delayed version
                } else {
                    0.0
                };
                
                // Mix dry and wet with feedback
                sample + delayed_sample * effect.intensity
            }
        }
    }
}

/// Custom audio source for R2D2 samples
struct R2D2AudioSource {
    samples: Vec<f32>,
    sample_rate: f32,
    position: usize,
}

impl R2D2AudioSource {
    fn new(samples: Vec<f32>, sample_rate: f32) -> Self {
        Self {
            samples,
            sample_rate,
            position: 0,
        }
    }
}

impl Iterator for R2D2AudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.samples.len() {
            return None;
        }

        let sample = self.samples[self.position];
        self.position += 1;
        Some(sample)
    }
}

impl Source for R2D2AudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.samples.len() as f32 / self.sample_rate,
        ))
    }
}

/// Audio source for synthesized sounds using FunDSP
struct SynthAudioSource {
    samples: Vec<f32>,
    sample_rate: f32,
    position: usize,
}

impl SynthAudioSource {
    fn new(samples: Vec<f32>, sample_rate: f32) -> Self {
        SynthAudioSource {
            samples,
            sample_rate,
            position: 0,
        }
    }
}

impl Iterator for SynthAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position < self.samples.len() {
            let sample = self.samples[self.position];
            self.position += 1;
            Some(sample)
        } else {
            None
        }
    }
}

impl Source for SynthAudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.samples.len() as f32 / self.sample_rate,
        ))
    }
}
