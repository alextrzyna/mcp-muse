use crate::midi::parser::{ParsedMidi, MidiNote};
use crate::midi::{SimpleSequence, SimpleNote};
use rodio::{OutputStream, Sink, Source};
use oxisynth::{SoundFont, Synth, MidiEvent};
use std::time::Duration;

use std::path::PathBuf;
use std::env;
use std::fs;

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

    /// Play a simple sequence of notes (much easier to use!)
    pub fn play_simple(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!("Playing simple sequence with {} notes", sequence.notes.len());
        
        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Convert SimpleNote to MidiNote
        let notes: Vec<MidiNote> = sequence.notes.into_iter().map(|simple_note| {
            MidiNote {
                note: simple_note.note,
                velocity: simple_note.velocity,
                channel: simple_note.channel,
                start_time: Duration::from_secs_f64(simple_note.start_time),
                duration: Duration::from_secs_f64(simple_note.duration),
            }
        }).collect();

        // Calculate total playback time
        let total_time = notes.iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        // Log first few notes for debugging
        for (i, note) in notes.iter().take(3).enumerate() {
            tracing::info!("Note {}: MIDI note {}, velocity {}, start={:.2}s, duration={:.2}s", 
                          i, note.note, note.velocity, note.start_time.as_secs_f64(), note.duration.as_secs_f64());
        }

        tracing::info!("Total playback time: {:.2}s", total_time.as_secs_f64());

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(notes)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;
        
        tracing::info!("Created OxiSynth audio source, starting playback");
        self.sink.append(synth_source);
        self.sink.play();
        
        // Wait for the audio to finish playing
        // Add a small buffer to ensure we don't cut off early
        let wait_time = total_time + Duration::from_millis(500);
        tracing::info!("Waiting {:.2}s for playback to complete...", wait_time.as_secs_f64());
        
        std::thread::sleep(wait_time);
        
        tracing::info!("OxiSynth playback completed");

        Ok(())
    }

    /// Play parsed MIDI data (legacy method)
    pub fn play_midi(&self, parsed_midi: ParsedMidi) -> Result<(), String> {
        tracing::info!("Playing MIDI with {} notes using OxiSynth", parsed_midi.notes.len());
        
        if parsed_midi.notes.is_empty() {
            tracing::warn!("No notes to play - MIDI file contains no note events");
            return Ok(());
        }

        // Calculate total playback time
        let total_time = parsed_midi.notes.iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        // Log first few notes for debugging
        for (i, note) in parsed_midi.notes.iter().take(3).enumerate() {
            tracing::info!("Note {}: MIDI note {}, velocity {}, start={:?}, duration={:?}", 
                          i, note.note, note.velocity, note.start_time, note.duration);
        }

        tracing::info!("Total playback time: {:.2}s", total_time.as_secs_f64());

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(parsed_midi.notes)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;
        
        tracing::info!("Created OxiSynth audio source, starting playback");
        self.sink.append(synth_source);
        self.sink.play();
        
        // Wait for the audio to finish playing
        // Add a small buffer to ensure we don't cut off early
        let wait_time = total_time + Duration::from_millis(500);
        tracing::info!("Waiting {:.2}s for playback to complete...", wait_time.as_secs_f64());
        
        std::thread::sleep(wait_time);
        
        tracing::info!("OxiSynth playback completed");

        Ok(())
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}

fn find_soundfont() -> Result<PathBuf, String> {
    // Try to find the SoundFont in various locations
    let exe_path = env::current_exe().map_err(|e| format!("Cannot find executable: {}", e))?;
    let exe_dir = exe_path.parent().ok_or("Cannot find executable directory")?;
    
    let possible_paths = vec![
        exe_dir.join("../assets/FluidR3_GM.sf2"),      // Development
        exe_dir.join("assets/FluidR3_GM.sf2"),         // Installed
        PathBuf::from("assets/FluidR3_GM.sf2"),        // Current directory
        PathBuf::from("FluidR3_GM.sf2"),               // Current directory
        // Also check target/debug/assets for development
        PathBuf::from("target/debug/assets/FluidR3_GM.sf2"),
        PathBuf::from("target/release/assets/FluidR3_GM.sf2"),
        // Fallback to old soundfont if it exists
        exe_dir.join("../assets/TimGM6mb.sf2"),      // Development (old)
        exe_dir.join("assets/TimGM6mb.sf2"),         // Installed (old)
        PathBuf::from("assets/TimGM6mb.sf2"),        // Current directory (old)
        PathBuf::from("TimGM6mb.sf2"),               // Current directory (old)
        PathBuf::from("target/debug/assets/TimGM6mb.sf2"),   // Development (old)
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
}

impl OxiSynthSource {
    fn new(notes: Vec<MidiNote>) -> Result<Self, String> {
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
        
        // Calculate total duration
        let total_duration = notes.iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1))
            .max(Duration::from_secs(5)); // Minimum 5 seconds to hear the audio

        let buffer_size = 1024; // Process audio in chunks
        
        tracing::info!("OxiSynth source created: {} notes, total duration: {:?}", notes.len(), total_duration);

        Ok(Self {
            synth,
            notes,
            sample_rate,
            current_sample: 0,
            total_duration,
            left_buffer: vec![0.0; buffer_size],
            right_buffer: vec![0.0; buffer_size],
            buffer_size,
            buffer_pos: buffer_size, // Start with empty buffer to trigger initial fill
            playing_notes: std::collections::HashMap::new(),
            samples_generated: 0,
        })
    }

    fn process_audio_chunk(&mut self) {
        // Handle note on/off events for this chunk
        for note in &self.notes {
            let note_start_sample = (note.start_time.as_secs_f32() * self.sample_rate as f32) as u32;
            let note_end_sample = ((note.start_time + note.duration).as_secs_f32() * self.sample_rate as f32) as u32;
            
            let current_sample_u32 = self.current_sample as u32;
            let chunk_end = current_sample_u32 + self.buffer_size as u32;
            
            // Check if we should start this note in this chunk
            if note_start_sample >= current_sample_u32 && note_start_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if !self.playing_notes.contains_key(&key) {
                    let midi_event = MidiEvent::NoteOn {
                        channel: 0,
                        key: note.note,
                        vel: note.velocity,
                    };
                    let _ = self.synth.send_event(midi_event);
                    self.playing_notes.insert(key, note.duration);
                    tracing::debug!("Note ON: {} at sample {}", note.note, note_start_sample);
                }
            }
            
            // Check if we should end this note in this chunk
            if note_end_sample >= current_sample_u32 && note_end_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if self.playing_notes.remove(&key).is_some() {
                    let midi_event = MidiEvent::NoteOff {
                        channel: 0,
                        key: note.note,
                    };
                    let _ = self.synth.send_event(midi_event);
                    tracing::debug!("Note OFF: {} at sample {}", note.note, note_end_sample);
                }
            }
        }
        
        // Clear buffers
        self.left_buffer.fill(0.0);
        self.right_buffer.fill(0.0);
        
        // Render audio - OxiSynth expects stereo output
        self.synth.write((&mut self.left_buffer[..], &mut self.right_buffer[..]));
        
        // Log some debug info about the audio levels
        let max_left = self.left_buffer.iter().map(|x| x.abs()).fold(0.0, f32::max);
        let max_right = self.right_buffer.iter().map(|x| x.abs()).fold(0.0, f32::max);
        
        if max_left > 0.001 || max_right > 0.001 {
            tracing::debug!("Audio chunk: max_left={:.4}, max_right={:.4}, samples={}", 
                           max_left, max_right, self.buffer_size);
        }
        
        // Reset buffer position
        self.buffer_pos = 0;
        self.samples_generated += self.buffer_size;
    }
}

impl Iterator for OxiSynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time = Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);
        
        if current_time > self.total_duration {
            tracing::info!("Audio playback finished after {} samples ({:.2}s)", 
                          self.samples_generated, current_time.as_secs_f32());
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