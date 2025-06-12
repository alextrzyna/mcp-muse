use crate::midi::parser::{ParsedMidi, MidiNote};
use rodio::{OutputStream, Sink, Source};
use std::time::Duration;
use std::f32::consts::PI;

pub struct MidiPlayer {
    _stream: OutputStream,
    sink: Sink,
}

impl MidiPlayer {
    pub fn new() -> Result<Self, String> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to create audio output stream: {}", e))?;
        
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        Ok(MidiPlayer { _stream, sink })
    }

    pub fn play_midi(&self, parsed_midi: ParsedMidi) -> Result<(), String> {
        tracing::info!("Playing MIDI with {} notes", parsed_midi.notes.len());

        // Create a composite source by mixing all notes
        let mixed_source = MidiSource::new(parsed_midi.notes);
        
        self.sink.append(mixed_source);
        self.sink.play();

        Ok(())
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

// Custom source that generates audio for MIDI notes
struct MidiSource {
    notes: Vec<MidiNote>,
    sample_rate: u32,
    current_sample: usize,
    total_duration: Duration,
}

impl MidiSource {
    fn new(notes: Vec<MidiNote>) -> Self {
        let sample_rate = 44100;
        
        // Calculate total duration needed
        let total_duration = notes.iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        Self {
            notes,
            sample_rate,
            current_sample: 0,
            total_duration,
        }
    }

    fn note_to_frequency(note: u8) -> f32 {
        // MIDI note 69 (A4) = 440 Hz
        // Each semitone is a factor of 2^(1/12)
        440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
    }

    fn generate_sample(&self, time: Duration) -> f32 {
        let mut sample = 0.0;
        let mut active_notes = 0;

        for note in &self.notes {
            // Check if this note should be playing at this time
            if time >= note.start_time && time < note.start_time + note.duration {
                let note_time = time - note.start_time;
                let frequency = Self::note_to_frequency(note.note);
                let velocity_factor = (note.velocity as f32) / 127.0;
                
                // Generate a simple sine wave with envelope
                let phase = 2.0 * PI * frequency * note_time.as_secs_f32();
                let envelope = Self::adsr_envelope(note_time, note.duration);
                
                sample += (phase.sin() * velocity_factor * envelope * 0.3).max(-1.0).min(1.0);
                active_notes += 1;
            }
        }

        // Normalize by number of active notes to prevent clipping
        if active_notes > 0 {
            sample / (active_notes as f32).sqrt()
        } else {
            0.0
        }
    }

    fn adsr_envelope(note_time: Duration, total_duration: Duration) -> f32 {
        let t = note_time.as_secs_f32();
        let total = total_duration.as_secs_f32();
        
        // Simple envelope: quick attack, sustain, quick release
        let attack_time = 0.01; // 10ms attack
        let release_time = 0.05; // 50ms release
        
        if t < attack_time {
            // Attack phase
            t / attack_time
        } else if t < total - release_time {
            // Sustain phase
            1.0
        } else {
            // Release phase
            (total - t) / release_time
        }
    }
}

impl Iterator for MidiSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let time = Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);
        
        if time > self.total_duration {
            return None;
        }

        let sample = self.generate_sample(time);
        self.current_sample += 1;
        
        Some(sample)
    }
}

impl Source for MidiSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1 // Mono
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
    fn test_note_to_frequency() {
        // Test some known MIDI note frequencies
        assert!((MidiSource::note_to_frequency(69) - 440.0).abs() < 0.01); // A4 = 440 Hz
        assert!((MidiSource::note_to_frequency(60) - 261.63).abs() < 0.01); // C4 ≈ 261.63 Hz
        assert!((MidiSource::note_to_frequency(81) - 880.0).abs() < 0.01); // A5 = 880 Hz
    }

    #[test]
    fn test_adsr_envelope() {
        let duration = Duration::from_secs(1);
        
        // Test attack phase (first 10ms)
        let attack_val = MidiSource::adsr_envelope(Duration::from_millis(5), duration);
        assert!(attack_val > 0.0 && attack_val < 1.0, "Should be in attack phase");
        
        // Test sustain phase (middle)
        let sustain_val = MidiSource::adsr_envelope(Duration::from_millis(500), duration);
        assert!((sustain_val - 1.0).abs() < 0.01, "Should be at full volume in sustain");
        
        // Test release phase (last 50ms)
        let release_val = MidiSource::adsr_envelope(Duration::from_millis(970), duration);
        assert!(release_val > 0.0 && release_val < 1.0, "Should be in release phase");
    }

    #[test]
    fn test_midi_source_creation() {
        let notes = vec![
            MidiNote {
                note: 60,
                velocity: 100,
                channel: 0,
                start_time: Duration::from_secs(0),
                duration: Duration::from_secs(1),
            }
        ];
        
        let source = MidiSource::new(notes);
        assert_eq!(source.sample_rate, 44100);
        assert_eq!(source.notes.len(), 1);
        assert_eq!(source.total_duration, Duration::from_secs(1));
    }

    #[test]
    fn test_generate_sample_silence() {
        let notes = vec![
            MidiNote {
                note: 60,
                velocity: 100,
                channel: 0,
                start_time: Duration::from_secs(1), // Note starts at 1 second
                duration: Duration::from_secs(1),
            }
        ];
        
        let source = MidiSource::new(notes);
        
        // At time 0, no note should be playing
        let sample = source.generate_sample(Duration::from_secs(0));
        assert_eq!(sample, 0.0, "Should be silent when no notes are playing");
    }

    #[test]
    fn test_generate_sample_with_note() {
        let notes = vec![
            MidiNote {
                note: 60,
                velocity: 127, // Max velocity
                channel: 0,
                start_time: Duration::from_secs(0),
                duration: Duration::from_secs(1),
            }
        ];
        
        let source = MidiSource::new(notes);
        
        // At time 0.1s, the note should be playing
        let sample = source.generate_sample(Duration::from_millis(100));
        assert!(sample.abs() > 0.01, "Should generate non-zero audio when note is playing");
    }

    #[test]
    fn test_multiple_notes_mixing() {
        let notes = vec![
            MidiNote {
                note: 60, // C4
                velocity: 100,
                channel: 0,
                start_time: Duration::from_secs(0),
                duration: Duration::from_secs(1),
            },
            MidiNote {
                note: 64, // E4
                velocity: 100,
                channel: 0,
                start_time: Duration::from_secs(0),
                duration: Duration::from_secs(1),
            }
        ];
        
        let source = MidiSource::new(notes);
        
        // Both notes should be playing and mixed together
        let sample = source.generate_sample(Duration::from_millis(100));
        assert!(sample.abs() > 0.01, "Should mix multiple notes together");
    }

    // Note: We can't easily test MidiPlayer::new() in unit tests because it requires
    // audio hardware. In a real testing environment, we'd use dependency injection
    // or mocking to test the player without requiring actual audio output.
} 