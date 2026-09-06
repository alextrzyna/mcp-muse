//! Turns a `SimpleSequence` into a `PlayCommand`: presets, musical time,
//! pre-rendered R2D2/synthesis buffers and a time-ordered MIDI event list.
#![allow(dead_code)]

use crate::midi::engine::{EventKind, seconds_to_frames};
use crate::midi::parser::MidiNote;
use std::collections::HashMap;

/// GM controller numbers the note schema exposes, in the order they are sent.
#[allow(clippy::type_complexity)]
const CONTROLLERS: [(u8, fn(&MidiNote) -> Option<u8>); 7] = [
    (91, |n| n.reverb),
    (93, |n| n.chorus),
    (7, |n| n.volume),
    (10, |n| n.pan),
    (8, |n| n.balance),
    (11, |n| n.expression),
    (64, |n| n.sustain),
];

/// Time-ordered MIDI events for one call. Per channel, the first note sends a
/// program change (its instrument, or 0 so an unspecified instrument is still
/// piano when layering); controllers are sent only when a note specifies a
/// value that differs from what this call last sent. Channel 9 needs no bank
/// select: OxiSynth treats it as the drum channel and resolves bank 128.
pub(crate) fn midi_events(notes: &[MidiNote]) -> Vec<(u64, EventKind)> {
    let mut sorted: Vec<&MidiNote> = notes.iter().collect();
    sorted.sort_by_key(|n| n.start_time);

    let mut program: HashMap<u8, u8> = HashMap::new();
    let mut controller: HashMap<(u8, u8), u8> = HashMap::new();
    let mut events = Vec::with_capacity(notes.len() * 3);

    for note in sorted {
        let at = seconds_to_frames(note.start_time);
        let channel = note.channel;
        let wanted = match note.instrument {
            Some(p) => Some(p),
            None if !program.contains_key(&channel) => Some(0),
            None => None,
        };
        if let Some(p) = wanted
            && program.get(&channel) != Some(&p)
        {
            events.push((
                at,
                EventKind::ProgramChange {
                    channel,
                    program: p,
                },
            ));
            program.insert(channel, p);
        }
        for (number, get) in CONTROLLERS {
            if let Some(value) = get(note)
                && controller.get(&(channel, number)) != Some(&value)
            {
                events.push((
                    at,
                    EventKind::ControlChange {
                        channel,
                        controller: number,
                        value,
                    },
                ));
                controller.insert((channel, number), value);
            }
        }
        events.push((
            at,
            EventKind::NoteOn {
                channel,
                key: note.note,
                velocity: note.velocity,
            },
        ));
        let off = at + seconds_to_frames(note.duration).max(1);
        events.push((
            off,
            EventKind::NoteOff {
                channel,
                key: note.note,
            },
        ));
    }

    // Stable: keeps setup-before-note-on and off-before-next-on at equal frames.
    events.sort_by_key(|(at, _)| *at);
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn note(start: f64, dur: f64, instrument: Option<u8>, reverb: Option<u8>) -> MidiNote {
        MidiNote {
            note: 60,
            velocity: 100,
            channel: 0,
            start_time: Duration::from_secs_f64(start),
            duration: Duration::from_secs_f64(dur),
            instrument,
            reverb,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
        }
    }

    #[test]
    fn setup_events_precede_note_on_and_are_deduplicated_per_channel() {
        let events = midi_events(&[
            note(0.25, 0.5, None, Some(40)),
            note(0.0, 0.5, Some(73), Some(40)),
        ]);
        assert_eq!(
            events,
            vec![
                (
                    0,
                    EventKind::ProgramChange {
                        channel: 0,
                        program: 73
                    }
                ),
                (
                    0,
                    EventKind::ControlChange {
                        channel: 0,
                        controller: 91,
                        value: 40
                    }
                ),
                (
                    0,
                    EventKind::NoteOn {
                        channel: 0,
                        key: 60,
                        velocity: 100
                    }
                ),
                (
                    11_025,
                    EventKind::NoteOn {
                        channel: 0,
                        key: 60,
                        velocity: 100
                    }
                ),
                (
                    22_050,
                    EventKind::NoteOff {
                        channel: 0,
                        key: 60
                    }
                ),
                (
                    33_075,
                    EventKind::NoteOff {
                        channel: 0,
                        key: 60
                    }
                ),
            ]
        );
    }

    #[test]
    fn an_unspecified_instrument_means_program_0_on_first_use_only() {
        let events = midi_events(&[note(0.0, 0.1, None, None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(
            programs,
            vec![&(
                0,
                EventKind::ProgramChange {
                    channel: 0,
                    program: 0
                }
            )]
        );

        let events = midi_events(&[note(0.0, 0.1, Some(48), None), note(0.5, 0.1, None, None)]);
        let programs: Vec<_> = events
            .iter()
            .filter(|(_, e)| matches!(e, EventKind::ProgramChange { .. }))
            .collect();
        assert_eq!(
            programs.len(),
            1,
            "a later note without instrument keeps the channel's program"
        );
    }

    #[test]
    fn a_zero_length_note_still_gets_its_note_off_after_note_on() {
        let events = midi_events(&[note(0.0, 0.0, Some(0), None)]);
        assert_eq!(
            events[1],
            (
                0,
                EventKind::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100
                }
            )
        );
        assert_eq!(
            events[2],
            (
                1,
                EventKind::NoteOff {
                    channel: 0,
                    key: 60
                }
            )
        );
    }
}
