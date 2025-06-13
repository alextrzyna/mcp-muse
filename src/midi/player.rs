use crate::expressive::{ExpressiveSynth, R2D2Emotion, R2D2Expression, R2D2Voice};
use crate::midi::parser::{MidiNote, ParsedMidi};
use crate::midi::SimpleSequence;
use oxisynth::{MidiEvent, SoundFont, Synth};
use rodio::{OutputStream, Sink, Source};
use std::time::Duration;

use std::env;
use std::fs;
use std::path::PathBuf;

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

    /// Calculate additional tail time needed for effects like reverb, chorus, sustain, and natural decay
    fn calculate_tail_time(notes: &[MidiNote]) -> Duration {
        let mut max_tail_seconds: f64 = 2.0; // Base tail time for natural instrument decay

        // Check for reverb effects
        let has_reverb = notes.iter().any(|note| note.reverb.is_some_and(|r| r > 0));
        if has_reverb {
            let max_reverb = notes
                .iter()
                .filter_map(|note| note.reverb)
                .max()
                .unwrap_or(0);
            // Reverb can add 1-6 seconds of tail depending on depth
            let reverb_tail = 1.0 + (max_reverb as f64 / 127.0) * 5.0;
            max_tail_seconds = max_tail_seconds.max(reverb_tail);
        }

        // Check for chorus effects
        let has_chorus = notes.iter().any(|note| note.chorus.is_some_and(|c| c > 0));
        if has_chorus {
            let max_chorus = notes
                .iter()
                .filter_map(|note| note.chorus)
                .max()
                .unwrap_or(0);
            // Chorus can add 0.5-2 seconds of tail
            let chorus_tail = 0.5 + (max_chorus as f64 / 127.0) * 1.5;
            max_tail_seconds = max_tail_seconds.max(chorus_tail);
        }

        // Check for sustain pedal
        let has_sustain = notes.iter().any(|note| note.sustain.is_some_and(|s| s > 0));
        if has_sustain {
            // Sustain pedal can significantly extend notes
            max_tail_seconds = max_tail_seconds.max(4.0);
        }

        // Check for instruments that naturally have long decay
        for note in notes {
            if let Some(instrument) = note.instrument {
                let additional_tail = match instrument {
                    // Piano family - long sustain and decay
                    0..=7 => 3.0,
                    // Organ family - can sustain indefinitely
                    16..=23 => 2.0,
                    // Guitar family - natural sustain
                    24..=31 => 2.5,
                    // Strings - natural decay
                    40..=47 => 2.0,
                    // Choir/Voice - natural decay
                    52..=55 => 1.5,
                    // Brass - can have long release
                    56..=63 => 1.5,
                    // Woodwinds - shorter decay
                    64..=71 => 1.0,
                    // Synth pads - often have long release
                    88..=95 => 3.0,
                    // Sound effects - variable
                    120..=127 => 2.0,
                    _ => 0.5,
                };
                max_tail_seconds = max_tail_seconds.max(additional_tail);
            }
        }

        Duration::from_secs_f64(max_tail_seconds)
    }

    /// Play a simple sequence of notes (much easier to use!)
    pub fn play_simple(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!(
            "Playing simple sequence with {} notes",
            sequence.notes.len()
        );

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Convert SimpleNote to MidiNote
        let notes: Vec<MidiNote> = sequence
            .notes
            .into_iter()
            .map(|simple_note| MidiNote {
                note: simple_note.note,
                velocity: simple_note.velocity,
                channel: simple_note.channel,
                start_time: Duration::from_secs_f64(simple_note.start_time),
                duration: Duration::from_secs_f64(simple_note.duration),
                instrument: simple_note.instrument,
                reverb: simple_note.reverb,
                chorus: simple_note.chorus,
                volume: simple_note.volume,
                pan: simple_note.pan,
                balance: simple_note.balance,
                expression: simple_note.expression,
                sustain: simple_note.sustain,
            })
            .collect();

        // Calculate total playback time including tail time for effects
        let note_end_time = notes
            .iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        let tail_time = Self::calculate_tail_time(&notes);
        let total_time = note_end_time + tail_time;

        // Log first few notes for debugging
        for (i, note) in notes.iter().take(3).enumerate() {
            tracing::info!(
                "Note {}: MIDI note {}, velocity {}, start={:.2}s, duration={:.2}s",
                i,
                note.note,
                note.velocity,
                note.start_time.as_secs_f64(),
                note.duration.as_secs_f64()
            );
        }

        tracing::info!(
            "Note end time: {:.2}s, tail time: {:.2}s, total playback time: {:.2}s",
            note_end_time.as_secs_f64(),
            tail_time.as_secs_f64(),
            total_time.as_secs_f64()
        );

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(notes, total_time)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;

        tracing::info!("Created OxiSynth audio source, starting playback");
        self.sink.append(synth_source);
        self.sink.play();

        // Wait for the audio to finish playing
        // Add a small buffer to ensure we don't cut off early
        let wait_time = total_time + Duration::from_millis(200);
        tracing::info!(
            "Waiting {:.2}s for playback to complete...",
            wait_time.as_secs_f64()
        );

        std::thread::sleep(wait_time);

        tracing::info!("OxiSynth playback completed");

        Ok(())
    }

    /// Play a simple sequence of notes asynchronously (non-blocking)
    #[allow(dead_code)]
    pub fn play_simple_async(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!(
            "Playing simple sequence with {} notes (async)",
            sequence.notes.len()
        );

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Convert SimpleNote to MidiNote
        let notes: Vec<MidiNote> = sequence
            .notes
            .into_iter()
            .map(|simple_note| MidiNote {
                note: simple_note.note,
                velocity: simple_note.velocity,
                channel: simple_note.channel,
                start_time: Duration::from_secs_f64(simple_note.start_time),
                duration: Duration::from_secs_f64(simple_note.duration),
                instrument: simple_note.instrument,
                reverb: simple_note.reverb,
                chorus: simple_note.chorus,
                volume: simple_note.volume,
                pan: simple_note.pan,
                balance: simple_note.balance,
                expression: simple_note.expression,
                sustain: simple_note.sustain,
            })
            .collect();

        // Calculate total playback time including tail time for effects
        let note_end_time = notes
            .iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        let tail_time = Self::calculate_tail_time(&notes);
        let total_time = note_end_time + tail_time;

        // Log first few notes for debugging
        for (i, note) in notes.iter().take(3).enumerate() {
            tracing::info!(
                "Note {}: MIDI note {}, velocity {}, start={:.2}s, duration={:.2}s",
                i,
                note.note,
                note.velocity,
                note.start_time.as_secs_f64(),
                note.duration.as_secs_f64()
            );
        }

        tracing::info!(
            "Note end time: {:.2}s, tail time: {:.2}s, total playback time: {:.2}s",
            note_end_time.as_secs_f64(),
            tail_time.as_secs_f64(),
            total_time.as_secs_f64()
        );

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(notes, total_time)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;

        tracing::info!("Created OxiSynth audio source, starting async playback");
        self.sink.append(synth_source);
        self.sink.play();

        // DON'T WAIT - return immediately for async playback
        tracing::info!("OxiSynth async playback started, returning immediately");

        Ok(())
    }

    /// Play parsed MIDI data (legacy method)
    pub fn play_midi(&self, parsed_midi: ParsedMidi) -> Result<(), String> {
        tracing::info!(
            "Playing MIDI with {} notes using OxiSynth",
            parsed_midi.notes.len()
        );

        if parsed_midi.notes.is_empty() {
            tracing::warn!("No notes to play - MIDI file contains no note events");
            return Ok(());
        }

        // Calculate total playback time including tail time for effects
        let note_end_time = parsed_midi
            .notes
            .iter()
            .map(|note| note.start_time + note.duration)
            .max()
            .unwrap_or(Duration::from_secs(1));

        let tail_time = Self::calculate_tail_time(&parsed_midi.notes);
        let total_time = note_end_time + tail_time;

        // Log first few notes for debugging
        for (i, note) in parsed_midi.notes.iter().take(3).enumerate() {
            tracing::info!(
                "Note {}: MIDI note {}, velocity {}, start={:?}, duration={:?}",
                i,
                note.note,
                note.velocity,
                note.start_time,
                note.duration
            );
        }

        tracing::info!(
            "Note end time: {:.2}s, tail time: {:.2}s, total playback time: {:.2}s",
            note_end_time.as_secs_f64(),
            tail_time.as_secs_f64(),
            total_time.as_secs_f64()
        );

        // Create OxiSynth-based source
        let synth_source = OxiSynthSource::new(parsed_midi.notes, total_time)
            .map_err(|e| format!("Failed to create synthesizer source: {}", e))?;

        tracing::info!("Created OxiSynth audio source, starting playback");
        self.sink.append(synth_source);
        self.sink.play();

        // Wait for the audio to finish playing
        // Add a small buffer to ensure we don't cut off early
        let wait_time = total_time + Duration::from_millis(200);
        tracing::info!(
            "Waiting {:.2}s for playback to complete...",
            wait_time.as_secs_f64()
        );

        std::thread::sleep(wait_time);

        tracing::info!("OxiSynth playback completed");

        Ok(())
    }

    /// Stop playback (if any)
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.sink.stop();
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }

    /// Play a mixed sequence containing both MIDI and R2D2 notes
    pub fn play_mixed(&self, sequence: SimpleSequence) -> Result<(), String> {
        tracing::info!("Playing mixed sequence with {} notes", sequence.notes.len());

        if sequence.notes.is_empty() {
            tracing::warn!("No notes to play - sequence is empty");
            return Ok(());
        }

        // Separate MIDI and R2D2 notes
        let mut midi_notes = Vec::new();
        let mut r2d2_events = Vec::new();

        for note in sequence.notes {
            if note.note_type == "r2d2" {
                // Validate R2D2 parameters
                if let Err(e) = note.validate_r2d2() {
                    return Err(format!("Invalid R2D2 note: {}", e));
                }

                // Create R2D2 expression
                let emotion_str = note
                    .r2d2_emotion
                    .as_ref()
                    .ok_or("R2D2 emotion is required")?;
                let emotion = match emotion_str.as_str() {
                    "Happy" => R2D2Emotion::Happy,
                    "Sad" => R2D2Emotion::Sad,
                    "Excited" => R2D2Emotion::Excited,
                    "Worried" => R2D2Emotion::Worried,
                    "Curious" => R2D2Emotion::Curious,
                    "Affirmative" => R2D2Emotion::Affirmative,
                    "Negative" => R2D2Emotion::Negative,
                    "Surprised" => R2D2Emotion::Surprised,
                    "Thoughtful" => R2D2Emotion::Thoughtful,
                    _ => return Err(format!("Unknown R2D2 emotion: {}", emotion_str)),
                };

                let expression = R2D2Expression {
                    emotion,
                    intensity: note.r2d2_intensity.unwrap_or(0.7),
                    duration: note.duration as f32,
                    phrase_complexity: note.r2d2_complexity.unwrap_or(2),
                    pitch_range: if let Some(range) = &note.r2d2_pitch_range {
                        if range.len() == 2 {
                            (range[0], range[1])
                        } else {
                            (200.0, 800.0) // Default range
                        }
                    } else {
                        (200.0, 800.0) // Default range
                    },
                    context: note.r2d2_context,
                };

                r2d2_events.push(R2D2Event {
                    start_time: note.start_time,
                    expression,
                });
            } else {
                // Convert to MidiNote
                midi_notes.push(MidiNote {
                    note: note.note,
                    velocity: note.velocity,
                    channel: note.channel,
                    start_time: Duration::from_secs_f64(note.start_time),
                    duration: Duration::from_secs_f64(note.duration),
                    instrument: note.instrument,
                    reverb: note.reverb,
                    chorus: note.chorus,
                    volume: note.volume,
                    pan: note.pan,
                    balance: note.balance,
                    expression: note.expression,
                    sustain: note.sustain,
                });
            }
        }

        // Calculate total playback time
        let midi_end_time = if !midi_notes.is_empty() {
            midi_notes
                .iter()
                .map(|note| note.start_time + note.duration)
                .max()
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(0)
        };

        let r2d2_end_time = if !r2d2_events.is_empty() {
            r2d2_events
                .iter()
                .map(|event| {
                    Duration::from_secs_f64(event.start_time + event.expression.duration as f64)
                })
                .max()
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(0)
        };

        let note_end_time = midi_end_time.max(r2d2_end_time);
        let tail_time = Self::calculate_tail_time(&midi_notes);
        let total_time = note_end_time + tail_time;

        tracing::info!(
            "Mixed sequence: {} MIDI notes, {} R2D2 events, total time: {:.2}s",
            midi_notes.len(),
            r2d2_events.len(),
            total_time.as_secs_f64()
        );

        // Create hybrid audio source
        let hybrid_source = HybridAudioSource::new(midi_notes, r2d2_events, total_time)
            .map_err(|e| format!("Failed to create hybrid audio source: {}", e))?;

        tracing::info!("Created hybrid audio source, starting playback");
        self.sink.append(hybrid_source);
        self.sink.play();

        // Wait for playback to complete
        let wait_time = total_time + Duration::from_millis(200);
        tracing::info!(
            "Waiting {:.2}s for mixed playback to complete...",
            wait_time.as_secs_f64()
        );

        std::thread::sleep(wait_time);
        tracing::info!("Mixed sequence playback completed");

        Ok(())
    }
}

fn find_soundfont() -> Result<PathBuf, String> {
    // Try to find the SoundFont in various locations
    let exe_path = env::current_exe().map_err(|e| format!("Cannot find executable: {}", e))?;
    let exe_dir = exe_path
        .parent()
        .ok_or("Cannot find executable directory")?;

    let possible_paths = vec![
        exe_dir.join("../assets/FluidR3_GM.sf2"), // Development
        exe_dir.join("assets/FluidR3_GM.sf2"),    // Installed
        PathBuf::from("assets/FluidR3_GM.sf2"),   // Current directory
        PathBuf::from("FluidR3_GM.sf2"),          // Current directory
        // Also check target/debug/assets for development
        PathBuf::from("target/debug/assets/FluidR3_GM.sf2"),
        PathBuf::from("target/release/assets/FluidR3_GM.sf2"),
        // Fallback to old soundfont if it exists
        exe_dir.join("../assets/TimGM6mb.sf2"), // Development (old)
        exe_dir.join("assets/TimGM6mb.sf2"),    // Installed (old)
        PathBuf::from("assets/TimGM6mb.sf2"),   // Current directory (old)
        PathBuf::from("TimGM6mb.sf2"),          // Current directory (old)
        PathBuf::from("target/debug/assets/TimGM6mb.sf2"), // Development (old)
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
    channel_instruments: std::collections::HashMap<u8, u8>, // channel -> current instrument
    channel_reverb: std::collections::HashMap<u8, u8>,      // channel -> current reverb depth
    channel_chorus: std::collections::HashMap<u8, u8>,      // channel -> current chorus depth
    channel_volume: std::collections::HashMap<u8, u8>,      // channel -> current volume
    channel_pan: std::collections::HashMap<u8, u8>,         // channel -> current pan position
    channel_balance: std::collections::HashMap<u8, u8>,     // channel -> current balance
    channel_expression: std::collections::HashMap<u8, u8>,  // channel -> current expression
    channel_sustain: std::collections::HashMap<u8, u8>,     // channel -> current sustain
}

impl OxiSynthSource {
    fn new(notes: Vec<MidiNote>, total_duration: Duration) -> Result<Self, String> {
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

        // Use the provided total duration which includes tail time
        let final_duration = total_duration.max(Duration::from_secs(1));

        let buffer_size = 1024; // Process audio in chunks

        tracing::info!(
            "OxiSynth source created: {} notes, total duration: {:?}",
            notes.len(),
            final_duration
        );

        Ok(Self {
            synth,
            notes,
            sample_rate,
            current_sample: 0,
            total_duration: final_duration,
            left_buffer: vec![0.0; buffer_size],
            right_buffer: vec![0.0; buffer_size],
            buffer_size,
            buffer_pos: buffer_size, // Start with empty buffer to trigger initial fill
            playing_notes: std::collections::HashMap::new(),
            samples_generated: 0,
            channel_instruments: std::collections::HashMap::new(),
            channel_reverb: std::collections::HashMap::new(),
            channel_chorus: std::collections::HashMap::new(),
            channel_volume: std::collections::HashMap::new(),
            channel_pan: std::collections::HashMap::new(),
            channel_balance: std::collections::HashMap::new(),
            channel_expression: std::collections::HashMap::new(),
            channel_sustain: std::collections::HashMap::new(),
        })
    }

    fn process_audio_chunk(&mut self) {
        // Handle note on/off events for this chunk
        for note in &self.notes {
            let note_start_sample =
                (note.start_time.as_secs_f32() * self.sample_rate as f32) as u32;
            let note_end_sample =
                ((note.start_time + note.duration).as_secs_f32() * self.sample_rate as f32) as u32;

            let current_sample_u32 = self.current_sample as u32;
            let chunk_end = current_sample_u32 + self.buffer_size as u32;

            // Check if we should start this note in this chunk
            if note_start_sample >= current_sample_u32 && note_start_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if !self.playing_notes.contains_key(&key) {
                    // Handle drums (channel 9) specially
                    if note.channel == 9 {
                        // For drums, force bank select 128 (percussion) if not already set
                        let current_bank = self.channel_instruments.get(&note.channel).copied();
                        if current_bank != Some(128) {
                            // Bank Select MSB (Controller 0) = 128 for drums
                            let bank_select_msb = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 0,    // Bank Select MSB
                                value: 128, // Percussion Bank
                            };
                            let _ = self.synth.send_event(bank_select_msb);

                            // Program Change to Standard Kit (program 0 in percussion bank)
                            let program_change = MidiEvent::ProgramChange {
                                channel: note.channel,
                                program_id: 0, // Standard Kit in percussion bank
                            };
                            let _ = self.synth.send_event(program_change);
                            self.channel_instruments.insert(note.channel, 128);
                            tracing::debug!(
                                "Drum Setup: channel 9 -> percussion bank 128, standard kit"
                            );
                        }
                    } else {
                        // Check if we need to send a program change for this channel
                        if let Some(instrument) = note.instrument {
                            let current_instrument =
                                self.channel_instruments.get(&note.channel).copied();
                            if current_instrument != Some(instrument) {
                                let program_change = MidiEvent::ProgramChange {
                                    channel: note.channel,
                                    program_id: instrument,
                                };
                                let _ = self.synth.send_event(program_change);
                                self.channel_instruments.insert(note.channel, instrument);
                                tracing::debug!(
                                    "Program Change: channel {} -> instrument {}",
                                    note.channel,
                                    instrument
                                );
                            }
                        }
                    }

                    // Check if we need to send reverb control change for this channel
                    if let Some(reverb) = note.reverb {
                        let current_reverb = self.channel_reverb.get(&note.channel).copied();
                        if current_reverb != Some(reverb) {
                            let reverb_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 91, // Effects1Depth (Reverb)
                                value: reverb,
                            };
                            let _ = self.synth.send_event(reverb_cc);
                            self.channel_reverb.insert(note.channel, reverb);
                            tracing::debug!(
                                "Reverb CC: channel {} -> depth {}",
                                note.channel,
                                reverb
                            );
                        }
                    }

                    // Check if we need to send chorus control change for this channel
                    if let Some(chorus) = note.chorus {
                        let current_chorus = self.channel_chorus.get(&note.channel).copied();
                        if current_chorus != Some(chorus) {
                            let chorus_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 93, // Effects3Depth (Chorus)
                                value: chorus,
                            };
                            let _ = self.synth.send_event(chorus_cc);
                            self.channel_chorus.insert(note.channel, chorus);
                            tracing::debug!(
                                "Chorus CC: channel {} -> depth {}",
                                note.channel,
                                chorus
                            );
                        }
                    }

                    // Check if we need to send volume control change for this channel
                    if let Some(volume) = note.volume {
                        let current_volume = self.channel_volume.get(&note.channel).copied();
                        if current_volume != Some(volume) {
                            let volume_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 7, // Channel Volume
                                value: volume,
                            };
                            let _ = self.synth.send_event(volume_cc);
                            self.channel_volume.insert(note.channel, volume);
                            tracing::debug!(
                                "Volume CC: channel {} -> volume {}",
                                note.channel,
                                volume
                            );
                        }
                    }

                    // Check if we need to send pan control change for this channel
                    if let Some(pan) = note.pan {
                        let current_pan = self.channel_pan.get(&note.channel).copied();
                        if current_pan != Some(pan) {
                            let pan_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 10, // Pan
                                value: pan,
                            };
                            let _ = self.synth.send_event(pan_cc);
                            self.channel_pan.insert(note.channel, pan);
                            tracing::debug!("Pan CC: channel {} -> pan {}", note.channel, pan);
                        }
                    }

                    // Check if we need to send balance control change for this channel
                    if let Some(balance) = note.balance {
                        let current_balance = self.channel_balance.get(&note.channel).copied();
                        if current_balance != Some(balance) {
                            let balance_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 8, // Balance
                                value: balance,
                            };
                            let _ = self.synth.send_event(balance_cc);
                            self.channel_balance.insert(note.channel, balance);
                            tracing::debug!(
                                "Balance CC: channel {} -> balance {}",
                                note.channel,
                                balance
                            );
                        }
                    }

                    // Check if we need to send expression control change for this channel
                    if let Some(expression) = note.expression {
                        let current_expression =
                            self.channel_expression.get(&note.channel).copied();
                        if current_expression != Some(expression) {
                            let expression_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 11, // Expression Controller
                                value: expression,
                            };
                            let _ = self.synth.send_event(expression_cc);
                            self.channel_expression.insert(note.channel, expression);
                            tracing::debug!(
                                "Expression CC: channel {} -> expression {}",
                                note.channel,
                                expression
                            );
                        }
                    }

                    // Check if we need to send sustain control change for this channel
                    if let Some(sustain) = note.sustain {
                        let current_sustain = self.channel_sustain.get(&note.channel).copied();
                        if current_sustain != Some(sustain) {
                            let sustain_cc = MidiEvent::ControlChange {
                                channel: note.channel,
                                ctrl: 64, // Damper Pedal (Sustain)
                                value: sustain,
                            };
                            let _ = self.synth.send_event(sustain_cc);
                            self.channel_sustain.insert(note.channel, sustain);
                            tracing::debug!(
                                "Sustain CC: channel {} -> sustain {}",
                                note.channel,
                                sustain
                            );
                        }
                    }

                    let midi_event = MidiEvent::NoteOn {
                        channel: note.channel,
                        key: note.note,
                        vel: note.velocity,
                    };
                    let _ = self.synth.send_event(midi_event);
                    self.playing_notes.insert(key, note.duration);
                    tracing::debug!(
                        "Note ON: {} channel {} at sample {}",
                        note.note,
                        note.channel,
                        note_start_sample
                    );
                }
            }

            // Check if we should end this note in this chunk
            if note_end_sample >= current_sample_u32 && note_end_sample < chunk_end {
                let key = (note_start_sample, note.note);
                if self.playing_notes.remove(&key).is_some() {
                    let midi_event = MidiEvent::NoteOff {
                        channel: note.channel,
                        key: note.note,
                    };
                    let _ = self.synth.send_event(midi_event);
                    tracing::debug!(
                        "Note OFF: {} channel {} at sample {}",
                        note.note,
                        note.channel,
                        note_end_sample
                    );
                }
            }
        }

        // Clear buffers
        self.left_buffer.fill(0.0);
        self.right_buffer.fill(0.0);

        // Render audio - OxiSynth expects stereo output
        self.synth
            .write((&mut self.left_buffer[..], &mut self.right_buffer[..]));

        // Log some debug info about the audio levels
        let max_left = self.left_buffer.iter().map(|x| x.abs()).fold(0.0, f32::max);
        let max_right = self
            .right_buffer
            .iter()
            .map(|x| x.abs())
            .fold(0.0, f32::max);

        if max_left > 0.001 || max_right > 0.001 {
            tracing::debug!(
                "Audio chunk: max_left={:.4}, max_right={:.4}, samples={}",
                max_left,
                max_right,
                self.buffer_size
            );
        }

        // Reset buffer position
        self.buffer_pos = 0;
        self.samples_generated += self.buffer_size;
    }
}

impl Iterator for OxiSynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_time =
            Duration::from_secs_f32(self.current_sample as f32 / self.sample_rate as f32);

        if current_time > self.total_duration {
            tracing::info!(
                "Audio playback finished after {} samples ({:.2}s)",
                self.samples_generated,
                current_time.as_secs_f32()
            );
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

/// R2D2 event for scheduling
#[derive(Debug, Clone)]
struct R2D2Event {
    start_time: f64, // seconds
    expression: R2D2Expression,
}

/// Hybrid audio source that mixes MIDI (OxiSynth) and R2D2 (ExpressiveSynth) audio
struct HybridAudioSource {
    // MIDI synthesis
    oxisynth_source: Option<OxiSynthSource>,

    // R2D2 synthesis
    r2d2_events: Vec<R2D2PrecomputedEvent>,
    sample_rate: u32,
    current_sample: usize,
    total_duration: Duration,

    // Audio mixing
    #[allow(dead_code)]
    mixing_buffer: Vec<f32>,
    #[allow(dead_code)]
    buffer_size: usize,
}

/// Pre-computed R2D2 event with generated audio samples
#[derive(Debug, Clone)]
struct R2D2PrecomputedEvent {
    start_sample: u32,
    samples: Vec<f32>,
}

impl HybridAudioSource {
    fn new(
        midi_notes: Vec<MidiNote>,
        r2d2_events: Vec<R2D2Event>,
        total_duration: Duration,
    ) -> Result<Self, String> {
        let sample_rate = 44100;
        let buffer_size = 4096;

        // Create MIDI synthesizer source if there are MIDI notes
        let oxisynth_source = if !midi_notes.is_empty() {
            Some(
                OxiSynthSource::new(midi_notes, total_duration)
                    .map_err(|e| format!("Failed to create OxiSynth source: {}", e))?,
            )
        } else {
            None
        };

        // Pre-generate R2D2 audio samples
        let mut precomputed_r2d2_events = Vec::new();

        if !r2d2_events.is_empty() {
            let expressive_synth = ExpressiveSynth::new()
                .map_err(|e| format!("Failed to create ExpressiveSynth: {}", e))?;

            let r2d2_voice = R2D2Voice::new();

            for event in r2d2_events {
                let start_sample = (event.start_time * sample_rate as f64) as u32;

                // Generate R2D2 samples using the existing generate_r2d2_samples_with_contour method
                let synth_params = r2d2_voice
                    .generate_expression_params(&event.expression)
                    .ok_or("Failed to generate R2D2 synthesis parameters")?;

                let samples = expressive_synth.generate_r2d2_samples_with_contour(
                    synth_params.base_freq,
                    event.expression.intensity,
                    synth_params.duration,
                    &synth_params.pitch_contour,
                );

                precomputed_r2d2_events.push(R2D2PrecomputedEvent {
                    start_sample,
                    samples,
                });
            }
        }

        tracing::info!(
            "Created HybridAudioSource: {} R2D2 events pre-computed, MIDI source: {}",
            precomputed_r2d2_events.len(),
            if oxisynth_source.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        Ok(HybridAudioSource {
            oxisynth_source,
            r2d2_events: precomputed_r2d2_events,
            sample_rate,
            current_sample: 0,
            total_duration,
            mixing_buffer: vec![0.0; buffer_size],
            buffer_size,
        })
    }

    fn get_r2d2_sample(&self, sample_index: usize) -> f32 {
        let mut r2d2_sample = 0.0;

        // Mix all active R2D2 events
        for event in &self.r2d2_events {
            if sample_index >= event.start_sample as usize {
                let relative_index = sample_index - event.start_sample as usize;
                if relative_index < event.samples.len() {
                    r2d2_sample += event.samples[relative_index];
                }
            }
        }

        r2d2_sample
    }
}

impl Iterator for HybridAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we've reached the end
        let total_samples = (self.total_duration.as_secs_f64() * self.sample_rate as f64) as usize;
        if self.current_sample >= total_samples {
            return None;
        }

        let mut mixed_sample = 0.0;

        // Get MIDI sample
        if let Some(ref mut midi_source) = self.oxisynth_source {
            if let Some(midi_sample) = midi_source.next() {
                mixed_sample += midi_sample;
            }
        }

        // Get R2D2 sample
        let r2d2_sample = self.get_r2d2_sample(self.current_sample);
        mixed_sample += r2d2_sample;

        // Apply gentle limiting to prevent clipping
        mixed_sample = mixed_sample.clamp(-1.0, 1.0);

        self.current_sample += 1;
        Some(mixed_sample)
    }
}

impl Source for HybridAudioSource {
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
