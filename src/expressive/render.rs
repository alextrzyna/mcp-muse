//! Renders every note of one patch into a single stereo buffer: voices are
//! summed per sample, then the patch's effects chain runs once over the sum.
//! The patch's optional LFO free-runs from the start of the buffer, read
//! once per frame, before the voices tick, and is mapped to a `Modulation`
//! each sample (identity when the LFO is absent or inactive).
#![allow(dead_code)]

use crate::expressive::engines::{
    FmVoice, GranularVoice, MIN_HIT_SECONDS, Modulation, PercussionVoice, SubtractiveVoice, Voice,
    WavetableVoice,
};
use crate::expressive::{EffectsChain, Lfo, Patch};

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
        if let Some(g) = &patch.granular
            && g.level > 0.0
        {
            voices.push(ActiveVoice {
                start,
                gate_end,
                gain,
                voice: Box::new(GranularVoice::new(g, note.frequency, sample_rate)),
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

    let mut lfo = patch
        .lfo
        .as_ref()
        .filter(|l| l.is_active())
        .map(|l| (Lfo::new(l.rate, l.wave, sample_rate), l.target, l.depth));
    let mut chain_l = EffectsChain::new(sample_rate, &patch.effects);
    let mut chain_r = EffectsChain::new(sample_rate, &patch.effects);
    let mut out = vec![[0.0f32; 2]; total];

    for (i, frame) in out.iter_mut().enumerate() {
        let mods = match &mut lfo {
            Some((osc, target, depth)) => Modulation::from_lfo(*target, *depth, osc.next()),
            None => Modulation::default(),
        };
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

    /// Zero-crossing rate in successive 50 ms windows, skipping the first:
    /// every note starts at phase 0 with the envelope at 0, so the very
    /// first window always loses the crossing that would have landed at
    /// sample 0, regardless of any LFO. A trailing partial window is also
    /// dropped: over ~10 ms it quantizes to 100 Hz steps, swamping the
    /// ±semitone swing this helper is meant to resolve.
    fn zcr_windows(s: &[f32]) -> Vec<f32> {
        if s.len() <= 2205 {
            return Vec::new();
        }
        let z: Vec<f32> = s[2205..]
            .chunks_exact(2205)
            .map(|w| crate::expressive::test_util::zero_crossing_rate(w, SR))
            .collect();
        assert!(!z.is_empty(), "buffer too short for a window");
        z
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

    #[test]
    fn pitch_lfo_moves_the_pitch_up_and_down_at_the_lfo_rate() {
        let p = patch(
            json!({"name": "vib", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 2.0, "depth": 1.0, "wave": "sine", "target": "pitch"}}),
        );
        let s = left(&render_patch(&p, &[note(0.0, 2.0, 440.0)], SR));
        let z = zcr_windows(&s);
        let (lo, hi) = z
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        // 440 Hz ± 2 semitones = 392..494 Hz, i.e. 784..988 crossings/s.
        assert!(hi > 940.0 && lo < 830.0, "pitch swings: {lo}..{hi}");
        let dry = patch(
            json!({"name": "dry", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}),
        );
        let zd = zcr_windows(&left(&render_patch(&dry, &[note(0.0, 2.0, 440.0)], SR)));
        let (lo, hi) = zd
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(hi - lo < 20.0, "no LFO, no swing: {lo}..{hi}");
    }

    #[test]
    fn cutoff_lfo_varies_high_harmonic_energy() {
        let p = patch(
            json!({"name": "wah", "subtractive": {"osc1": {"wave": "saw"},
            "filter": {"type": "low_pass", "cutoff": 400, "resonance": 0.2},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 1.0, "depth": 1.0, "wave": "square", "target": "cutoff"}}),
        );
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 110.0)], SR));
        // Square LFO: first half cycle cutoff x4 (1600 Hz), second half /4 (100 Hz).
        let open = crate::expressive::test_util::goertzel_power(&s[2205..20000], 1100.0, SR);
        let closed = crate::expressive::test_util::goertzel_power(&s[24255..42000], 1100.0, SR);
        assert!(
            crate::expressive::test_util::db(open / closed) > 20.0,
            "10th harmonic follows the LFO"
        );
    }

    #[test]
    fn amplitude_lfo_is_a_tremolo() {
        let p = patch(
            json!({"name": "trem", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 4.0, "depth": 1.0, "wave": "sine", "target": "amplitude"}}),
        );
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let env: Vec<f32> = s[2205..].chunks(441).map(rms).collect();
        let (lo, hi) = env
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(lo < hi * 0.2, "amplitude dips near silence: {lo} vs {hi}");
    }

    #[test]
    fn morph_lfo_moves_the_wavetable_between_tables() {
        // basic -> warm: the 3rd harmonic grows with morph.
        let p = patch(
            json!({"name": "mw", "wavetable": {"table": "basic", "morph": 0.5,
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 1.0, "depth": 1.0, "wave": "square", "target": "morph"}}),
        );
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let h3 = |w: &[f32]| {
            crate::expressive::test_util::goertzel_power(w, 660.0, SR)
                / crate::expressive::test_util::goertzel_power(w, 220.0, SR)
        };
        let first = h3(&s[2205..20000]); // morph 1.0 (warm)
        let second = h3(&s[24255..42000]); // morph 0.0 (basic)
        assert!(
            crate::expressive::test_util::db(first / second) > 10.0,
            "{first} vs {second}"
        );
    }

    #[test]
    fn an_inactive_lfo_is_identity() {
        let with = patch(
            json!({"name": "a", "subtractive": {"env": {"release": 0.01}},
            "lfo": {"rate": 5.0, "depth": 0.0, "target": "pitch"}}),
        );
        let without = patch(json!({"name": "b", "subtractive": {"env": {"release": 0.01}}}));
        let a = left(&render_patch(&with, &[note(0.0, 0.5, 220.0)], SR));
        let b = left(&render_patch(&without, &[note(0.0, 0.5, 220.0)], SR));
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6));
    }

    #[test]
    fn granular_engine_renders_true_stereo() {
        let p = patch(
            json!({"name": "g", "granular": {"stereo_width": 1.0, "randomness": 0.5,
            "density": 30, "env": {"attack": 0.001, "release": 0.2}}}),
        );
        let buf = render_patch(&p, &[note(0.0, 1.0, 220.0)], SR);
        let l: Vec<f32> = buf.iter().map(|s| s[0]).collect();
        let r: Vec<f32> = buf.iter().map(|s| s[1]).collect();
        assert!(rms(&l[4410..44100]) > 0.05);
        let diff: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.02, "left and right differ");
        assert_eq!(buf.len(), (1.2 * SR) as usize, "gate + release");
    }

    #[test]
    fn grain_density_lfo_reaches_the_granular_voice() {
        let mk = |lfo: serde_json::Value| {
            patch(
                json!({"name": "g", "granular": {"density": 4, "grain_ms": 20,
            "randomness": 0.0, "stereo_width": 0.0, "env": {"attack": 0.001, "release": 0.01}}, "lfo": lfo}),
            )
        };
        let steady = mk(json!({"target": "off"}));
        let pumped =
            mk(json!({"rate": 0.5, "depth": 1.0, "wave": "square", "target": "grain_density"}));
        let a = left(&render_patch(&steady, &[note(0.0, 2.0, 220.0)], SR));
        let b = left(&render_patch(&pumped, &[note(0.0, 2.0, 220.0)], SR));
        // Square LFO at 0.5 Hz: first second at 2x density, second second at 0.5x.
        let first = rms(&b[..44100]);
        let second = rms(&b[44100..88200]);
        assert!(
            first > second * 1.3,
            "denser first half: {first} vs {second}"
        );
        let sa = rms(&a[..44100]);
        let sb = rms(&a[44100..88200]);
        assert!(
            (sa / sb - 1.0).abs() < 0.3,
            "steady without the LFO: {sa} vs {sb}"
        );
    }
}
