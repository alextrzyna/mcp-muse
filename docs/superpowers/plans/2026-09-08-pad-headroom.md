# Pad Headroom (issue #109) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop pad chords from clipping the engine's soft clipper, make the built-in headroom test hear what `test-synths` plays, balance the drum kit against the synths, and protect agent-defined patches with a per-patch peak limiter.

**Architecture:** A small `PeakLimiter` (instant attack, ~50 ms release, fixed ceiling) sits at the end of `render_patch`'s per-sample loop, after the effects chain and before `SYNTH_BUS_GAIN`; below the ceiling its gain is exactly 1.0 so quiet material is bit-identical. Percussion hits are peak-normalised once at note-on so `level` means the same thing for every kind. The library headroom test renders each category's demo phrase plus a sustained full-velocity note and asserts a ceiling that the limiter cannot reach, so it still forces sensible patch levels. A level pass on the JSON patches, set by measurement, closes the pad loudness spread and lifts the kit.

**Tech Stack:** Rust 2024, existing `render_patch`, `test_util` measurements (`rms`, `db`), serde JSON patches.

**Spec:** GitHub issue #109 ("Pad chords clip the soft clipper; add polyphony headroom") and its 2026-09-07 comment; the design approved in chat on 2026-09-08 (chord-aware test, per-patch limiter, level pass; out of scope: MIDI bus loudness, lookahead limiting, the soft clipper itself).

## Global Constraints

- DSP is verified by measurement (`src/expressive/test_util.rs`), never by ear.
- Never allocate or log in per-sample loops; `render_patch` runs on the tool thread, but its per-sample loop must stay allocation-free.
- `SYNTH_BUS_GAIN` = 0.5 is applied in `translate.rs` after `render_patch`; the engine soft clipper knee is 0.8. So a patch buffer must peak ≤ 1.6 to stay clean; the limiter ceiling is **1.5** (`PATCH_LIMITER_CEILING`) and the headroom test's ceiling is **1.4** (`HEADROOM_CEILING`, i.e. 0.7 after the bus gain), which the limiter cannot produce, so the test catches patches that lean on the limiter.
- Material that never crosses the ceiling must render bit-identically with the limiter in place.
- Level bands for built-ins (pre-bus peak over the category's demo phrase at velocity 100/127): pads 0.7–1.4 (chord), drums 0.5–1.4 (two hits), every other category 0.3–1.4; every patch's 3 s single note at velocity 1.0 ≤ 1.4. Levels are set by measurement to land inside these bands, not guessed.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` (run `rustup update stable` first) and `cargo test` pass before every commit.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jxcj13gt2pyZbmDvvC1Pep
  ```
- Work in the worktree `.claude/worktrees/fix-pad-headroom` (branch `worktree-fix-pad-headroom`, from `main` 7af6672).

---

## File map

| File | Responsibility |
|---|---|
| `src/expressive/limiter.rs` (new) | `PeakLimiter` and its measurement tests |
| `src/expressive/mod.rs` | `pub mod limiter;` |
| `src/expressive/render.rs` | limiter in the render loop; `PATCH_LIMITER_CEILING`; tests |
| `src/expressive/engines/percussion.rs` | peak-normalise the hit at note-on |
| `src/expressive/patch.rs` (tests) | chord-aware headroom test + ignored survey test |
| `src/expressive/patches/*.json` | level pass |
| `CLAUDE.md`, `README.md` | one line each on the limiter and normalisation |

---

### Task 1: Peak limiter at the end of `render_patch`

**Files:**
- Create: `src/expressive/limiter.rs`
- Modify: `src/expressive/mod.rs`, `src/expressive/render.rs`

**Interfaces:**
- Produces:
  ```rust
  // src/expressive/limiter.rs
  #[derive(Debug, Clone)]
  pub struct PeakLimiter { ceiling: f32, release: f32, gain: f32 }
  impl PeakLimiter {
      pub fn new(ceiling: f32, release_seconds: f32, sample_rate: f32) -> Self;
      #[inline] pub fn process(&mut self, l: f32, r: f32) -> (f32, f32);
      pub fn gain(&self) -> f32;   // current gain, 1.0 when idle
  }
  // src/expressive/render.rs
  pub const PATCH_LIMITER_CEILING: f32 = 1.5;
  pub const PATCH_LIMITER_RELEASE_SECONDS: f32 = 0.05;
  ```

- [ ] **Step 1: Write the failing tests**

`src/expressive/limiter.rs` (tests module at the bottom of the new file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::sine;

    const SR: f32 = 44100.0;

    fn run(lim: &mut PeakLimiter, input: &[f32]) -> Vec<f32> {
        input.iter().map(|&x| lim.process(x, x).0).collect()
    }

    #[test]
    fn output_never_exceeds_the_ceiling() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let out = run(&mut lim, &sine(220.0, 0.5, SR, 3.0));
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 1.5 + 1e-6, "peak {peak}");
        assert!(peak > 1.4, "the limiter holds the signal near the ceiling, not far under it: {peak}");
    }

    #[test]
    fn signal_under_the_ceiling_is_bit_identical() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let input = sine(220.0, 0.5, SR, 1.49);
        let out = run(&mut lim, &input);
        assert_eq!(out, input);
        assert_eq!(lim.gain(), 1.0);
    }

    #[test]
    fn gain_recovers_after_the_loud_part_ends() {
        let mut lim = PeakLimiter::new(1.5, 0.05, SR);
        let mut input = sine(220.0, 0.2, SR, 3.0);
        input.extend(sine(220.0, 0.5, SR, 0.3));
        let out = run(&mut lim, &input);
        let loud_end = (0.2 * SR) as usize;
        // Right after the burst the gain is still ~0.5, so the quiet part is attenuated...
        let early = &out[loud_end..loud_end + 100];
        assert!(early.iter().fold(0.0f32, |m, x| m.max(x.abs())) < 0.25);
        // ...and within five release times it is back to unity.
        let late = &out[loud_end + (0.3 * SR) as usize..];
        let late_peak = late.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((late_peak - 0.3).abs() < 0.01, "recovered to {late_peak}");
    }

    #[test]
    fn stereo_uses_the_louder_channel_and_keeps_the_image() {
        let mut lim = PeakLimiter::new(1.0, 0.05, SR);
        let (l, r) = lim.process(2.0, 0.5);
        assert!((l - 1.0).abs() < 1e-6);
        assert!((r - 0.25).abs() < 1e-6, "same gain on both channels: {r}");
    }
}
```

`src/expressive/render.rs` tests:

```rust
    #[test]
    fn a_hot_chord_is_limited_before_the_bus() {
        // Three saws at level 1.0 sum well past the ceiling.
        let p = patch(json!({"name": "hot", "level": 1.0,
            "subtractive": {"osc1": {"wave": "saw"}, "env": {"attack": 0.001, "release": 0.05}}}));
        let chord = [note(0.0, 0.5, 130.81), note(0.0, 0.5, 196.0), note(0.0, 0.5, 261.63)];
        let buf = render_patch(&p, &chord, SR, 120);
        let peak = buf.iter().flat_map(|s| s.iter()).fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= PATCH_LIMITER_CEILING + 1e-6, "peak {peak}");
        assert!(peak > 1.4, "the chord actually drove the limiter: {peak}");
    }

    #[test]
    fn a_quiet_note_is_unchanged_by_the_limiter() {
        let p = patch(json!({"name": "q", "level": 0.3,
            "subtractive": {"osc1": {"wave": "sine"}, "env": {"attack": 0.01, "release": 0.05}}}));
        let n = [note(0.0, 0.5, 220.0)];
        let buf = render_patch(&p, &n, SR, 120);
        let peak = buf.iter().flat_map(|s| s.iter()).fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.5);
        // Same render through a limiter is a no-op: compare against a manual pass.
        let mut lim = crate::expressive::limiter::PeakLimiter::new(
            PATCH_LIMITER_CEILING, PATCH_LIMITER_RELEASE_SECONDS, SR);
        for s in &buf {
            let (l, r) = lim.process(s[0], s[1]);
            assert_eq!((l, r), (s[0], s[1]));
        }
        assert_eq!(lim.gain(), 1.0);
    }
```

- [ ] **Step 2: Run to verify failure**

`cargo test limiter render:: 2>&1 | tail`. Expected: `limiter` module missing / constants undefined.

- [ ] **Step 3: Implement**

`src/expressive/limiter.rs`:

```rust
//! A gentle per-patch peak limiter: instant attack, exponential release,
//! fixed ceiling. Below the ceiling the gain is exactly 1.0, so quiet
//! material passes through bit-identically.

#[derive(Debug, Clone)]
pub struct PeakLimiter {
    ceiling: f32,
    /// Per-sample release coefficient toward unity gain.
    release: f32,
    gain: f32,
}

impl PeakLimiter {
    pub fn new(ceiling: f32, release_seconds: f32, sample_rate: f32) -> Self {
        let samples = (release_seconds * sample_rate).max(1.0);
        Self {
            ceiling: ceiling.max(1e-3),
            release: 1.0 - (-1.0 / samples).exp(),
            gain: 1.0,
        }
    }

    /// Current gain: 1.0 when idle, below 1.0 while recovering from a peak.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Limits a stereo frame with one shared gain so the image is preserved.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs());
        let target = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };
        self.gain = if target < self.gain {
            target
        } else {
            (self.gain + (1.0 - self.gain) * self.release).min(target)
        };
        (l * self.gain, r * self.gain)
    }
}
```

Note the idle case: `target == 1.0`, `gain == 1.0` → `(1.0 + 0.0 * release).min(1.0) == 1.0` and `l * 1.0 == l` exactly, which is what the bit-identical tests rely on.

`src/expressive/mod.rs`: add `pub mod limiter;` next to the other modules.

`src/expressive/render.rs`: add the two constants near `MAX_RENDER_SECONDS` with a comment ("`SYNTH_BUS_GAIN` 0.5 × 1.5 = 0.75, under the engine's 0.8 soft-clip knee"), create `let mut limiter = PeakLimiter::new(PATCH_LIMITER_CEILING, PATCH_LIMITER_RELEASE_SECONDS, sample_rate);` before the loop, and change the frame write to `*frame = { let (l, r) = (chain_l.process(l), chain_r.process(r)); limiter.process(l, r).into() }` (or two lines; keep it allocation-free). Import `crate::expressive::limiter::PeakLimiter`.

- [ ] **Step 4: Run the tests**

`cargo test 2>&1 | grep -E "^test result|panicked|FAILED"` — all pass (baseline 199 unit + 37 integration, plus 6).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/expressive/limiter.rs src/expressive/mod.rs src/expressive/render.rs
git commit -m "feat(render): per-patch peak limiter before the bus gain"
```

---

### Task 2: Peak-normalise percussion hits

**Files:**
- Modify: `src/expressive/engines/percussion.rs`

**Interfaces:**
- Produces: `pub const PERCUSSION_PEAK: f32 = 0.9;` in `engines/percussion.rs`; `PercussionVoice::new` normalises the rendered hit so its absolute peak is `PERCUSSION_PEAK` before `level` applies (a silent render stays silent).

- [ ] **Step 1: Write the failing tests**

In `src/expressive/engines/percussion.rs` tests (next to the existing `level applied` test; update that test's upper bound to `0.5 * PERCUSSION_PEAK + 1e-4`):

```rust
    #[test]
    fn every_kind_is_normalised_to_the_same_peak() {
        for kind in ["kick", "snare", "hihat", "cymbal", "zap", "swoosh", "chime", "burst"] {
            let cfg: Percussion =
                serde_json::from_str(&format!(r#"{{"kind": "{kind}", "level": 1.0}}"#)).unwrap();
            let mut v = PercussionVoice::new(&cfg, 0.5, 44100.0);
            let mods = Modulation::default();
            let mut peak = 0.0f32;
            while v.is_active() {
                peak = peak.max(v.tick(&mods).0.abs());
            }
            assert!((peak - PERCUSSION_PEAK).abs() < 0.02, "{kind} peaks at {peak}");
        }
    }
```

If a kind in that list is not a valid `kind` value, use the exact `PercussionKind` serde names from `src/expressive/patch.rs` instead.

- [ ] **Step 2: Run to verify failure**

`cargo test engines::percussion 2>&1 | tail`. Expected: kick far below 0.9, cymbal possibly above.

- [ ] **Step 3: Implement**

```rust
/// Every hit is normalised to this peak before `level` applies, so `level`
/// means the same loudness for a kick as for a cymbal.
pub const PERCUSSION_PEAK: f32 = 0.9;

fn normalise_peak(samples: &mut [f32], target: f32) {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 1e-6 {
        let g = target / peak;
        for s in samples.iter_mut() {
            *s *= g;
        }
    }
}
```

Call `normalise_peak(&mut samples, PERCUSSION_PEAK);` in `PercussionVoice::new` after `percussion::render` and before `fade_out` (the fade only touches the tail, so the peak is measured on the whole hit). This runs once per note-on, not per sample.

- [ ] **Step 4: Run the tests**

`cargo test 2>&1 | grep -E "^test result|panicked|FAILED"`. The built-in library test may now fail on drum or fx patches being too hot (e.g. `crash_cymbal` at level 0.7 → 0.63 pre-bus is fine, but check `burst`/`chime`); if any built-in exceeds `peak * 0.5 <= 0.8`, lower that patch's `level` in its JSON in this task so the suite stays green, and note it in the report (Task 3 does the full level pass).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/expressive/engines/percussion.rs src/expressive/patches
git commit -m "feat(percussion): normalise every hit to the same peak so level is comparable"
```

---

### Task 3: Chord-aware headroom test and the level pass

**Files:**
- Modify: `src/expressive/patch.rs` (tests), `src/expressive/patches/*.json`

**Interfaces:**
- Consumes: `PATCH_LIMITER_CEILING` (Task 1), `PERCUSSION_PEAK` (Task 2).
- Produces: in `patch.rs` tests, `const HEADROOM_CEILING: f32 = 1.4;`, `fn demo_phrase(category: PatchCategory) -> &'static [(u8, f32, f32)]`, `fn phrase_events(p: &Patch, phrase: &[(u8, f32, f32)], velocity: f32) -> Vec<NoteEvent>`, `fn peak(buf: &[[f32; 2]]) -> f32`; the rewritten `every_builtin_patch_parses_validates_and_renders_cleanly`; an `#[ignore]` survey test `print_builtin_headroom_survey`.

- [ ] **Step 1: Write the survey and the failing test**

Replace the body of `every_builtin_patch_parses_validates_and_renders_cleanly` in `src/expressive/patch.rs`:

```rust
    /// The phrases `cargo run -- test-synths` plays, so the test hears what the user hears.
    fn demo_phrase(category: PatchCategory) -> &'static [(u8, f32, f32)] {
        match category {
            PatchCategory::Drums | PatchCategory::Fx => &[(36, 0.0, 0.5), (36, 0.5, 0.5)],
            PatchCategory::Pad => &[(48, 0.0, 3.0), (55, 0.0, 3.0), (60, 0.0, 3.0)],
            PatchCategory::Keys => &[(60, 0.0, 0.6), (64, 0.7, 0.6), (67, 1.4, 1.2)],
            _ => &[(36, 0.0, 0.4), (43, 0.5, 0.4), (48, 1.0, 0.8)],
        }
    }

    fn phrase_events(p: &Patch, phrase: &[(u8, f32, f32)], velocity: f32) -> Vec<NoteEvent> {
        phrase
            .iter()
            .map(|&(n, start, duration)| NoteEvent {
                start,
                duration,
                frequency: if p.has_pitched_engine() {
                    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
                } else {
                    60.0
                },
                velocity,
            })
            .collect()
    }

    fn peak(buf: &[[f32; 2]]) -> f32 {
        buf.iter().flat_map(|s| s.iter()).fold(0.0f32, |m, x| m.max(x.abs()))
    }

    /// Ceiling for built-ins over their demo phrase, before `SYNTH_BUS_GAIN`.
    /// Below `PATCH_LIMITER_CEILING` on purpose: a built-in must not lean on the limiter.
    const HEADROOM_CEILING: f32 = 1.4;

    /// Lower bound per category so a patch is not lost in a mix (pre-bus peak).
    fn headroom_floor(category: PatchCategory) -> f32 {
        match category {
            PatchCategory::Pad => 0.7,
            PatchCategory::Drums => 0.5,
            _ => 0.3,
        }
    }

    #[test]
    fn every_builtin_patch_parses_validates_and_renders_cleanly() {
        use crate::expressive::render::{PATCH_LIMITER_CEILING, render_patch};
        assert!(HEADROOM_CEILING < PATCH_LIMITER_CEILING);
        let lib = PatchLibrary::new();
        assert!(lib.count() >= 44, "expected the built-ins, got {}", lib.count());
        for name in lib.names() {
            let p = lib.get(name).unwrap();
            p.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
            let category = p.category.unwrap_or_else(|| panic!("{name} needs a category"));
            assert!(!p.description.is_empty(), "{name} needs a description");

            let phrase = render_patch(p, &phrase_events(p, demo_phrase(category), 100.0 / 127.0), 44100.0, 120);
            let phrase_peak = peak(&phrase);
            assert!(phrase_peak.is_finite(), "{name} produced NaN/inf");
            assert!(
                phrase_peak <= HEADROOM_CEILING,
                "{name} peaks at {phrase_peak} over its demo phrase; lower its level"
            );
            assert!(
                phrase_peak >= headroom_floor(category),
                "{name} peaks at only {phrase_peak} over its demo phrase; raise its level"
            );

            // A long note at full velocity catches slow pads measured mid-attack by the phrase.
            let held = render_patch(p, &phrase_events(p, &[(60, 0.0, 3.0)], 1.0), 44100.0, 120);
            let held_peak = peak(&held);
            assert!(
                held_peak <= HEADROOM_CEILING,
                "{name} peaks at {held_peak} on a held full-velocity note; lower its level"
            );
        }
    }

    /// `cargo test print_builtin_headroom_survey -- --ignored --nocapture`
    /// prints pre-bus peak and RMS per patch; use it to set levels.
    #[test]
    #[ignore]
    fn print_builtin_headroom_survey() {
        use crate::expressive::render::render_patch;
        use crate::expressive::test_util::{db, rms};
        let lib = PatchLibrary::new();
        for (category, patches) in lib.catalog() {
            println!("## {}", category.as_str());
            for p in patches {
                let buf = render_patch(p, &phrase_events(p, demo_phrase(category), 100.0 / 127.0), 44100.0, 120);
                let mono: Vec<f32> = buf.iter().map(|s| 0.5 * (s[0] + s[1])).collect();
                let held = render_patch(p, &phrase_events(p, &[(60, 0.0, 3.0)], 1.0), 44100.0, 120);
                println!(
                    "{:<22} level {:<5} phrase peak {:.2} rms {:>6.1} dB   held peak {:.2}",
                    p.name, p.level, peak(&buf), db(rms(&mono)), peak(&held)
                );
            }
        }
    }
```

Keep the existing `use` lines the module needs (`NoteEvent` from `crate::expressive::render`); `PatchCategory::as_str` and `PatchLibrary::catalog` already exist.

- [ ] **Step 2: Run to verify failure and survey**

```bash
cargo test every_builtin_patch 2>&1 | tail -5
cargo test print_builtin_headroom_survey -- --ignored --nocapture 2>&1 | grep -v "^test \|running\|^$"
```

Expected: the test fails on the hot pads (warm_pad, analog_wash, ob_brass, …) and possibly on quiet patches below the floor. Paste the survey table into the report.

- [ ] **Step 3: Level pass**

Edit `level` in the JSON patches until every built-in lands inside the bands (use the survey; re-run after each batch). Rules:
- Pads: target phrase peak 0.9–1.2 (mid-band). Expect warm_pad, analog_wash, ob_brass around 0.35–0.45, the other subtractive pads around 0.45–0.6; raise `wt_vocal_pad`, `grain_cloud`, `formant_texture`, `drone` only if they are under the 0.7 floor (do not raise `formant_texture` above 1.0; if it is under the floor at 1.0, leave it and say so).
- Drums: target hit peak 0.9–1.2. With normalisation a hit at `level` L peaks ≈ 0.9 × L × 0.79; set kick 1.0, snare 0.9, hi-hats 0.6–0.7, crash 0.8 and confirm with the survey.
- Fx, keys, bass, lead: only touch patches outside their band.
- Change nothing but `level`.

- [ ] **Step 4: Run the tests**

`cargo test 2>&1 | grep -E "^test result|panicked|FAILED"` — all green, and re-run the survey once more for the report.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/expressive/patch.rs src/expressive/patches
git commit -m "test(patches): chord-aware headroom test; level pass for pads and drums"
```

---

### Task 4: Docs

**Files:**
- Modify: `CLAUDE.md`, `README.md`

- [ ] **Step 1: Edit**

- `CLAUDE.md` pipeline item 2: after "the patch's effects chain applied once" add ", a per-patch peak limiter (`PATCH_LIMITER_CEILING` 1.5, so chords stay under the 0.8 soft-clip knee after `SYNTH_BUS_GAIN`)". In the Synthesis list add to the `percussion.rs` bullet: "hits are peak-normalised to `PERCUSSION_PEAK` at note-on so `level` is comparable across kinds." Add one bullet under Important Architectural Decisions: "**Headroom is measured, not assumed**: the library test renders each category's demo phrase (a chord for pads) and a held full-velocity note and asserts a 1.4 pre-bus ceiling below the limiter's 1.5; set levels with `cargo test print_builtin_headroom_survey -- --ignored --nocapture`."
- `README.md`: in the engines/rendering section, one sentence that patches are rendered through a gentle peak limiter so chords on any patch, including agent-defined ones, do not clip.

- [ ] **Step 2: Verify and commit**

```bash
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test 2>&1 | grep -E "^test result"
git add CLAUDE.md README.md
git commit -m "docs: per-patch limiter, percussion normalisation, measured headroom"
```

---

## Self-review notes

- Issue coverage: proposed fix 1 (chord-aware test + pad levels) → Task 3; fix 2 (limiter) → Task 1; fix 3 (drum levels) → Tasks 2 and 3; comment's sustained-note guard → Task 3 held note; pad loudness spread → Task 3 floor/ceiling bands.
- Names consistent: `PeakLimiter::{new, process, gain}`, `PATCH_LIMITER_CEILING`, `PATCH_LIMITER_RELEASE_SECONDS`, `PERCUSSION_PEAK`, `HEADROOM_CEILING`, `demo_phrase`, `phrase_events`, `peak`, `headroom_floor`, `print_builtin_headroom_survey`.
- Out of scope: MIDI bus loudness, lookahead limiting, soft clipper changes, `PatchLibrary` rebuilt per call.
