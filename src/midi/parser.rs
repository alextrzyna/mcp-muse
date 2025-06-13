use midly::{MidiMessage, Smf, TrackEventKind};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MidiNote {
    pub note: u8,
    pub velocity: u8,
    pub channel: u8,
    pub start_time: Duration,
    pub duration: Duration,
    pub instrument: Option<u8>,
    pub reverb: Option<u8>,
    pub chorus: Option<u8>,
    pub volume: Option<u8>,
    pub pan: Option<u8>,
    pub balance: Option<u8>,
    pub expression: Option<u8>,
    pub sustain: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub notes: Vec<MidiNote>,
    #[allow(dead_code)]
    pub tempo: u32, // microseconds per quarter note
    #[allow(dead_code)]
    pub ticks_per_quarter: u16,
}

pub fn parse_midi_data(midi_bytes: &[u8]) -> Result<ParsedMidi, String> {
    tracing::info!("Parsing MIDI data ({} bytes)", midi_bytes.len());

    let smf = Smf::parse(midi_bytes).map_err(|e| format!("Failed to parse MIDI: {}", e))?;

    tracing::info!(
        "MIDI header parsed: format={:?}, tracks={}, timing={:?}",
        smf.header.format,
        smf.tracks.len(),
        smf.header.timing
    );

    let ticks_per_quarter = match smf.header.timing {
        midly::Timing::Metrical(tpq) => tpq.as_int(),
        midly::Timing::Timecode(_, _) => {
            return Err("Timecode timing not supported".to_string());
        }
    };

    let mut notes = Vec::new();
    let mut tempo = 500_000; // Default tempo (120 BPM)
    let mut note_on_events = std::collections::HashMap::new();

    // Process all tracks
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        tracing::info!("Processing track {} with {} events", track_idx, track.len());
        let mut current_time = 0u32;
        for event in track {
            current_time += event.delta.as_int();

            match event.kind {
                TrackEventKind::Meta(midly::MetaMessage::Tempo(new_tempo)) => {
                    tempo = new_tempo.as_int();
                }
                TrackEventKind::Meta(_) => {} // Ignore other meta messages
                TrackEventKind::Midi { channel, message } => {
                    match message {
                        MidiMessage::NoteOn { key, vel } => {
                            tracing::debug!(
                                "Note On: channel={}, key={}, velocity={}, time={}",
                                channel.as_int(),
                                key.as_int(),
                                vel.as_int(),
                                current_time
                            );
                            if vel.as_int() > 0 {
                                // Store note on event
                                note_on_events.insert(
                                    (channel.as_int(), key.as_int()),
                                    (current_time, vel.as_int()),
                                );
                            } else {
                                // Note on with velocity 0 is note off
                                if let Some((start_time, velocity)) =
                                    note_on_events.remove(&(channel.as_int(), key.as_int()))
                                {
                                    let duration = ticks_to_duration(
                                        current_time - start_time,
                                        ticks_per_quarter,
                                        tempo,
                                    );
                                    let start_duration =
                                        ticks_to_duration(start_time, ticks_per_quarter, tempo);

                                    notes.push(MidiNote {
                                        note: key.as_int(),
                                        velocity,
                                        channel: channel.as_int(),
                                        start_time: start_duration,
                                        duration,
                                        instrument: None,
                                        reverb: None,
                                        chorus: None,
                                        volume: None,
                                        pan: None,
                                        balance: None,
                                        expression: None,
                                        sustain: None,
                                    });
                                }
                            }
                        }
                        MidiMessage::NoteOff { key, vel: _ } => {
                            tracing::debug!(
                                "Note Off: channel={}, key={}, time={}",
                                channel.as_int(),
                                key.as_int(),
                                current_time
                            );
                            if let Some((start_time, velocity)) =
                                note_on_events.remove(&(channel.as_int(), key.as_int()))
                            {
                                let duration = ticks_to_duration(
                                    current_time - start_time,
                                    ticks_per_quarter,
                                    tempo,
                                );
                                let start_duration =
                                    ticks_to_duration(start_time, ticks_per_quarter, tempo);

                                notes.push(MidiNote {
                                    note: key.as_int(),
                                    velocity,
                                    channel: channel.as_int(),
                                    start_time: start_duration,
                                    duration,
                                    instrument: None,
                                    reverb: None,
                                    chorus: None,
                                    volume: None,
                                    pan: None,
                                    balance: None,
                                    expression: None,
                                    sustain: None,
                                });
                            }
                        }
                        _ => {} // Ignore other MIDI messages for now
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    }

    // Sort notes by start time
    notes.sort_by(|a, b| a.start_time.cmp(&b.start_time));

    tracing::info!("MIDI parsing complete: {} notes found", notes.len());
    for (i, note) in notes.iter().take(5).enumerate() {
        tracing::debug!(
            "Note {}: key={}, vel={}, start={:?}, duration={:?}",
            i,
            note.note,
            note.velocity,
            note.start_time,
            note.duration
        );
    }

    Ok(ParsedMidi {
        notes,
        tempo,
        ticks_per_quarter,
    })
}

fn ticks_to_duration(ticks: u32, ticks_per_quarter: u16, tempo: u32) -> Duration {
    let microseconds_per_tick = (tempo as f64) / (ticks_per_quarter as f64);
    let total_microseconds = (ticks as f64) * microseconds_per_tick;
    Duration::from_micros(total_microseconds as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_midi_bytes() -> Vec<u8> {
        // Create a simple MIDI file with a few notes
        let mut bytes = Vec::new();

        // MIDI header
        bytes.extend_from_slice(&[
            0x4D, 0x54, 0x68, 0x64, // "MThd"
            0x00, 0x00, 0x00, 0x06, // Header length (6 bytes)
            0x00, 0x00, // Format type 0
            0x00, 0x01, // Number of tracks (1)
            0x00, 0x60, // Ticks per quarter note (96)
        ]);

        // Track header
        let track_events = [
            // Note On C4 (middle C)
            0x00, 0x90, 0x3C, 0x64, // Delta=0, Note On Ch0, Note=60 (C4), Vel=100
            0x60, 0x80, 0x3C, 0x00, // Delta=96, Note Off Ch0, Note=60, Vel=0
            // Note On E4
            0x00, 0x90, 0x40, 0x64, // Delta=0, Note On Ch0, Note=64 (E4), Vel=100
            0x60, 0x80, 0x40, 0x00, // Delta=96, Note Off Ch0, Note=64, Vel=0
            // End of track
            0x00, 0xFF, 0x2F, 0x00, // Delta=0, Meta Event, End of Track, Length=0
        ];

        bytes.extend_from_slice(&[
            0x4D, 0x54, 0x72, 0x6B, // "MTrk"
            0x00, 0x00, 0x00, 0x18, // Track length (24 bytes)
        ]);
        bytes.extend_from_slice(&track_events);

        bytes
    }

    #[test]
    fn test_parse_simple_midi() {
        let midi_bytes = create_simple_midi_bytes();
        let result = parse_midi_data(&midi_bytes);

        assert!(result.is_ok(), "MIDI parsing should succeed");

        let parsed = result.unwrap();
        assert_eq!(parsed.notes.len(), 2, "Should parse 2 notes");
        assert_eq!(
            parsed.ticks_per_quarter, 96,
            "Should have correct ticks per quarter"
        );
        assert_eq!(parsed.tempo, 500_000, "Should have default tempo");

        // Check first note (C4)
        let note1 = &parsed.notes[0];
        assert_eq!(note1.note, 60, "First note should be C4 (60)");
        assert_eq!(note1.velocity, 100, "First note should have velocity 100");
        assert_eq!(note1.channel, 0, "First note should be on channel 0");

        // Check second note (E4)
        let note2 = &parsed.notes[1];
        assert_eq!(note2.note, 64, "Second note should be E4 (64)");
        assert_eq!(note2.velocity, 100, "Second note should have velocity 100");
    }

    #[test]
    fn test_parse_invalid_midi() {
        let invalid_bytes = vec![0x00, 0x01, 0x02, 0x03];
        let result = parse_midi_data(&invalid_bytes);

        assert!(result.is_err(), "Should fail to parse invalid MIDI data");
    }

    #[test]
    fn test_ticks_to_duration() {
        let duration = ticks_to_duration(96, 96, 500_000);
        // At 96 ticks per quarter and 500,000 microseconds per quarter,
        // 96 ticks should equal 500,000 microseconds = 0.5 seconds
        assert_eq!(duration, Duration::from_micros(500_000));
    }

    #[test]
    fn test_empty_midi() {
        // Create minimal MIDI file with no notes
        let mut bytes = Vec::new();

        // MIDI header
        bytes.extend_from_slice(&[
            0x4D, 0x54, 0x68, 0x64, // "MThd"
            0x00, 0x00, 0x00, 0x06, // Header length (6 bytes)
            0x00, 0x00, // Format type 0
            0x00, 0x01, // Number of tracks (1)
            0x00, 0x60, // Ticks per quarter note (96)
        ]);

        // Empty track
        bytes.extend_from_slice(&[
            0x4D, 0x54, 0x72, 0x6B, // "MTrk"
            0x00, 0x00, 0x00, 0x04, // Track length (4 bytes)
            0x00, 0xFF, 0x2F, 0x00, // End of track
        ]);

        let result = parse_midi_data(&bytes);
        assert!(result.is_ok(), "Should parse empty MIDI file");

        let parsed = result.unwrap();
        assert_eq!(parsed.notes.len(), 0, "Should have no notes");
    }
}
