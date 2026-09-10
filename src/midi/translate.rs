//! Turns a `SimpleSequence` into a `PlayCommand`: musical time, pre-rendered
//! R2D2/patch buffers and a time-ordered MIDI event list.

use crate::expressive::{
    EffectsChain, EffectsPresetLibrary, ExpressiveSynth, MAX_RENDER_SECONDS, NoteEvent, Patch,
    PatchLibrary, R2D2Emotion, R2D2Expression, R2D2Voice, SynthRef, render_length_seconds,
    render_patch,
};
use crate::midi::engine::{
    EventKind, PlayCommand, PlayMode, SAMPLE_RATE, SYNTH_BUS_GAIN, seconds_to_frames,
};
use crate::midi::parser::MidiNote;
use crate::midi::{EffectConfig, SimpleSequence, effects_tail_seconds};
use std::collections::HashMap;
use std::time::Duration;

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

/// One patch's group: its key, the resolved/inline patch, and every note
/// scheduled against it this call (absolute start time, event).
type PatchGroup = (String, Patch, Vec<(f64, NoteEvent)>);

/// Result of translating one call.
#[derive(Debug)]
pub struct Translation {
    pub command: PlayCommand,
    /// Time until the last note plus its effect tail; reported to the caller.
    pub duration: Duration,
}

/// Whether the patch, R2D2 and MIDI-bus effect chains are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effects {
    /// Every chain applied, as in playback.
    Wet,
    /// Every chain bypassed: dry material for mixing elsewhere.
    Dry,
}

/// One patch group's rendered stereo buffer, already at bus level.
#[derive(Debug, Clone)]
pub struct PatchRender {
    /// The patch's key (`Patch::key`), used to name an exported file.
    pub name: String,
    /// Frames after the start of the composition.
    pub start: u64,
    pub samples: Vec<[f32; 2]>,
}

/// A translated call with its sound sources kept apart, so an export can
/// write them separately. Playback flattens it with `into_command`.
#[derive(Debug)]
pub struct TranslatedParts {
    pub midi: Vec<MidiNote>,
    /// MIDI bus chain for this call, if any MIDI note specified one.
    pub midi_effects: Option<Vec<EffectConfig>>,
    pub patches: Vec<PatchRender>,
    /// One buffer per R2D2 note at its start frame.
    pub r2d2: Vec<(u64, Vec<[f32; 2]>)>,
    pub tempo: u32,
    /// Time until the last note plus its effect tail.
    pub duration: Duration,
}

impl TranslatedParts {
    /// Flatten into the engine's command: R2D2 buffers first, then patch
    /// buffers (the order playback has always used).
    pub fn into_command(self, mode: PlayMode) -> PlayCommand {
        let mut buffers = self.r2d2;
        buffers.extend(self.patches.into_iter().map(|p| (p.start, p.samples)));
        PlayCommand {
            events: midi_events(&self.midi),
            external: Vec::new(),
            buffers,
            midi_effects: self.midi_effects,
            mode,
            tempo: self.tempo,
        }
    }
}

pub struct Translator {
    effects_library: EffectsPresetLibrary,
    patch_library: PatchLibrary,
    /// `Err(reason)` when no SoundFont is loaded; MIDI notes then fail here.
    midi_available: Result<(), String>,
}

impl Translator {
    pub fn new(midi_available: Result<(), String>) -> Self {
        Self {
            effects_library: EffectsPresetLibrary::new(),
            patch_library: PatchLibrary::new(),
            midi_available,
        }
    }

    /// Session patches first (they may shadow built-ins), then the library, then inline.
    fn resolve_patch<'a>(
        &'a self,
        reference: &'a SynthRef,
        session: &'a HashMap<String, Patch>,
    ) -> Result<&'a Patch, String> {
        match reference {
            SynthRef::Inline(patch) => {
                patch.validate()?;
                Ok(patch)
            }
            SynthRef::Name(name) => {
                let key = name.trim().to_lowercase();
                if let Some(p) = session.get(&key) {
                    return Ok(p);
                }
                if let Some(p) = self.patch_library.get(&key) {
                    return Ok(p);
                }
                let mut defined: Vec<&str> = session.values().map(|p| p.name.as_str()).collect();
                defined.sort_unstable();
                Err(format!(
                    "Unknown synth '{}'. Defined this session: {:?}. Call list_sounds with section \"synths\" for the built-in patches, or define_synth to create one.",
                    name, defined
                ))
            }
        }
    }

    pub fn translate(
        &self,
        sequence: SimpleSequence,
        mode: PlayMode,
        session_patches: &HashMap<String, Patch>,
    ) -> Result<Translation, String> {
        let parts = self.translate_parts(sequence, session_patches, Effects::Wet)?;
        let duration = parts.duration;
        Ok(Translation {
            command: parts.into_command(mode),
            duration,
        })
    }

    /// Translate without flattening. `effects` selects wet (playback) or
    /// dry (bypassed chains) rendering of the patch and R2D2 buffers and
    /// decides whether the MIDI bus chain is kept.
    pub fn translate_parts(
        &self,
        sequence: SimpleSequence,
        session_patches: &HashMap<String, Patch>,
        effects: Effects,
    ) -> Result<TranslatedParts, String> {
        if sequence.notes.is_empty() {
            return Ok(TranslatedParts {
                midi: Vec::new(),
                midi_effects: None,
                patches: Vec::new(),
                r2d2: Vec::new(),
                tempo: sequence.tempo,
                duration: Duration::ZERO,
            });
        }

        // Effects validation, named effect chains and musical time -> seconds.
        let mut processed_notes = Vec::new();
        for mut note in sequence.notes {
            if let Err(e) = note.validate_effects() {
                tracing::warn!("Invalid effects on note: {}", e);
                note.effects = None;
                note.effects_preset = None;
            }
            self.apply_effects_preset(&mut note);
            if note.start_time.is_none()
                && let Some(musical_time) = &note.musical_time
            {
                note.start_time =
                    Some(musical_time.to_seconds(sequence.tempo, sequence.beats_per_bar, 480));
            }
            if note.duration.is_none()
                && let Some(ref musical_duration) = note.musical_duration
            {
                note.duration =
                    Some(musical_duration.to_seconds(sequence.tempo, sequence.beats_per_bar));
            }
            processed_notes.push(note);
        }

        // Synthesis and R2D2 notes render their own effects into their sample
        // buffers. MIDI comes out of OxiSynth as one mixed bus, so the first
        // MIDI note that specifies effects defines the chain for that bus.
        let midi_effects: Option<Vec<EffectConfig>> = match effects {
            Effects::Dry => None,
            Effects::Wet => processed_notes
                .iter()
                .filter(|n| n.note_type != "r2d2" && !n.is_synthesis())
                .find_map(|n| n.effects.clone().filter(|e| !e.is_empty())),
        };

        let mut midi_notes: Vec<MidiNote> = Vec::new();
        let mut r2d2_buffers: Vec<(u64, Vec<[f32; 2]>)> = Vec::new();
        let mut patches: Vec<PatchRender> = Vec::new();
        let mut note_end = Duration::ZERO;
        let expressive_synth = ExpressiveSynth::new();
        let r2d2_voice = R2D2Voice::new();

        // Notes referencing an agent-defined patch, grouped so every note of
        // one patch in this call renders into a single stereo buffer. `f64`
        // here is the note's absolute start time; insertion order is kept
        // with `patch_groups` alongside a lookup index for grouping.
        let mut patch_groups: Vec<PatchGroup> = Vec::new();
        let mut patch_group_index: HashMap<String, usize> = HashMap::new();

        for (i, note) in processed_notes.into_iter().enumerate() {
            // A negative start time would panic in `Duration`; treat it as 0.
            let start = Duration::from_secs_f64(note.start_time.unwrap_or(0.0).max(0.0));
            if let Some(reference) = note.synth.as_ref() {
                note.validate_synth()
                    .map_err(|e| format!("Note {}: {}", i + 1, e))?;
                let patch = self
                    .resolve_patch(reference, session_patches)
                    .map_err(|e| format!("Note {}: {}", i + 1, e))?;
                let abs_start = note.start_time.unwrap_or(0.0).max(0.0);
                let duration = note.duration.unwrap_or(1.0).max(0.0) as f32;
                let velocity = note.velocity.unwrap_or(100) as f32 / 127.0;
                let frequency = match note.note {
                    Some(n) => 440.0 * 2f32.powf((n as f32 - 69.0) / 12.0),
                    None if patch.has_pitched_engine() => {
                        return Err(format!(
                            "Note {}: synth '{}' is pitched, so it needs a MIDI note",
                            i + 1,
                            patch.name
                        ));
                    }
                    None => 0.0,
                };
                let event = NoteEvent {
                    start: 0.0, // corrected once the group's earliest note is known
                    duration,
                    frequency,
                    velocity,
                };
                let group_key = match reference {
                    SynthRef::Name(_) => patch.key(),
                    SynthRef::Inline(_) => serde_json::to_string(patch).map_err(|e| {
                        format!("Note {}: failed to key inline patch: {}", i + 1, e)
                    })?,
                };
                let idx = *patch_group_index
                    .entry(group_key.clone())
                    .or_insert_with(|| {
                        patch_groups.push((group_key, patch.clone(), Vec::new()));
                        patch_groups.len() - 1
                    });
                patch_groups[idx].2.push((abs_start, event));
            } else if note.note_type == "r2d2" {
                note.validate_r2d2()
                    .map_err(|e| format!("Invalid R2D2 note: {}", e))?;
                let emotion = parse_emotion(
                    note.r2d2_emotion
                        .as_deref()
                        .ok_or("R2D2 emotion is required")?,
                )?;
                let expression = R2D2Expression {
                    emotion,
                    intensity: note.r2d2_intensity.unwrap_or(0.7),
                    // A negative duration would panic in `Duration::from_secs_f32` below.
                    duration: note.duration.unwrap_or(1.0).max(0.0) as f32,
                    phrase_complexity: note.r2d2_complexity.unwrap_or(2),
                    pitch_range: match &note.r2d2_pitch_range {
                        Some(range) if range.len() == 2 => (range[0], range[1]),
                        _ => (200.0, 800.0),
                    },
                    context: note.r2d2_context,
                };
                let params = r2d2_voice
                    .generate_expression_params(&expression)
                    .ok_or("Failed to generate R2D2 synthesis parameters")?;
                let mut samples = expressive_synth.generate_r2d2_samples_with_contour(
                    params.base_freq,
                    expression.intensity,
                    params.duration,
                    &params.pitch_contour,
                );
                let note_effects: &[EffectConfig] = match effects {
                    Effects::Dry => &[],
                    Effects::Wet => note.effects.as_deref().unwrap_or(&[]),
                };
                let mut chain =
                    EffectsChain::with_tempo(SAMPLE_RATE as f32, sequence.tempo, note_effects);
                let mut tail = 0.0f32;
                if !chain.is_empty() {
                    tail = r2d2_tail_seconds(note_effects, sequence.tempo);
                    samples.resize(samples.len() + (tail * SAMPLE_RATE as f32) as usize, 0.0);
                    chain.process_buffer(&mut samples);
                }
                note_end = note_end.max(
                    start
                        + Duration::from_secs_f32(expression.duration)
                        + Duration::from_secs_f32(tail),
                );
                r2d2_buffers.push((
                    seconds_to_frames(start),
                    samples.into_iter().map(|s| [s, s]).collect(),
                ));
            } else if let Some(key) = note.note {
                let duration = Duration::from_secs_f64(note.duration.unwrap_or(1.0).max(0.0));
                note_end = note_end.max(start + duration);
                midi_notes.push(MidiNote {
                    note: key,
                    velocity: note.velocity.unwrap_or(80),
                    channel: note.channel,
                    start_time: start,
                    duration,
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

        // Render one stereo buffer per patch group, scheduled at the
        // group's earliest note.
        for (_, mut patch, timed_events) in patch_groups {
            if effects == Effects::Dry {
                patch.effects.clear();
            }
            let first = timed_events
                .iter()
                .map(|(abs, _)| *abs)
                .fold(f64::INFINITY, f64::min);
            let events: Vec<NoteEvent> = timed_events
                .iter()
                .map(|(abs, event)| NoteEvent {
                    start: (*abs - first) as f32,
                    ..*event
                })
                .collect();
            let mut samples = render_patch(&patch, &events, SAMPLE_RATE as f32, sequence.tempo);
            for frame in &mut samples {
                frame[0] *= SYNTH_BUS_GAIN;
                frame[1] *= SYNTH_BUS_GAIN;
            }
            patches.push(PatchRender {
                name: patch.key(),
                start: seconds_to_frames(Duration::from_secs_f64(first)),
                samples,
            });
            note_end = note_end.max(Duration::from_secs_f64(
                first + render_length_seconds(&patch, &events, sequence.tempo) as f64,
            ));
        }

        if !midi_notes.is_empty()
            && let Err(reason) = &self.midi_available
        {
            return Err(format!("MIDI notes need a SoundFont: {}", reason));
        }

        let duration = if midi_notes.is_empty() && r2d2_buffers.is_empty() && patches.is_empty() {
            Duration::ZERO
        } else {
            // The bus chain rings out too: a beat-synced delay outlasts the
            // CC-based estimate by far, and the caller sleeps this duration.
            let bus_tail = Duration::from_secs_f32(effects_tail_seconds(
                midi_effects.as_deref().unwrap_or(&[]),
                sequence.tempo,
            ));
            note_end + calculate_tail_time(&midi_notes).max(bus_tail)
        };
        tracing::info!(
            "Translated {} MIDI notes and {} buffers, {:.2}s including tail",
            midi_notes.len(),
            r2d2_buffers.len() + patches.len(),
            duration.as_secs_f64()
        );

        Ok(TranslatedParts {
            midi: midi_notes,
            midi_effects,
            patches,
            r2d2: r2d2_buffers,
            tempo: sequence.tempo,
            duration,
        })
    }

    /// Append a named chain (`effects_preset`) to whatever the note already asks for.
    fn apply_effects_preset(&self, note: &mut crate::midi::SimpleNote) {
        let Some(name) = note.effects_preset.clone() else {
            return;
        };
        match self.effects_library.get_preset(&name) {
            Some(effects) => {
                note.effects
                    .get_or_insert_with(Vec::new)
                    .extend(effects.clone());
                tracing::info!("Applied effects preset '{}' to note", name);
            }
            None => tracing::warn!("Effects preset '{}' not found", name),
        }
    }
}

fn parse_emotion(name: &str) -> Result<R2D2Emotion, String> {
    Ok(match name {
        "Happy" => R2D2Emotion::Happy,
        "Sad" => R2D2Emotion::Sad,
        "Excited" => R2D2Emotion::Excited,
        "Worried" => R2D2Emotion::Worried,
        "Curious" => R2D2Emotion::Curious,
        "Affirmative" => R2D2Emotion::Affirmative,
        "Negative" => R2D2Emotion::Negative,
        "Surprised" => R2D2Emotion::Surprised,
        "Thoughtful" => R2D2Emotion::Thoughtful,
        _ => return Err(format!("Unknown R2D2 emotion: {}", name)),
    })
}

/// Seconds of silence appended to an R2D2 note so its effects can ring out.
/// Capped like a patch render: `tempo` is bounded at the tool layer, but this
/// buffer is sized here, so it carries the same ceiling `render_patch` does.
fn r2d2_tail_seconds(effects: &[EffectConfig], tempo: u32) -> f32 {
    effects_tail_seconds(effects, tempo).min(MAX_RENDER_SECONDS)
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

    use crate::midi::engine::{PlayMode, SYNTH_BUS_GAIN};
    use crate::midi::{SimpleNote, SimpleSequence};
    use serde_json::json;

    fn seq(notes: Vec<SimpleNote>) -> SimpleSequence {
        SimpleSequence {
            notes,
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    fn no_session() -> HashMap<String, crate::expressive::Patch> {
        HashMap::new()
    }

    fn patch_note(synth: serde_json::Value, note: u8, start: f64, dur: f64) -> SimpleNote {
        SimpleNote {
            note: Some(note),
            velocity: Some(100),
            start_time: Some(start),
            duration: Some(dur),
            synth: Some(serde_json::from_value(synth).unwrap()),
            ..Default::default()
        }
    }

    #[test]
    fn the_sequence_tempo_reaches_patch_effects_and_the_midi_bus() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut s = seq(vec![patch_note(
            json!({"name": "d", "subtractive": {"env": {"release": 0.01}},
            "effects": [{"type": "delay", "random_beats": [1.0, 1.0], "intensity": 0.5}]}),
            60,
            0.0,
            0.1,
        )]);
        s.tempo = 60;
        let slow = t
            .translate(s.clone(), PlayMode::Replace, &no_session())
            .unwrap();
        s.tempo = 120;
        let fast = t.translate(s, PlayMode::Replace, &no_session()).unwrap();
        assert!(slow.duration > fast.duration, "longer beats, longer tail");
        assert_eq!(fast.command.tempo, 120);
        assert_eq!(slow.command.tempo, 60);
    }

    #[test]
    fn patch_notes_become_buffers_at_bus_level() {
        let t = Translator::new(Err("no soundfont".into()));
        let tr = t
            .translate(
                seq(vec![patch_note(
                    json!({"name": "s", "level": 1.0, "subtractive": {"osc1": {"wave": "sine"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}),
                    69,
                    0.0,
                    0.5,
                )]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let peak = tr.command.buffers[0]
            .1
            .iter()
            .fold(0.0f32, |m, s| m.max(s[0].abs()));
        assert!(peak <= SYNTH_BUS_GAIN * 100.0 / 127.0 + 0.01 && peak > SYNTH_BUS_GAIN * 0.5);
    }

    fn r2d2_note(duration: f64, effects: Option<Vec<EffectConfig>>) -> SimpleNote {
        SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(duration),
            effects,
            ..Default::default()
        }
    }

    fn wet_reverb() -> EffectConfig {
        serde_json::from_value(json!({
            "type": "reverb", "room_size": 0.8, "wet_level": 0.5, "intensity": 0.8
        }))
        .unwrap()
    }

    #[test]
    fn dry_parts_bypass_every_effect_chain() {
        let t = Translator::new(Ok(()));
        let reverb_patch = json!({"name": "wet", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "effects": [{"type": "reverb", "room_size": 0.8, "wet_level": 0.5, "intensity": 0.8}]});
        let notes = || {
            vec![
                patch_note(reverb_patch.clone(), 69, 0.0, 0.5),
                r2d2_note(0.5, Some(vec![wet_reverb()])),
                SimpleNote {
                    note: Some(60),
                    duration: Some(0.5),
                    effects: Some(vec![wet_reverb()]),
                    ..Default::default()
                },
            ]
        };
        let wet = t
            .translate_parts(seq(notes()), &no_session(), Effects::Wet)
            .unwrap();
        let dry = t
            .translate_parts(seq(notes()), &no_session(), Effects::Dry)
            .unwrap();

        assert!(wet.midi_effects.is_some(), "wet keeps the bus chain");
        assert!(dry.midi_effects.is_none(), "dry drops the bus chain");
        assert_eq!(wet.midi.len(), 1);
        assert_eq!(dry.midi.len(), 1);
        assert_eq!(wet.patches.len(), 1);
        assert_eq!(dry.patches.len(), 1);
        assert_eq!(dry.patches[0].name, "wet");
        assert!(
            dry.patches[0].samples.len() < wet.patches[0].samples.len(),
            "a dry patch buffer has no reverb tail"
        );
        assert_eq!(wet.r2d2.len(), 1);
        assert!(
            dry.r2d2[0].1.len() < wet.r2d2[0].1.len(),
            "a dry R2D2 buffer has no reverb tail"
        );
        assert!(dry.duration < wet.duration);
    }

    #[test]
    fn wet_parts_flatten_to_the_playback_command() {
        let t = Translator::new(Ok(()));
        let notes = || {
            vec![
                patch_note(json!({"name": "s", "subtractive": {}}), 60, 0.25, 0.5),
                patch_note(json!("tr_808_kick"), 36, 0.0, 0.25),
                SimpleNote {
                    note: Some(60),
                    duration: Some(0.5),
                    channel: 2,
                    instrument: Some(73),
                    ..Default::default()
                },
            ]
        };
        let direct = t
            .translate(seq(notes()), PlayMode::Layer, &no_session())
            .unwrap();
        let parts = t
            .translate_parts(seq(notes()), &no_session(), Effects::Wet)
            .unwrap();
        assert_eq!(parts.duration, direct.duration);
        assert_eq!(parts.tempo, 120);

        let command = parts.into_command(PlayMode::Layer);
        assert_eq!(command.mode, PlayMode::Layer);
        assert_eq!(command.tempo, direct.command.tempo);
        assert_eq!(command.events, direct.command.events);
        assert_eq!(
            command.midi_effects.is_some(),
            direct.command.midi_effects.is_some()
        );
        let shape = |c: &PlayCommand| -> Vec<(u64, usize)> {
            c.buffers.iter().map(|(s, b)| (*s, b.len())).collect()
        };
        assert_eq!(shape(&command), shape(&direct.command));
    }

    #[test]
    fn midi_notes_need_a_soundfont_but_patches_do_not() {
        let t = Translator::new(Err("no soundfont".into()));
        assert!(
            t.translate(
                seq(vec![patch_note(json!("sub_bass"), 36, 0.0, 0.2)]),
                PlayMode::Replace,
                &no_session()
            )
            .is_ok()
        );
        let midi = SimpleNote {
            note: Some(60),
            ..Default::default()
        };
        assert!(
            t.translate(seq(vec![midi]), PlayMode::Replace, &no_session())
                .unwrap_err()
                .contains("SoundFont")
        );
    }

    #[test]
    fn a_negative_duration_on_a_patch_note_does_not_panic() {
        let t = Translator::new(Err("no soundfont".into()));
        let n = patch_note(json!("sub_bass"), 36, 0.0, -1.0);
        assert!(
            t.translate(seq(vec![n]), PlayMode::Replace, &no_session())
                .is_ok()
        );
    }

    #[test]
    fn musical_time_is_converted_with_the_sequence_tempo() {
        let translator = Translator::new(Ok(()));
        let mut s = seq(vec![SimpleNote {
            note: Some(60),
            // `SimpleNote::default()` fills these in; musical time only applies
            // when the caller left the seconds fields unset.
            start_time: None,
            duration: None,
            musical_time: Some(crate::midi::MusicalTime {
                bar: 2,
                beat: 1,
                tick: 0,
            }),
            musical_duration: Some(crate::midi::MusicalDuration::Bars(1.0)),
            ..Default::default()
        }]);
        s.tempo = 60;
        s.beats_per_bar = 4;
        let t = translator
            .translate(s, PlayMode::Replace, &no_session())
            .unwrap();
        let on = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOn { .. }))
            .unwrap();
        assert_eq!(on.0, 4 * 44_100, "bar 2 at 60 BPM in 4/4 starts at 4 s");
        let off = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOff { .. }))
            .unwrap();
        assert_eq!(off.0, 8 * 44_100);
    }

    #[test]
    fn an_empty_sequence_translates_to_nothing() {
        let t = Translator::new(Ok(()))
            .translate(seq(vec![]), PlayMode::Replace, &no_session())
            .unwrap();
        assert!(t.command.events.is_empty() && t.command.buffers.is_empty());
        assert_eq!(t.duration, Duration::ZERO);
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

    fn reverb() -> EffectConfig {
        serde_json::from_value(serde_json::json!({
            "type": "reverb", "room_size": 0.6, "intensity": 0.4
        }))
        .unwrap()
    }

    #[test]
    fn a_negative_start_time_is_clamped_instead_of_panicking() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    velocity: Some(100),
                    duration: Some(0.5),
                    start_time: Some(-1.0),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert_eq!(
            t.command.events.first().map(|(at, _)| *at),
            Some(0),
            "a negative start time must land at offset 0"
        );
    }

    #[test]
    fn a_negative_duration_on_a_midi_note_is_clamped_instead_of_panicking() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    velocity: Some(100),
                    duration: Some(-1.0),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let note_off = t
            .command
            .events
            .iter()
            .find(|(_, e)| matches!(e, EventKind::NoteOff { .. }))
            .expect("a note off must still be scheduled");
        assert_eq!(
            note_off.0, 1,
            "a negative duration must clamp to 0 and hit the max(1) floor in midi_events"
        );
    }

    #[test]
    fn the_bus_chain_comes_from_the_first_midi_note_that_has_effects() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![
                    SimpleNote {
                        note: Some(60),
                        duration: Some(0.2),
                        ..Default::default()
                    },
                    SimpleNote {
                        note: Some(64),
                        duration: Some(0.2),
                        effects: Some(vec![reverb()]),
                        ..Default::default()
                    },
                    SimpleNote {
                        note: Some(67),
                        duration: Some(0.2),
                        effects: Some(vec![
                            serde_json::from_value(serde_json::json!({
                                "type": "delay", "delay_time": 0.25,
                                "feedback": 0.3, "intensity": 0.5
                            }))
                            .unwrap(),
                        ]),
                        ..Default::default()
                    },
                ]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let chain = t
            .command
            .midi_effects
            .expect("a MIDI note supplied effects");
        assert_eq!(chain.len(), 1, "later MIDI effects must not join the chain");
        assert!(matches!(
            chain[0].effect,
            crate::midi::EffectType::Reverb { .. }
        ));
    }

    /// `effects_preset` is merged for every note, not only for notes that also
    /// named a (now removed) Rust preset, so a plain MIDI note's named chain
    /// reaches the bus.
    #[test]
    fn effects_preset_on_a_midi_note_becomes_the_bus_chain() {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    duration: Some(0.2),
                    effects_preset: Some("studio".into()),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let chain = t
            .command
            .midi_effects
            .expect("the studio preset must reach the MIDI bus");
        assert!(!chain.is_empty(), "the studio chain must not be empty");
    }

    #[test]
    fn explicit_effects_come_before_the_preset_chain() {
        let distortion: EffectConfig =
            serde_json::from_value(serde_json::json!({"type": "distortion"})).unwrap();
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    duration: Some(0.2),
                    effects: Some(vec![distortion]),
                    effects_preset: Some("studio".into()),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let chain = t.command.midi_effects.expect("effects were supplied");
        assert!(
            matches!(chain[0].effect, crate::midi::EffectType::Distortion { .. }),
            "the note's own effects must stay first, got {:?}",
            chain[0].effect
        );
        assert!(
            chain.len() > 1,
            "the preset chain must be appended, got {} effects",
            chain.len()
        );

        // An unknown preset name fails `validate_effects`, which warns and
        // drops both effect fields rather than erroring or panicking.
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note: Some(60),
                    duration: Some(0.2),
                    effects_preset: Some("nope".into()),
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert!(
            t.command.midi_effects.is_none(),
            "an unknown effects preset must leave the bus dry"
        );
    }

    #[test]
    fn effects_on_synthesis_notes_are_rejected_and_never_reach_the_midi_bus() {
        // A patch renders through its own chain, so a note-level chain would
        // be dropped: the translator refuses it instead of leaking it onto
        // the shared MIDI bus.
        let err = Translator::new(Ok(()))
            .translate(
                seq(vec![
                    SimpleNote {
                        duration: Some(0.2),
                        effects: Some(vec![reverb()]),
                        ..patch_note(json!("sub_bass"), 36, 0.0, 0.2)
                    },
                    SimpleNote {
                        note: Some(60),
                        duration: Some(0.2),
                        ..Default::default()
                    },
                ]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap_err();
        assert!(err.contains("patch's \"effects\" chain"), "{err}");

        // Without them the bus stays dry: the patch's audio is pre-rendered.
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![
                    patch_note(json!("sub_bass"), 36, 0.0, 0.2),
                    SimpleNote {
                        note: Some(60),
                        duration: Some(0.2),
                        ..Default::default()
                    },
                ]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert!(
            t.command.midi_effects.is_none(),
            "synthesis effects are rendered into the buffer, not onto the MIDI bus"
        );
    }

    fn r2d2_buffer_len(effects: Option<Vec<EffectConfig>>) -> usize {
        let t = Translator::new(Ok(()))
            .translate(
                seq(vec![SimpleNote {
                    note_type: "r2d2".to_string(),
                    r2d2_emotion: Some("Happy".to_string()),
                    r2d2_intensity: Some(0.7),
                    r2d2_complexity: Some(2),
                    duration: Some(0.5),
                    effects,
                    ..Default::default()
                }]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        assert_eq!(t.command.buffers.len(), 1);
        t.command.buffers[0].1.len()
    }

    #[test]
    fn an_r2d2_buffer_carries_its_own_effect_tail() {
        let tail = 0.5 * SAMPLE_RATE as f64 + SAMPLE_RATE as f64;
        let with_effects = r2d2_buffer_len(Some(vec![reverb()]));
        assert!(
            with_effects as f64 >= tail,
            "expected room for a one-second tail, got {with_effects} samples"
        );
        let without = r2d2_buffer_len(None);
        assert!(
            (without as f64) < tail,
            "a dry R2D2 note must not be padded, got {without} samples"
        );
    }

    #[test]
    fn an_r2d2_tail_is_capped_like_a_patch_render() {
        // The tool layer keeps `tempo` inside 20-300, but nothing downstream
        // re-checks it, so the tail itself carries `render_patch`'s ceiling.
        let delay: Vec<EffectConfig> = vec![
            serde_json::from_value(
                serde_json::json!({"type": "delay", "random_beats": [4.0, 4.0]}),
            )
            .unwrap(),
        ];
        assert_eq!(r2d2_tail_seconds(&delay, 1), MAX_RENDER_SECONDS);
        // 4 beats at 20 BPM is 12 s, so the uncapped tail is 48.5 s.
        assert!((r2d2_tail_seconds(&delay, 20) - 48.5).abs() < 1e-3);
    }

    #[test]
    fn the_slowest_supported_tempo_still_bounds_the_r2d2_buffer() {
        // Built directly, bypassing the tool-level tempo check.
        let t = Translator::new(Ok(()));
        let mut s = seq(vec![SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(0.2),
            effects: Some(vec![
                serde_json::from_value(
                    serde_json::json!({"type": "delay", "random_beats": [4.0, 4.0]}),
                )
                .unwrap(),
            ]),
            ..Default::default()
        }]);
        s.tempo = 20;
        let tr = t.translate(s, PlayMode::Replace, &no_session()).unwrap();
        let frames = tr.command.buffers[0].1.len() as f32;
        let ceiling =
            (crate::midi::MAX_NOTE_SECONDS as f32 + MAX_RENDER_SECONDS) * SAMPLE_RATE as f32;
        assert!(frames <= ceiling, "{frames} frames");
        assert!(
            tr.duration.as_secs_f32() <= crate::midi::MAX_NOTE_SECONDS as f32 + MAX_RENDER_SECONDS,
            "{:?}",
            tr.duration
        );
    }

    #[test]
    fn the_midi_bus_duration_covers_its_delay_tail() {
        let t = Translator::new(Ok(()));
        let mut s = seq(vec![SimpleNote {
            note: Some(60),
            start_time: Some(0.0),
            duration: Some(0.5),
            effects: Some(vec![
                serde_json::from_value(
                    serde_json::json!({"type": "delay", "random_beats": [1.0, 1.0]}),
                )
                .unwrap(),
            ]),
            ..Default::default()
        }]);
        s.tempo = 60;
        let tr = t.translate(s, PlayMode::Replace, &no_session()).unwrap();
        // 1 beat at 60 BPM is 1 s, so the delay rings for 4 x 1 + 0.5 s.
        assert!(
            tr.duration.as_secs_f64() >= 0.5 + 4.5 - 1e-6,
            "the bus tail must follow the delay, not just the CC-based estimate: {:?}",
            tr.duration
        );
    }

    #[test]
    fn an_r2d2_notes_reported_duration_follows_the_tempo_scaled_tail() {
        let delay: EffectConfig = serde_json::from_value(serde_json::json!({
            "type": "delay", "random_beats": [1.0, 1.0], "intensity": 0.5
        }))
        .unwrap();
        let r2d2_note = |effects| SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(0.2),
            effects: Some(vec![effects]),
            ..Default::default()
        };
        let t = Translator::new(Ok(()));
        let mut s = seq(vec![r2d2_note(delay.clone())]);
        s.tempo = 60;
        let slow = t
            .translate(s.clone(), PlayMode::Replace, &no_session())
            .unwrap();
        s.tempo = 120;
        let fast = t.translate(s, PlayMode::Replace, &no_session()).unwrap();
        assert!(
            slow.duration > fast.duration,
            "slower tempo means longer beat-synced repeats, so a longer reported duration: {:?} vs {:?}",
            slow.duration,
            fast.duration
        );
        // 1 beat at 60 BPM is 1 s -> tail 4.5 s; note duration 0.2 s.
        assert!(
            slow.duration.as_secs_f64() >= 0.2 + 4.5 - 1e-6,
            "duration must cover the note plus its tempo-scaled tail: {:?}",
            slow.duration
        );
        // 1 beat at 120 BPM is 0.5 s -> tail 2.5 s; note duration 0.2 s.
        assert!(
            fast.duration.as_secs_f64() >= 0.2 + 2.5 - 1e-6,
            "duration must cover the note plus its tempo-scaled tail: {:?}",
            fast.duration
        );
    }

    #[test]
    fn notes_on_the_same_patch_share_one_buffer_scheduled_at_the_first_note() {
        let t = Translator::new(Err("no soundfont".into()));
        let seq = seq(vec![
            patch_note(json!("minimoog_bass"), 36, 0.5, 0.5),
            patch_note(json!("minimoog_bass"), 43, 1.0, 0.5),
            patch_note(json!("tr_808_kick"), 36, 0.0, 0.25),
        ]);
        let tr = t.translate(seq, PlayMode::Replace, &no_session()).unwrap();
        assert_eq!(tr.command.buffers.len(), 2, "one buffer per patch");
        let starts: Vec<u64> = tr.command.buffers.iter().map(|b| b.0).collect();
        assert!(starts.contains(&0));
        assert!(starts.contains(&seconds_to_frames(Duration::from_secs_f64(0.5))));
        // Bass buffer: two notes -> ends at 1.5 s + release; the reported duration covers it.
        assert!(tr.duration.as_secs_f64() >= 1.5);
    }

    #[test]
    fn inline_patches_render_and_identical_inline_patches_group() {
        let t = Translator::new(Err("no soundfont".into()));
        let inline = json!({"name": "blip", "subtractive": {"osc1": {"wave": "square"}}});
        let seq = seq(vec![
            patch_note(inline.clone(), 60, 0.0, 0.2),
            patch_note(inline, 64, 0.2, 0.2),
        ]);
        let tr = t.translate(seq, PlayMode::Replace, &no_session()).unwrap();
        assert_eq!(tr.command.buffers.len(), 1);
        assert!(tr.command.buffers[0].1.iter().any(|s| s[0].abs() > 0.01));
    }

    #[test]
    fn session_patches_shadow_builtins_and_unknown_names_list_what_exists() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut session = no_session();
        let mine: crate::expressive::Patch =
            serde_json::from_value(json!({"name": "Mine", "percussion": {"kind": "snare"}}))
                .unwrap();
        session.insert(mine.key(), mine);
        let ok = t.translate(
            seq(vec![patch_note(json!("mine"), 38, 0.0, 0.2)]),
            PlayMode::Replace,
            &session,
        );
        assert!(ok.is_ok());
        let err = t
            .translate(
                seq(vec![patch_note(json!("nope"), 38, 0.0, 0.2)]),
                PlayMode::Replace,
                &session,
            )
            .unwrap_err();
        assert!(
            err.contains("nope") && err.contains("Mine") && err.contains("list_sounds"),
            "{err}"
        );
    }

    #[test]
    fn a_pitched_patch_needs_a_note_but_percussion_does_not() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut n = patch_note(json!("minimoog_bass"), 36, 0.0, 0.2);
        n.note = None;
        let err = t
            .translate(seq(vec![n]), PlayMode::Replace, &no_session())
            .unwrap_err();
        assert!(err.contains("note"), "{err}");
        let mut k = patch_note(json!("tr_808_kick"), 36, 0.0, 0.2);
        k.note = None;
        assert!(
            t.translate(seq(vec![k]), PlayMode::Replace, &no_session())
                .is_ok()
        );
    }

    #[test]
    fn an_invalid_inline_patch_is_an_error_naming_the_field() {
        let t = Translator::new(Err("no soundfont".into()));
        let bad = json!({"name": "hot", "subtractive": {"filter": {"cutoff": 99999}}});
        let err = t
            .translate(
                seq(vec![patch_note(bad, 60, 0.0, 0.2)]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap_err();
        assert!(err.contains("subtractive.filter.cutoff"), "{err}");
        assert!(err.starts_with("Note 1:"), "{err}");
    }
}
