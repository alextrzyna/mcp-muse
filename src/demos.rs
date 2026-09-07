//! Listen-by-ear demos run from the CLI. They play through the same
//! `MidiPlayer` the server uses; DSP correctness is covered by the unit tests.

use crate::expressive::{PatchLibrary, SynthRef};
use crate::midi::{MidiPlayer, PlayMode, SimpleNote, SimpleSequence};
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

fn patch_notes(name: &str, notes: &[(u8, f64, f64)]) -> SimpleSequence {
    SimpleSequence {
        notes: notes
            .iter()
            .map(|&(note, start, duration)| SimpleNote {
                note: Some(note),
                velocity: Some(100),
                start_time: Some(start),
                duration: Some(duration),
                synth: Some(SynthRef::Name(name.to_string())),
                ..Default::default()
            })
            .collect(),
        tempo: 120,
        beats_per_bar: 4,
    }
}

/// Every built-in patch, one short phrase each, grouped by category.
pub fn test_synths() -> Result<(), Box<dyn std::error::Error>> {
    let mut player = MidiPlayer::new()?;
    let library = PatchLibrary::new();
    let none = HashMap::new();
    for (category, patches) in library.catalog() {
        println!("\n## {}", category.as_str());
        for patch in patches {
            println!("- {}: {}", patch.name, patch.description);
            let phrase: &[(u8, f64, f64)] = match category.as_str() {
                "drums" | "fx" => &[(36, 0.0, 0.5), (36, 0.5, 0.5)],
                "pad" => &[(48, 0.0, 3.0), (55, 0.0, 3.0), (60, 0.0, 3.0)],
                _ => &[(36, 0.0, 0.4), (43, 0.5, 0.4), (48, 1.0, 0.8)],
            };
            let duration =
                player.play(patch_notes(&patch.name, phrase), PlayMode::Replace, &none)?;
            sleep(duration.min(Duration::from_secs(4)));
        }
    }
    Ok(())
}

/// A bar of 808/909 drums from the percussion patches.
pub fn test_drums() -> Result<(), Box<dyn std::error::Error>> {
    let mut player = MidiPlayer::new()?;
    let none = HashMap::new();
    let mut notes = Vec::new();
    for beat in 0..4 {
        let t = beat as f64 * 0.5;
        notes.push(("tr_808_kick", t, 0.4));
        notes.push(("tr_808_hihat", t + 0.25, 0.15));
        if beat % 2 == 1 {
            notes.push(("tr_909_snare", t, 0.3));
        }
    }
    let seq = SimpleSequence {
        notes: notes
            .into_iter()
            .map(|(name, start, duration)| SimpleNote {
                start_time: Some(start),
                duration: Some(duration),
                synth: Some(SynthRef::Name(name.to_string())),
                ..Default::default()
            })
            .collect(),
        tempo: 120,
        beats_per_bar: 4,
    };
    println!("🥁 808 kick + hats, 909 snare");
    let duration = player.play(seq, PlayMode::Replace, &none)?;
    sleep(duration);
    Ok(())
}

/// A MIDI piano chord dry, then with reverb, then with a delay chain.
pub fn test_effects() -> Result<(), Box<dyn std::error::Error>> {
    let mut player = MidiPlayer::new()?;
    let none = HashMap::new();
    let chord = |effects: Option<Vec<crate::midi::EffectConfig>>| SimpleSequence {
        notes: [60u8, 64, 67]
            .iter()
            .map(|&n| SimpleNote {
                note: Some(n),
                velocity: Some(100),
                instrument: Some(1),
                start_time: Some(0.0),
                duration: Some(2.0),
                effects: effects.clone(),
                ..Default::default()
            })
            .collect(),
        tempo: 120,
        beats_per_bar: 4,
    };
    let reverb = serde_json::from_value(serde_json::json!([
        {"type": "reverb", "room_size": 0.9, "wet_level": 0.6, "intensity": 0.8}
    ]))?;
    let delay = serde_json::from_value(serde_json::json!([
        {"type": "delay", "delay_time": 0.375, "feedback": 0.5, "intensity": 0.6}
    ]))?;
    for (label, fx) in [
        ("dry", None),
        ("reverb", Some(reverb)),
        ("delay", Some(delay)),
    ] {
        println!("🎹 piano chord: {label}");
        let duration = player.play(chord(fx), PlayMode::Replace, &none)?;
        sleep(duration);
    }
    Ok(())
}
