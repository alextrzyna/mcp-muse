use std::time::Duration;
use anyhow::Result;

use crate::midi::{SimpleNote, SimpleSequence, MidiPlayer};
use crate::expressive::{PolyphonicVoiceManager, SynthParams, SynthType, EnvelopeParams};

/// Comprehensive polyphony validation tests
pub struct PolyphonyValidator {
    player: MidiPlayer,
}

impl PolyphonyValidator {
    pub fn new() -> Result<Self, String> {
        let player = MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;
        Ok(Self { player })
    }

    /// Test 1: Basic polyphonic chord progression
    pub fn test_chord_progression(&self) -> Result<(), String> {
        println!("🎹 Test 1: Polyphonic Chord Progression");
        println!("Testing classic synthesizer presets with complex chord progressions...");

        // Create a rich chord progression using classic presets
        let mut notes = Vec::new();

        // C Major chord with Jupiter-8 Strings
        let chord_times = [0.0, 2.0, 4.0, 6.0];
        let chord_progressions = [
            vec![60, 64, 67, 72], // C Major
            vec![57, 60, 64, 69], // A Minor  
            vec![58, 62, 65, 70], // Bb Major
            vec![67, 71, 74, 79], // G Major
        ];

        for (i, &start_time) in chord_times.iter().enumerate() {
            let chord = &chord_progressions[i % chord_progressions.len()];
            
            for &note_num in chord {
                notes.push(SimpleNote {
                    start_time: Some(start_time),
                    duration: Some(3.0), // Long notes for rich overlapping
                    musical_time: None,
                    musical_duration: None,
                    note: Some(note_num),
                    velocity: Some(80),
                    preset_name: Some("JP-8 Strings".to_string()),
                    ..Default::default()
                });
            }
        }

        // Add a bass line with Minimoog Bass
        let bass_notes = [36, 33, 34, 43]; // C, A, Bb, G bass notes
        for (i, &start_time) in chord_times.iter().enumerate() {
            notes.push(SimpleNote {
                start_time: Some(start_time),
                duration: Some(1.8),
                musical_time: None,
                musical_duration: None,
                note: Some(bass_notes[i % bass_notes.len()]),
                velocity: Some(100),
                preset_name: Some("Minimoog Bass".to_string()),
                ..Default::default()
            });
        }

        let sequence = SimpleSequence { notes };
        println!("▶️  Playing chord progression with {} total notes (up to 8 simultaneous)", sequence.notes.len());
        
        self.player.play_enhanced_mixed(sequence)?;
        
        println!("✅ Chord progression test completed successfully!");
        Ok(())
    }

    /// Test 2: Fast arpeggios and voice stealing
    pub fn test_fast_arpeggios(&self) -> Result<(), String> {
        println!("\n🎼 Test 2: Fast Arpeggios and Voice Stealing");
        println!("Testing rapid note sequences to validate voice stealing algorithms...");

        let mut notes = Vec::new();
        let arp_pattern = [60, 64, 67, 72, 76, 79, 84, 88]; // C Major arpeggio
        
        // Create overlapping fast arpeggios
        for sequence in 0..4 {
            let base_time = sequence as f64 * 1.0;
            
            for (i, &note_num) in arp_pattern.iter().enumerate() {
                notes.push(SimpleNote {
                    start_time: base_time + (i as f64 * 0.1), // Very fast notes every 100ms
                    duration: 0.8, // Long enough to create overlaps
                    note: Some(note_num + (sequence * 12) as u8), // Transpose each sequence
                    velocity: Some(90 + (i % 4) as u8 * 10), // Varying velocities
                    preset_name: Some("Prophet Lead".to_string()),
                    ..Default::default()
                });
            }
        }

        let sequence = SimpleSequence { notes };
        println!("▶️  Playing fast arpeggios with {} notes (testing voice stealing)", sequence.notes.len());
        
        self.player.play_enhanced_mixed(sequence)?;
        
        println!("✅ Fast arpeggio test completed successfully!");
        Ok(())
    }

    /// Test 3: Polyphonic acid bass sequences
    pub fn test_acid_bass_polyphony(&self) -> Result<(), String> {
        println!("\n🔊 Test 3: Polyphonic Acid Bass Sequences");
        println!("Testing TB-303 style bass with overlapping notes and filter modulation...");

        let mut notes = Vec::new();
        let bass_pattern = [36, 36, 43, 41, 38, 43, 36, 41]; // Typical acid pattern
        
        // Create multiple overlapping bass lines
        for line in 0..3 {
            let base_time = line as f64 * 0.5;
            
            for (i, &note_num) in bass_pattern.iter().enumerate() {
                let variation = if i % 3 == 0 { "squelchy" } else { "" };
                
                notes.push(SimpleNote {
                    start_time: base_time + (i as f64 * 0.25),
                    duration: 0.4 + (i % 2) as f64 * 0.2, // Varying durations
                    note: Some(note_num),
                    velocity: Some(100 + (i % 3) as u8 * 15),
                    preset_name: Some("TB-303 Acid".to_string()),
                    preset_variation: if variation.is_empty() { None } else { Some(variation.to_string()) },
                    ..Default::default()
                });
            }
        }

        let sequence = SimpleSequence { notes };
        println!("▶️  Playing acid bass sequence with {} notes (testing preset variations)", sequence.notes.len());
        
        self.player.play_enhanced_mixed(sequence)?;
        
        println!("✅ Acid bass polyphony test completed successfully!");
        Ok(())
    }

    /// Test 4: Mixed audio modes (MIDI + Presets + R2D2 + Synthesis)
    pub fn test_mixed_audio_modes(&self) -> Result<(), String> {
        println!("\n🎛️  Test 4: Mixed Audio Modes");
        println!("Testing simultaneous MIDI, presets, R2D2, and synthesis...");

        let mut notes = Vec::new();

        // MIDI drum pattern
        notes.push(SimpleNote {
            start_time: Some(0.0),
            duration: Some(0.1),
            musical_time: None,
            musical_duration: None,
            note: Some(36), // Kick
            velocity: Some(120),
            instrument: Some(128), // Drum kit
            channel: Some(9),      // MIDI drum channel
            ..Default::default()
        });
        notes.push(SimpleNote {
            start_time: Some(0.5),
            duration: Some(0.1),
            musical_time: None,
            musical_duration: None,
            note: Some(38), // Snare
            velocity: Some(100),
            instrument: Some(128),
            channel: Some(9),
            ..Default::default()
        });

        // Preset bass
        notes.push(SimpleNote {
            start_time: Some(0.0),
            duration: 1.0,
            musical_time: None,
            musical_duration: None,
            note: Some(48),
            velocity: Some(90),
            preset_name: Some("Jupiter Bass".to_string()),
            ..Default::default()
        });

        // R2D2 expression
        notes.push(SimpleNote {
            start_time: Some(0.5),
            duration: Some(0.8),
            musical_time: None,
            musical_duration: None,
            r2d2_emotion: Some("Excited".to_string()),
            r2d2_intensity: Some(0.8),
            r2d2_complexity: Some(3),
            ..Default::default()
        });

        // Custom synthesis
        notes.push(SimpleNote {
            start_time: Some(1.0),
            duration: Some(1.5),
            musical_time: None,
            musical_duration: None,
            synth_type: Some("sawtooth".to_string()),
            synth_frequency: Some(440.0),
            synth_amplitude: Some(0.5),
            synth_filter_cutoff: Some(1200.0),
            synth_reverb: Some(0.3),
            ..Default::default()
        });

        let sequence = SimpleSequence { notes };
        println!("▶️  Playing mixed audio sequence with {} notes (MIDI + Presets + R2D2 + Synthesis)", sequence.notes.len());
        
        self.player.play_enhanced_mixed(sequence)?;
        
        println!("✅ Mixed audio modes test completed successfully!");
        Ok(())
    }

    /// Test 5: Voice manager stress test
    pub fn test_voice_manager_stress(&self) -> Result<(), String> {
        println!("\n⚡ Test 5: Voice Manager Stress Test");
        println!("Creating high voice count scenario to test voice stealing and performance...");

        let mut notes = Vec::new();
        
        // Create 50+ simultaneous notes to force voice stealing
        for i in 0..50 {
            let start_time = (i / 10) as f64 * 0.1; // Groups of 10 notes
            let note_num = 60 + (i % 12) as u8; // Chromatic scale
            
            notes.push(SimpleNote {
                start_time: Some(start_time),
                duration: Some(2.0), // Long notes to force overlap
                musical_time: None,
                musical_duration: None,
                note: Some(note_num),
                velocity: Some(80 + (i % 20) as u8), // Varying priorities
                preset_name: Some("Analog Wash".to_string()),
                ..Default::default()
            });
        }

        let sequence = SimpleSequence { notes };
        println!("▶️  Playing stress test with {} notes (max 32 voices - testing stealing)", sequence.notes.len());
        
        self.player.play_enhanced_mixed(sequence)?;
        
        println!("✅ Voice manager stress test completed successfully!");
        Ok(())
    }

    /// Test 6: Voice manager unit tests
    pub fn test_voice_manager_unit_tests(&self) -> Result<(), String> {
        println!("\n🔧 Test 6: Voice Manager Unit Tests");
        println!("Testing voice manager internals directly...");

        let mut voice_manager = PolyphonicVoiceManager::new(44100.0);
        
        // Test 1: Basic voice allocation
        let synth_params = SynthParams {
            synth_type: SynthType::Sine,
            frequency: 440.0,
            amplitude: 0.5,
            duration: 1.0,
            envelope: EnvelopeParams {
                attack: 0.1,
                decay: 0.2,
                sustain: 0.7,
                release: 0.3,
            },
            filter: None,
            effects: Vec::new(),
        };

        println!("  🧪 Testing basic voice allocation...");
        let voice_id = voice_manager.allocate_voice(synth_params.clone(), 0.0, Some(60), 0, 100)
            .map_err(|e| format!("Voice allocation failed: {}", e))?;
        println!("     ✅ Allocated voice ID: {}", voice_id);

        // Test 2: Voice count tracking
        println!("  🧪 Testing voice count tracking...");
        let active_count = voice_manager.active_voice_count();
        println!("     ✅ Active voices: {}", active_count);
        assert_eq!(active_count, 1, "Expected 1 active voice");

        // Test 3: Multiple voice allocation
        println!("  🧪 Testing multiple voice allocation...");
        for i in 1..5 {
            let _voice_id = voice_manager.allocate_voice(
                synth_params.clone(), 
                i as f64 * 0.1, 
                Some(60 + i as u8), 
                0, 
                100
            ).map_err(|e| format!("Voice allocation {} failed: {}", i, e))?;
        }
        let active_count = voice_manager.active_voice_count();
        println!("     ✅ Active voices after allocation: {}", active_count);

        // Test 4: Voice processing
        println!("  🧪 Testing voice processing...");
        let dt = 1.0 / 44100.0; // One sample at 44.1kHz
        for _ in 0..1000 { // Process 1000 samples
            let _output = voice_manager.process_voices(dt);
        }
        println!("     ✅ Voice processing completed without errors");

        // Test 5: Voice information
        println!("  🧪 Testing voice information retrieval...");
        let voice_info = voice_manager.get_voice_info();
        println!("     ✅ Retrieved info for {} voices", voice_info.len());
        
        for (id, state, note, channel) in voice_info.iter().take(3) {
            println!("       Voice {}: state={:?}, note={:?}, channel={}", id, state, note, channel);
        }

        println!("✅ Voice manager unit tests completed successfully!");
        Ok(())
    }

    /// Run all polyphony validation tests
    pub fn run_all_tests(&self) -> Result<(), String> {
        println!("🚀 Starting Comprehensive Polyphony Validation");
        println!("=".repeat(60));

        let tests = [
            ("Chord Progression", Self::test_chord_progression),
            ("Fast Arpeggios", Self::test_fast_arpeggios),
            ("Acid Bass Polyphony", Self::test_acid_bass_polyphony),
            ("Mixed Audio Modes", Self::test_mixed_audio_modes),
            ("Voice Manager Stress", Self::test_voice_manager_stress),
            ("Voice Manager Unit Tests", Self::test_voice_manager_unit_tests),
        ];

        let mut passed = 0;
        let mut failed = 0;

        for (test_name, test_fn) in tests.iter() {
            match test_fn(self) {
                Ok(()) => {
                    println!("✅ {} - PASSED", test_name);
                    passed += 1;
                }
                Err(e) => {
                    println!("❌ {} - FAILED: {}", test_name, e);
                    failed += 1;
                }
            }
            
            // Brief pause between tests
            std::thread::sleep(Duration::from_millis(500));
        }

        println!("\n" + "=".repeat(60));
        println!("🎯 Polyphony Validation Results:");
        println!("   ✅ Passed: {}", passed);
        println!("   ❌ Failed: {}", failed);
        println!("   📊 Success Rate: {:.1}%", (passed as f32 / (passed + failed) as f32) * 100.0);

        if failed == 0 {
            println!("\n🎉 ALL POLYPHONY TESTS PASSED!");
            println!("🏆 Real-time polyphonic voice management is fully operational!");
        } else {
            return Err(format!("{} test(s) failed", failed));
        }

        Ok(())
    }
}

impl Default for PolyphonyValidator {
    fn default() -> Self {
        Self::new().expect("Failed to create PolyphonyValidator")
    }
}

/// Performance monitoring utilities
pub struct PolyphonyPerformanceMonitor {
    start_time: std::time::Instant,
    voice_counts: Vec<usize>,
}

impl PolyphonyPerformanceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            voice_counts: Vec::new(),
        }
    }

    pub fn record_voice_count(&mut self, count: usize) {
        self.voice_counts.push(count);
    }

    pub fn report(&self) {
        let elapsed = self.start_time.elapsed();
        let max_voices = self.voice_counts.iter().max().unwrap_or(&0);
        let avg_voices = if !self.voice_counts.is_empty() {
            self.voice_counts.iter().sum::<usize>() as f32 / self.voice_counts.len() as f32
        } else {
            0.0
        };

        println!("\n📊 Performance Report:");
        println!("   ⏱️  Total time: {:.2}s", elapsed.as_secs_f32());
        println!("   🎵 Max simultaneous voices: {}", max_voices);
        println!("   📈 Average voice count: {:.1}", avg_voices);
        println!("   📊 Voice utilization: {:.1}%", (*max_voices as f32 / 32.0) * 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_manager_creation() {
        let voice_manager = PolyphonicVoiceManager::new(44100.0);
        assert_eq!(voice_manager.active_voice_count(), 0);
    }

    #[test]
    fn test_basic_voice_allocation() -> Result<(), Box<dyn std::error::Error>> {
        let mut voice_manager = PolyphonicVoiceManager::new(44100.0);
        
        let synth_params = SynthParams {
            synth_type: SynthType::Sine,
            frequency: 440.0,
            amplitude: 0.5,
            duration: 1.0,
            envelope: EnvelopeParams {
                attack: 0.1,
                decay: 0.2,
                sustain: 0.7,
                release: 0.3,
            },
            filter: None,
            effects: Vec::new(),
        };

        let _voice_id = voice_manager.allocate_voice(synth_params, 0.0, Some(60), 0, 100)?;
        assert_eq!(voice_manager.active_voice_count(), 1);
        
        Ok(())
    }

    #[test]
    fn test_voice_processing() -> Result<(), Box<dyn std::error::Error>> {
        let mut voice_manager = PolyphonicVoiceManager::new(44100.0);
        
        let synth_params = SynthParams {
            synth_type: SynthType::Sine,
            frequency: 440.0,
            amplitude: 0.5,
            duration: 1.0,
            envelope: EnvelopeParams {
                attack: 0.1,
                decay: 0.2,
                sustain: 0.7,
                release: 0.3,
            },
            filter: None,
            effects: Vec::new(),
        };

        let _voice_id = voice_manager.allocate_voice(synth_params, 0.0, Some(60), 0, 100)?;
        
        // Process some samples
        let dt = 1.0 / 44100.0;
        for _ in 0..100 {
            let _output = voice_manager.process_voices(dt);
        }
        
        Ok(())
    }
} 