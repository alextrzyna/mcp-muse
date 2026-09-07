//! Renders every note of one patch into a single stereo buffer: voices are
//! summed per sample, then the patch's effects chain runs once over the sum.
#![allow(dead_code)]

use crate::expressive::engines::{
    FmVoice, MIN_HIT_SECONDS, Modulation, PercussionVoice, SubtractiveVoice, Voice, WavetableVoice,
};
use crate::expressive::{EffectsChain, Patch};

/// Extra silence rendered after the last release so reverbs and delays can ring out.
pub const EFFECT_TAIL_SECONDS: f32 = 1.0;

/// Hard ceiling on how much audio one `render_patch` call may produce.
/// `SimpleNote::validate_timing` already rejects absurd note lengths at the
/// tool boundary (MAX_NOTE_SECONDS); this is defence in depth for every other
/// caller, so a bad number can never allocate gigabytes and hang the server.
pub const MAX_RENDER_SECONDS: f32 = 600.0;

/// One note to render, in seconds relative to the buffer start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    pub start: f32,
    pub duration: f32,
    pub frequency: f32,
    /// 0..1; scales amplitude linearly.
    pub velocity: f32,
}

struct ActiveVoice {
    start: usize,
    gate_end: usize,
    gain: f32,
    voice: Box<dyn Voice>,
}

/// Seconds of audio `render_patch` produces for these notes.
pub fn render_length_seconds(patch: &Patch, notes: &[NoteEvent]) -> f32 {
    if notes.is_empty() {
        return 0.0;
    }
    let min_gate = if patch.percussion.as_ref().is_some_and(|p| p.level > 0.0) {
        MIN_HIT_SECONDS
    } else {
        0.0
    };
    let last_gate = notes
        .iter()
        .map(|n| n.start.max(0.0) + n.duration.max(0.0).max(min_gate))
        .fold(0.0f32, f32::max);
    let effect_tail = if patch.effects.iter().any(|e| e.enabled) {
        EFFECT_TAIL_SECONDS
    } else {
        0.0
    };
    (last_gate + patch.release_seconds() + effect_tail).min(MAX_RENDER_SECONDS)
}

/// Render all `notes` through `patch` into one stereo buffer.
pub fn render_patch(patch: &Patch, notes: &[NoteEvent], sample_rate: f32) -> Vec<[f32; 2]> {
    let total = (render_length_seconds(patch, notes) * sample_rate) as usize;
    if total == 0 {
        return Vec::new();
    }

    let mut voices: Vec<ActiveVoice> = Vec::new();
    for note in notes {
        let start = (note.start.max(0.0) * sample_rate) as usize;
        let gate = note.duration.max(0.0);
        let gate_end = start + (gate * sample_rate) as usize;
        let gain = note.velocity.clamp(0.0, 1.0) * patch.level;
        if let Some(sub) = &patch.subtractive
            && sub.level > 0.0
        {
            voices.push(ActiveVoice {
                start,
                gate_end,
                gain,
                voice: Box::new(SubtractiveVoice::new(sub, note.frequency, sample_rate)),
            });
        }
        if let Some(fm) = &patch.fm
            && fm.level > 0.0
        {
            voices.push(ActiveVoice {
                start,
                gate_end,
                gain,
                voice: Box::new(FmVoice::new(fm, note.frequency, sample_rate)),
            });
        }
        if let Some(wt) = &patch.wavetable
            && wt.level > 0.0
        {
            voices.push(ActiveVoice {
                start,
                gate_end,
                gain,
                voice: Box::new(WavetableVoice::new(wt, note.frequency, sample_rate)),
            });
        }
        if let Some(perc) = &patch.percussion
            && perc.level > 0.0
        {
            voices.push(ActiveVoice {
                start,
                gate_end,
                gain,
                voice: Box::new(PercussionVoice::new(perc, gate, sample_rate)),
            });
        }
    }

    let mods = Modulation::default();
    let mut chain_l = EffectsChain::new(sample_rate, &patch.effects);
    let mut chain_r = EffectsChain::new(sample_rate, &patch.effects);
    let mut out = vec![[0.0f32; 2]; total];

    for (i, frame) in out.iter_mut().enumerate() {
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for v in &mut voices {
            if i < v.start || !v.voice.is_active() {
                continue;
            }
            if i == v.gate_end {
                v.voice.gate_off();
            }
            let (vl, vr) = v.voice.tick(&mods);
            l += vl * v.gain;
            r += vr * v.gain;
        }
        *frame = [chain_l.process(l), chain_r.process(r)];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::rms;
    use serde_json::json;

    const SR: f32 = 44100.0;

    fn patch(v: serde_json::Value) -> Patch {
        serde_json::from_value(v).unwrap()
    }

    fn note(start: f32, duration: f32, frequency: f32) -> NoteEvent {
        NoteEvent {
            start,
            duration,
            frequency,
            velocity: 1.0,
        }
    }

    fn left(buf: &[[f32; 2]]) -> Vec<f32> {
        buf.iter().map(|s| s[0]).collect()
    }

    #[test]
    fn length_covers_the_last_note_its_release_and_the_effect_tail() {
        let dry = patch(json!({"name": "d", "subtractive": {"env": {"release": 0.5}}}));
        let notes = [note(0.0, 1.0, 220.0), note(1.0, 1.0, 330.0)];
        assert!((render_length_seconds(&dry, &notes) - 2.5).abs() < 1e-4);
        let wet = patch(
            json!({"name": "w", "subtractive": {"env": {"release": 0.5}},
            "effects": [{"type": "reverb"}]}),
        );
        assert!((render_length_seconds(&wet, &notes) - 3.5).abs() < 1e-4);
        assert_eq!(render_patch(&dry, &notes, SR).len(), (2.5 * SR) as usize);
    }

    #[test]
    fn a_note_starts_at_its_offset_and_rings_through_its_release() {
        let p =
            patch(json!({"name": "p", "subtractive": {"env": {"attack": 0.001, "release": 0.3}}}));
        let buf = left(&render_patch(&p, &[note(0.5, 0.5, 220.0)], SR));
        assert!(
            rms(&buf[..(0.45 * SR) as usize]) < 1e-6,
            "silent before start"
        );
        assert!(
            rms(&buf[(0.6 * SR) as usize..(0.9 * SR) as usize]) > 0.1,
            "sounding"
        );
        assert!(
            rms(&buf[(1.1 * SR) as usize..(1.2 * SR) as usize]) > 0.01,
            "release audible"
        );
        // The buffer ends exactly when the release does (1.0 s gate + 0.3 s release).
        assert_eq!(buf.len(), (1.3 * SR) as usize);
        assert!(
            rms(&buf[(1.295 * SR) as usize..]) < 0.05,
            "faded out by the end"
        );
    }

    #[test]
    fn overlapping_notes_sum() {
        let p = patch(
            json!({"name": "p", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}),
        );
        let one = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let two = left(&render_patch(
            &p,
            &[note(0.0, 1.0, 220.0), note(0.0, 1.0, 220.0)],
            SR,
        ));
        let r1 = rms(&one[4410..40000]);
        let r2 = rms(&two[4410..40000]);
        assert!(
            (r2 / r1 - 2.0).abs() < 0.05,
            "two identical voices double the amplitude"
        );
    }

    #[test]
    fn velocity_and_patch_level_scale_amplitude() {
        let p = patch(
            json!({"name": "p", "level": 0.5, "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}),
        );
        let full = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let soft = left(&render_patch(
            &p,
            &[NoteEvent {
                velocity: 0.5,
                ..note(0.0, 1.0, 220.0)
            }],
            SR,
        ));
        let peak = |s: &[f32]| s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            (peak(&full) - 0.5).abs() < 0.03,
            "level 0.5: {}",
            peak(&full)
        );
        assert!(
            (peak(&soft) - 0.25).abs() < 0.03,
            "velocity halves it: {}",
            peak(&soft)
        );
    }

    #[test]
    fn effects_run_once_over_the_summed_signal() {
        let dry = patch(json!({"name": "d", "subtractive": {"env": {"release": 0.01}}}));
        let wet = patch(
            json!({"name": "w", "subtractive": {"env": {"release": 0.01}},
            "effects": [{"type": "reverb", "room_size": 0.9, "wet_level": 0.8, "intensity": 1.0}]}),
        );
        let n = [note(0.0, 0.5, 220.0)];
        let d = left(&render_patch(&dry, &n, SR));
        let w = left(&render_patch(&wet, &n, SR));
        let tail = (0.7 * SR) as usize..(1.2 * SR) as usize;
        // Dry has no effect tail at all: its buffer ends at the release, well
        // before `tail` starts (this is what makes the reverb's tail audible).
        assert!(d.len() <= tail.start, "dry has no ringing tail");
        assert!(rms(&w[tail]) > 1e-3, "reverb tail rings");
    }

    #[test]
    fn mono_engines_are_centered_and_percussion_ignores_pitch() {
        let p = patch(json!({"name": "k", "percussion": {"kind": "kick"}}));
        let a = render_patch(&p, &[note(0.0, 0.5, 60.0)], SR);
        assert!(a.iter().all(|s| s[0] == s[1]), "centered");
        assert!(rms(&left(&a)) > 0.01);
    }

    #[test]
    fn two_engines_layer() {
        let p = patch(
            json!({"name": "both", "subtractive": {"level": 0.5, "env": {"release": 0.01}},
            "percussion": {"kind": "kick", "level": 0.5}}),
        );
        let a = left(&render_patch(&p, &[note(0.0, 0.5, 110.0)], SR));
        let sub_only =
            patch(json!({"name": "s", "subtractive": {"level": 0.5, "env": {"release": 0.01}}}));
        let b = left(&render_patch(&sub_only, &[note(0.0, 0.5, 110.0)], SR));
        assert!(
            rms(&a[..2205]) > rms(&b[..2205]) * 1.2,
            "kick adds energy at the start"
        );
    }

    #[test]
    fn an_absurd_duration_is_capped_at_the_render_limit() {
        // Rendered at 100 Hz so the capped buffer stays tiny; the point is
        // that no note can size a buffer beyond MAX_RENDER_SECONDS.
        let p = patch(json!({"name": "p", "subtractive": {"env": {"release": 0.01}}}));
        let notes = [note(0.0, 100_000.0, 220.0)];
        assert!(render_length_seconds(&p, &notes) <= MAX_RENDER_SECONDS);
        let buf = render_patch(&p, &notes, 100.0);
        assert!(
            buf.len() <= (MAX_RENDER_SECONDS * 100.0) as usize,
            "buffer of {} samples",
            buf.len()
        );
    }

    #[test]
    fn empty_notes_render_nothing() {
        let p = patch(json!({"name": "p", "subtractive": {}}));
        assert!(render_patch(&p, &[], SR).is_empty());
    }

    #[test]
    fn a_zero_duration_percussion_note_still_renders_its_hit() {
        let p = patch(json!({"name": "k", "percussion": {"kind": "kick"}}));
        let n = [NoteEvent {
            start: 0.0,
            duration: 0.0,
            frequency: 60.0,
            velocity: 1.0,
        }];
        let buf = render_patch(&p, &n, SR);
        assert_eq!(buf.len(), (MIN_HIT_SECONDS * SR) as usize);
        assert!(rms(&left(&buf)) > 0.01);
    }

    #[test]
    fn fm_engine_renders_and_layers_with_subtractive() {
        let fm_only = patch(json!({"name": "f", "fm": {"operators": [
            {"ratio": 1.0, "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            {"ratio": 2.0, "level": 0.5, "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}]}}));
        let a = left(&render_patch(&fm_only, &[note(0.0, 0.5, 220.0)], SR));
        assert!(rms(&a[441..22050]) > 0.3, "fm voice sounds");
        assert!(a.iter().all(|x| x.is_finite()));

        // The fm operator is detuned 7 cents from the subtractive voice: at an
        // exact 1:1 ratio with zero detune the sine carrier locks into the
        // opposite phase of the saw's fundamental (measured: their raw
        // per-sample product integrates strongly negative), so layering it
        // *removes* energy instead of adding it. A few cents of detune (the
        // same trick `dx7_e_piano`'s second carrier uses) breaks that lock.
        let both = patch(json!({"name": "b", "level": 0.5,
            "subtractive": {"level": 0.5, "env": {"release": 0.01}},
            "fm": {"level": 0.5, "operators": [{"ratio": 1.0, "detune_cents": 7.0, "env": {"release": 0.01}}]}}));
        let sub_only = patch(json!({"name": "s", "level": 0.5,
            "subtractive": {"level": 0.5, "env": {"release": 0.01}}}));
        let b = left(&render_patch(&both, &[note(0.0, 0.5, 220.0)], SR));
        let s = left(&render_patch(&sub_only, &[note(0.0, 0.5, 220.0)], SR));
        assert!(
            rms(&b[4410..22050]) > rms(&s[4410..22050]) * 1.2,
            "fm adds energy"
        );
    }

    #[test]
    fn fm_release_extends_the_buffer() {
        let p = patch(json!({"name": "f", "fm": {"operators": [{"env": {"release": 0.7}}]}}));
        assert!((render_length_seconds(&p, &[note(0.0, 0.5, 220.0)]) - 1.2).abs() < 1e-4);
    }

    #[test]
    fn wavetable_engine_renders() {
        let p =
            patch(json!({"name": "w", "wavetable": {"table": "organ", "env": {"release": 0.2}}}));
        let buf = left(&render_patch(&p, &[note(0.0, 0.5, 220.0)], SR));
        assert!(rms(&buf[441..22050]) > 0.3);
        assert_eq!(buf.len(), (0.7 * SR) as usize, "gate + release");
    }
}
