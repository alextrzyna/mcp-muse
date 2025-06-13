use anyhow::Result;
use rodio::{OutputStream, Sink, Source};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Core expressive synthesizer for R2D2-style vocalizations
/// Using a simpler approach with direct audio generation
pub struct ExpressiveSynth {
    sample_rate: f32,
    _stream: OutputStream,
    sink: Arc<Mutex<Sink>>,
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

    /// Play an expressive R2D2 sound
    pub fn play_r2d2_expression(
        &self,
        base_freq: f32,
        emotion_intensity: f32,
        pitch_contour: Vec<f32>,
        duration: f32,
    ) -> Result<()> {
        // Generate R2D2-style audio samples with emotion-specific pitch contour
        let samples = self.generate_r2d2_samples_with_contour(base_freq, emotion_intensity, duration, &pitch_contour);
        
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
    fn generate_r2d2_samples_with_contour(&self, base_freq: f32, emotion_intensity: f32, duration: f32, pitch_contour: &[f32]) -> Vec<f32> {
        let sample_count = (self.sample_rate * duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);
        
        let dt = 1.0 / self.sample_rate;
        
        // Simplified parameters to emphasize pitch contours
        let carrier_freq = base_freq;
        let mod_freq = base_freq * 0.4; // Simple ring modulation ratio
        
        // Very subtle vibrato so it doesn't mask pitch contours
        let vibrato_rate = 2.5; // Even gentler
        let vibrato_depth = 0.015; // Very subtle
        
        for i in 0..sample_count {
            let t = i as f32 * dt;
            let progress = t / duration; // 0.0 to 1.0 through the sound
            
            // PROMINENT PITCH CONTOUR from emotion presets
            let pitch_multiplier = self.interpolate_pitch_contour(progress, pitch_contour, emotion_intensity);
            let contoured_freq = carrier_freq * pitch_multiplier;
            
            // Very subtle vibrato that won't mask the pitch contour
            let vibrato = (2.0 * std::f32::consts::PI * vibrato_rate * t).sin() * vibrato_depth;
            let final_carrier_freq = contoured_freq * (1.0 + vibrato);
            let final_mod_freq = mod_freq * pitch_multiplier * (1.0 + vibrato * 0.3);
            
            // Generate carrier and modulator
            let carrier = (2.0 * std::f32::consts::PI * final_carrier_freq * t).sin();
            let modulator = (2.0 * std::f32::consts::PI * final_mod_freq * t).sin();
            
            // Ring modulation (the core R2D2 sound)
            let ring_mod = carrier * modulator;
            
            // Minimal harmonic content to keep focus on pitch contour
            let harmonic2 = (2.0 * std::f32::consts::PI * final_carrier_freq * 1.5 * t).sin() * 0.15;
            
            // Mix components - keep it simple and focused
            let voice = ring_mod * 0.7 + harmonic2;
            
            // Apply envelope
            let envelope = self.calculate_envelope(t, duration, emotion_intensity);
            
            // Light soft clipping
            let clipped = self.soft_clip(voice);
            
            let sample = clipped * envelope * 0.25;
            samples.push(sample);
        }
        
        samples
    }
    
    /// Generate R2D2-style audio samples with prominent pitch contours (fallback method)
    fn generate_r2d2_samples(&self, base_freq: f32, emotion_intensity: f32, duration: f32) -> Vec<f32> {
        let sample_count = (self.sample_rate * duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);
        
        let dt = 1.0 / self.sample_rate;
        
        // Simplified parameters to emphasize pitch contours
        let carrier_freq = base_freq;
        let mod_freq = base_freq * 0.4; // Simple ring modulation ratio
        
        // Much subtler vibrato so it doesn't mask pitch contours
        let vibrato_rate = 3.0; // Fixed, gentle rate
        let vibrato_depth = 0.02; // Very subtle
        
        for i in 0..sample_count {
            let t = i as f32 * dt;
            let progress = t / duration; // 0.0 to 1.0 through the sound
            
            // PROMINENT PITCH CONTOUR - this should be the main feature
            let pitch_multiplier = self.calculate_pitch_contour(progress, emotion_intensity);
            let contoured_freq = carrier_freq * pitch_multiplier;
            
            // Very subtle vibrato that won't mask the pitch contour
            let vibrato = (2.0 * std::f32::consts::PI * vibrato_rate * t).sin() * vibrato_depth;
            let final_carrier_freq = contoured_freq * (1.0 + vibrato);
            let final_mod_freq = mod_freq * pitch_multiplier * (1.0 + vibrato * 0.5);
            
            // Generate carrier and modulator
            let carrier = (2.0 * std::f32::consts::PI * final_carrier_freq * t).sin();
            let modulator = (2.0 * std::f32::consts::PI * final_mod_freq * t).sin();
            
            // Ring modulation (the core R2D2 sound)
            let ring_mod = carrier * modulator;
            
            // Minimal harmonic content to keep focus on pitch contour
            let harmonic2 = (2.0 * std::f32::consts::PI * final_carrier_freq * 2.0 * t).sin() * 0.2;
            
            // Simple formant emphasis without complex filtering
            let formant_emphasis = (2.0 * std::f32::consts::PI * (final_carrier_freq * 1.5) * t).sin() * 0.15;
            
            // Mix components - keep it simple
            let voice = ring_mod * 0.6 + harmonic2 + formant_emphasis;
            
            // Apply envelope
            let envelope = self.calculate_envelope(t, duration, emotion_intensity);
            
            // Light soft clipping
            let clipped = self.soft_clip(voice);
            
            let sample = clipped * envelope * 0.2;
            samples.push(sample);
        }
        
        samples
    }
    
    /// Interpolate pitch contour from emotion presets
    fn interpolate_pitch_contour(&self, progress: f32, pitch_contour: &[f32], intensity: f32) -> f32 {
        if pitch_contour.is_empty() {
            return 1.0;
        }
        
        if pitch_contour.len() == 1 {
            return 1.0 + pitch_contour[0] * intensity;
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
        // The contour values are 0.0-1.0, we want pitch multipliers around 0.5-2.0
        let pitch_multiplier = 0.7 + interpolated * intensity * 1.0;
        
        pitch_multiplier.max(0.3).min(2.5) // Clamp to reasonable range
    }
    
    /// Calculate pitch contour multiplier based on progress through the sound (fallback)
    fn calculate_pitch_contour(&self, progress: f32, intensity: f32) -> f32 {
        // Simple neutral contour for fallback
        1.0 + (progress * 0.2 - 0.1) * intensity
    }
    
    /// Simple resonator simulation for formant filtering
    fn simple_resonator(&self, input: f32, freq: f32, t: f32) -> f32 {
        // Simulate a resonant filter by emphasizing frequencies near the formant
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        let resonance = phase.sin() * 0.7 + (phase * 1.5).sin() * 0.3;
        input * resonance
    }
    
    /// Soft clipping to prevent harsh distortion
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
        Some(Duration::from_secs_f32(self.samples.len() as f32 / self.sample_rate))
    }
} 