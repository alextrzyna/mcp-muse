# Agent-Defined Synths, PR 1 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an agent define a synth patch as JSON (`define_synth` or inline on a note), render notes through it with a subtractive or percussion engine plus a shared effects chain, and remove the old flat `synth_*` / `preset_*` fields and Rust presets.

**Architecture:** New `src/expressive/{envelope,oscillator,patch,render}.rs` and `src/expressive/engines/` hold the data model and offline renderer. The translator groups synthesis notes by patch, renders one stereo buffer per group, and schedules it through the existing `MidiEngine` buffers (made stereo). The server gains `define_synth` and a `synth` field on notes; built-in patches are JSON files embedded with `include_str!`.

**Tech Stack:** Rust 2024 edition, serde/serde_json, rand 0.10, existing `Svf`/`EffectsChain` DSP, `test_util` signal measurements. Commands: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.

**Spec:** `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md` (sections 1 to 8; this plan covers PR 1 of section 8).

## Global Constraints

- Every DSP claim is verified by measurement (`src/expressive/test_util.rs`: `goertzel_power`, `rms`, `zero_crossing_rate`, `db`), never by ear.
- Never allocate or log inside per-sample loops on the audio thread (`src/midi/engine.rs`). Offline rendering on the tool thread may allocate.
- Malformed JSON shapes return JSON-RPC `-32602`; semantic problems during execution return `isError: true` with the field path and allowed range.
- `SimpleNote` and every patch struct carry `#[serde(deny_unknown_fields)]`.
- Patch JSON is snake_case. Envelope defaults are attack 0.01, decay 0.1, sustain 0.8, release 0.3.
- Internal sample rate is `SAMPLE_RATE` = 44100 (`src/midi/engine.rs`).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass before every commit.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jxcj13gt2pyZbmDvvC1Pep
  ```
- Work on a branch `feat/agent-defined-synths` created from `main` (use the `superpowers:using-git-worktrees` skill).

---

## File map

| File | Responsibility |
|---|---|
| `src/expressive/envelope.rs` (new) | `Adsr` config and `GateEnvelope` (gate-driven ADSR) |
| `src/expressive/oscillator.rs` (new) | `PhaseAccumulator`, `poly_blep`, `Wave`, `wave_sample` (moved out of `synth.rs`) |
| `src/expressive/patch.rs` (new) | `Patch`, `Subtractive`, `Osc`, `Osc2`, `Filter`, `Percussion`, `SynthRef`, `PatchCategory`, validation, `PatchLibrary` |
| `src/expressive/patches/*.json` (new) | built-in patches |
| `src/expressive/engines/mod.rs` (new) | `Voice` trait, `Modulation` |
| `src/expressive/engines/subtractive.rs` (new) | `SubtractiveVoice` |
| `src/expressive/engines/percussion.rs` (new) | `PercussionVoice` (wraps `percussion::render`) |
| `src/expressive/percussion.rs` | takes a `&Percussion` config; gains `chime` and `burst` |
| `src/expressive/render.rs` (new) | `NoteEvent`, `render_patch` |
| `src/expressive/synth.rs` | shrinks to `ExpressiveSynth` R2D2 rendering only |
| `src/expressive/presets/` | deleted |
| `src/expressive/mod.rs` | module list |
| `src/midi/mod.rs` | `SimpleNote` loses `synth_*`/`preset_*`, gains `synth: Option<SynthRef>`, `deny_unknown_fields` |
| `src/midi/engine.rs` | stereo buffers |
| `src/midi/translate.rs` | patch resolution, per-patch render, no presets |
| `src/midi/player.rs` | `play` takes session patches |
| `src/server/mcp.rs` | `define_synth`, `synth` in note schema, `list_sounds` synths section |
| `src/demos.rs`, `src/main.rs` | demos rewritten around patches; three commands remain |
| `tests/integration/mcp_protocol.rs` | updated and new integration tests |
| `README.md`, `CLAUDE.md` | docs |

---

### Task 1: Gate-driven ADSR envelope

**Files:**
- Create: `src/expressive/envelope.rs`
- Modify: `src/expressive/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Adsr { pub attack: f32, pub decay: f32, pub sustain: f32, pub release: f32 }  // serde, Default
  pub struct GateEnvelope;
  impl GateEnvelope {
      pub fn new(params: &Adsr, sample_rate: f32) -> Self;
      pub fn gate_off(&mut self);
      pub fn is_active(&self) -> bool;   // false once release has finished
      pub fn next(&mut self) -> f32;     // 0..1, advances one sample
  }
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/envelope.rs` with only the tests module for now:

```rust
//! Gate-driven ADSR: attack, decay and sustain while the gate is open,
//! release once it closes. Segments are linear.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 1000.0;

    fn env(a: f32, d: f32, s: f32, r: f32) -> GateEnvelope {
        GateEnvelope::new(
            &Adsr {
                attack: a,
                decay: d,
                sustain: s,
                release: r,
            },
            SR,
        )
    }

    #[test]
    fn default_adsr_matches_the_spec() {
        let d = Adsr::default();
        assert_eq!((d.attack, d.decay, d.sustain, d.release), (0.01, 0.1, 0.8, 0.3));
    }

    #[test]
    fn attack_reaches_full_level_then_decays_to_sustain() {
        let mut e = env(0.1, 0.1, 0.5, 0.1);
        let attack: Vec<f32> = (0..100).map(|_| e.next()).collect();
        assert!(attack[0] < 0.05, "starts near zero: {}", attack[0]);
        assert!((attack[99] - 1.0).abs() < 0.02, "peaks at 1: {}", attack[99]);
        for _ in 0..100 {
            e.next();
        }
        let sustain = e.next();
        assert!((sustain - 0.5).abs() < 0.02, "sustains at 0.5: {}", sustain);
        assert!(e.is_active());
    }

    #[test]
    fn release_runs_from_the_current_level_and_then_deactivates() {
        let mut e = env(0.01, 0.01, 0.6, 0.1);
        for _ in 0..50 {
            e.next();
        }
        e.gate_off();
        let first = e.next();
        assert!(first < 0.6 && first > 0.55, "release starts from sustain: {first}");
        for _ in 0..100 {
            e.next();
        }
        assert_eq!(e.next(), 0.0);
        assert!(!e.is_active());
    }

    #[test]
    fn gate_off_during_attack_releases_from_the_partial_level() {
        let mut e = env(1.0, 0.1, 0.8, 0.1);
        for _ in 0..500 {
            e.next();
        }
        e.gate_off();
        let level = e.next();
        assert!(level < 0.5 && level > 0.45, "released from ~0.5: {level}");
    }

    #[test]
    fn zero_length_segments_do_not_divide_by_zero() {
        let mut e = env(0.0, 0.0, 1.0, 0.0);
        assert!((e.next() - 1.0).abs() < 0.01);
        e.gate_off();
        e.next();
        assert!(!e.is_active());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Add `pub mod envelope;` and `pub use envelope::*;` to `src/expressive/mod.rs`. Run:

```bash
cargo test envelope 2>&1 | tail -20
```

Expected: compile errors, `Adsr` and `GateEnvelope` not found.

- [ ] **Step 3: Implement**

Add above the tests module in `src/expressive/envelope.rs`:

```rust
/// Envelope times in seconds (0.001 to 10) and sustain level 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Adsr {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.3,
        }
    }
}

impl Adsr {
    /// Field-path error for an out-of-range value; `path` is e.g. "subtractive.env".
    pub fn validate(&self, path: &str) -> Result<(), String> {
        for (name, value) in [
            ("attack", self.attack),
            ("decay", self.decay),
            ("release", self.release),
        ] {
            if !(0.0..=10.0).contains(&value) {
                return Err(format!("{path}.{name} must be 0 to 10 seconds, got {value}"));
            }
        }
        if !(0.0..=1.0).contains(&self.sustain) {
            return Err(format!("{path}.sustain must be 0 to 1, got {}", self.sustain));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Idle,
}

/// Shortest segment we step through, so a zero-length stage completes in one sample.
const MIN_SEGMENT_SECONDS: f32 = 0.0005;

#[derive(Debug, Clone)]
pub struct GateEnvelope {
    attack_inc: f32,
    decay_dec: f32,
    sustain: f32,
    release_samples: f32,
    stage: Stage,
    level: f32,
    release_from: f32,
    release_pos: f32,
}

impl GateEnvelope {
    pub fn new(params: &Adsr, sample_rate: f32) -> Self {
        let seg = |seconds: f32| seconds.max(MIN_SEGMENT_SECONDS) * sample_rate;
        let sustain = params.sustain.clamp(0.0, 1.0);
        Self {
            attack_inc: 1.0 / seg(params.attack),
            decay_dec: (1.0 - sustain) / seg(params.decay),
            sustain,
            release_samples: seg(params.release),
            stage: Stage::Attack,
            level: 0.0,
            release_from: 0.0,
            release_pos: 0.0,
        }
    }

    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.release_from = self.level;
            self.release_pos = 0.0;
            self.stage = Stage::Release;
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= self.decay_dec;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.release_pos += 1.0;
                self.level = self.release_from * (1.0 - self.release_pos / self.release_samples);
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {}
        }
        self.level
    }
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test envelope 2>&1 | tail -20
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/envelope.rs src/expressive/mod.rs
git commit -m "feat(expressive): gate-driven ADSR envelope"
```

---

### Task 2: Oscillator module with a `Wave` enum

Move `PhaseAccumulator` and `poly_blep` out of `synth.rs` into their own module and add a waveform function the subtractive engine uses.

**Files:**
- Create: `src/expressive/oscillator.rs`
- Modify: `src/expressive/synth.rs` (remove `PhaseAccumulator`, `poly_blep`; import them), `src/expressive/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct PhaseAccumulator; // new(sample_rate), next_phase(freq)->radians, next(freq)->sin, next_unit(freq)->0..1 (now pub)
  pub fn poly_blep(t: f32, dt: f32) -> f32;
  #[derive(Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum Wave { Sine, Saw, Square, Triangle, Noise }   // Default = Saw
  pub fn wave_sample(wave: Wave, phase: f32, dt: f32, pulse_width: f32, rng: &mut impl rand::Rng) -> f32;
  ```
  `phase` is the unit phase in 0..1 from `next_unit`; `dt` = `freq / sample_rate`.

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/oscillator.rs` with the tests module:

```rust
//! Phase accumulation and band-limited basic waveforms shared by the engines.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn render(wave: Wave, freq: f32, seconds: f32) -> Vec<f32> {
        let mut acc = PhaseAccumulator::new(SR);
        let mut rng = rand::rng();
        let dt = freq / SR;
        (0..(seconds * SR) as usize)
            .map(|_| wave_sample(wave, acc.next_unit(freq), dt, 0.5, &mut rng))
            .collect()
    }

    #[test]
    fn every_pitched_wave_has_the_requested_fundamental() {
        for wave in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle] {
            let s = render(wave, 220.0, 1.0);
            let rate = zero_crossing_rate(&s, SR);
            assert!((rate - 440.0).abs() < 10.0, "{wave:?}: {rate} crossings/s");
        }
    }

    #[test]
    fn square_is_odd_harmonics_only() {
        let s = render(Wave::Square, 200.0, 1.0);
        let odd = goertzel_power(&s, 600.0, SR);
        let even = goertzel_power(&s, 400.0, SR);
        assert!(db(odd / even) > 20.0, "odd/even = {} dB", db(odd / even));
    }

    #[test]
    fn polyblep_saw_aliases_less_than_a_naive_saw() {
        let freq = 4000.0;
        let s = render(Wave::Saw, freq, 1.0);
        let mut acc = PhaseAccumulator::new(SR);
        let naive: Vec<f32> = (0..SR as usize)
            .map(|_| 2.0 * acc.next_unit(freq) - 1.0)
            .collect();
        // 44100 - 12000 = 32100 -> aliases to 12000 - (32100 - 22050) ... measure a
        // known alias: the 6th harmonic (24000) folds to 20100.
        let alias = 20100.0;
        assert!(
            goertzel_power(&s, alias, SR) < goertzel_power(&naive, alias, SR) * 0.5,
            "PolyBLEP should reduce the folded harmonic"
        );
    }

    #[test]
    fn noise_is_broadband_and_bounded() {
        let s = render(Wave::Noise, 440.0, 0.5);
        assert!(s.iter().all(|x| x.abs() <= 1.0));
        let low = goertzel_power(&s, 500.0, SR);
        let high = goertzel_power(&s, 9000.0, SR);
        assert!(db(low / high).abs() < 15.0, "white noise is roughly flat");
    }

    #[test]
    fn wave_names_are_snake_case() {
        let w: Wave = serde_json::from_str("\"saw\"").unwrap();
        assert_eq!(w, Wave::Saw);
        assert!(serde_json::from_str::<Wave>("\"Sawtooth\"").is_err());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod oscillator;` and `pub use oscillator::*;` to `src/expressive/mod.rs`. Run `cargo test oscillator 2>&1 | tail`. Expected: compile errors for missing items.

- [ ] **Step 3: Move and implement**

Cut `PhaseAccumulator` (struct, impl) and `poly_blep` from `src/expressive/synth.rs` and paste them into `oscillator.rs` above the tests, making `next_unit` and `poly_blep` `pub`. In `synth.rs`, add `use crate::expressive::oscillator::{PhaseAccumulator, poly_blep};` (drop `poly_blep` from the import if `synth.rs` no longer references it after Task 10; for now it does). Then add:

```rust
/// Basic oscillator shapes. `noise` ignores pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    Sine,
    #[default]
    Saw,
    Square,
    Triangle,
    Noise,
}

/// One sample of `wave` at unit phase `phase` (0..1). `dt` is the phase
/// increment per sample (`freq / sample_rate`) for the PolyBLEP correction.
#[inline]
pub fn wave_sample(wave: Wave, phase: f32, dt: f32, pulse_width: f32, rng: &mut impl Rng) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Saw => 2.0 * phase - 1.0 - poly_blep(phase, dt),
        Wave::Square => {
            let pw = pulse_width.clamp(0.05, 0.95);
            let naive = if phase < pw { 1.0 } else { -1.0 };
            naive + poly_blep(phase, dt) - poly_blep((phase + 1.0 - pw).rem_euclid(1.0), dt)
        }
        Wave::Triangle => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }
        Wave::Noise => rng.random::<f32>() * 2.0 - 1.0,
    }
}
```

Delete the now-duplicate `polyblep_saw_aliases_less_than_naive` test from `synth.rs` if it only exercised `poly_blep` (it used `SynthType::Sawtooth`; leave it if it still compiles, it is removed in Task 10 anyway).

- [ ] **Step 4: Run the tests**

```bash
cargo test oscillator 2>&1 | tail -20 && cargo test 2>&1 | tail -5
```

Expected: 5 new tests pass; the whole suite still passes.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/oscillator.rs src/expressive/synth.rs src/expressive/mod.rs
git commit -m "refactor(expressive): oscillator module with Wave enum"
```

---

### Task 3: Patch data model and validation

**Files:**
- Create: `src/expressive/patch.rs`
- Modify: `src/expressive/mod.rs`

**Interfaces:**
- Consumes: `Adsr` (Task 1), `Wave` (Task 2), `crate::midi::EffectConfig`.
- Produces:
  ```rust
  #[serde(deny_unknown_fields)] pub struct Patch {
      pub name: String, pub description: String, pub category: Option<PatchCategory>,
      pub level: f32, pub subtractive: Option<Subtractive>, pub percussion: Option<Percussion>,
      pub effects: Vec<EffectConfig>,
  }
  pub enum PatchCategory { Bass, Pad, Lead, Keys, Drums, Fx }  // snake_case, as_str()
  pub struct Subtractive { pub level: f32, pub osc1: Osc, pub osc2: Option<Osc2>, pub filter: Option<Filter>, pub env: Adsr }
  pub struct Osc { pub wave: Wave, pub pulse_width: f32 }
  pub struct Osc2 { pub wave: Wave, pub pulse_width: f32, pub mix: f32, pub detune_cents: f32, pub octave: i8 }
  pub struct Filter { pub kind: FilterKind /* json "type" */, pub cutoff: f32, pub resonance: f32, pub slope: u8, pub env_amount: f32, pub env: Adsr }
  pub enum FilterKind { LowPass, HighPass, BandPass }  // snake_case
  pub struct Percussion { pub kind: PercussionKind, pub level: f32, pub frequency: Option<f32>, /* Option<f32> per parameter, see below */ }
  pub enum PercussionKind { Kick, Snare, Hihat, Cymbal, Zap, Swoosh, Chime, Burst }
  #[serde(untagged)] pub enum SynthRef { Name(String), Inline(Patch) }
  impl Patch { pub fn validate(&self) -> Result<(), String>; pub fn key(&self) -> String /* lowercase name */; pub fn release_seconds(&self) -> f32; pub fn has_pitched_engine(&self) -> bool; }
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/patch.rs` with:

```rust
//! Agent-defined synth patches: the JSON data model, validation and the
//! library of built-in patches. See docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md.

use crate::expressive::{Adsr, Wave};
use crate::midi::EffectConfig;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: serde_json::Value) -> Result<Patch, String> {
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    #[test]
    fn minimal_patch_fills_in_defaults() {
        let p = parse(json!({"name": "saw", "subtractive": {}})).unwrap();
        let sub = p.subtractive.as_ref().unwrap();
        assert_eq!(sub.osc1.wave, Wave::Saw);
        assert_eq!(sub.level, 1.0);
        assert!(sub.osc2.is_none() && sub.filter.is_none());
        assert_eq!(sub.env, Adsr::default());
        assert_eq!(p.level, 1.0);
        assert!(p.effects.is_empty());
        assert!(p.validate().is_ok());
    }

    #[test]
    fn full_subtractive_patch_round_trips() {
        let v = json!({
            "name": "warm_pad", "description": "pad", "category": "pad", "level": 0.8,
            "subtractive": {
                "level": 1.0,
                "osc1": {"wave": "saw"},
                "osc2": {"wave": "square", "pulse_width": 0.3, "mix": 0.4, "detune_cents": 12, "octave": -1},
                "filter": {"type": "low_pass", "cutoff": 800, "resonance": 0.3, "slope": 24,
                           "env_amount": 0.6, "env": {"attack": 1.5, "decay": 2.0, "sustain": 0.3, "release": 3.0}},
                "env": {"attack": 0.8, "decay": 1.0, "sustain": 0.7, "release": 2.5}
            },
            "effects": [{"type": "reverb", "room_size": 0.7, "intensity": 0.35}]
        });
        let p = parse(v.clone()).unwrap();
        assert!(p.validate().is_ok());
        let back = serde_json::to_value(&p).unwrap();
        assert_eq!(back["subtractive"]["filter"]["type"], "low_pass");
        assert_eq!(back["subtractive"]["osc2"]["octave"], -1);
        assert_eq!(back["category"], "pad");
        let again = parse(back).unwrap();
        assert_eq!(again.subtractive.unwrap().filter.unwrap().slope, 24);
    }

    #[test]
    fn unknown_fields_are_rejected_by_name() {
        let err = parse(json!({"name": "x", "subtractive": {"cutoff": 500}})).unwrap_err();
        assert!(err.contains("cutoff"), "{err}");
        let err = parse(json!({"name": "x", "synth_type": "saw"})).unwrap_err();
        assert!(err.contains("synth_type"), "{err}");
    }

    #[test]
    fn validation_names_the_field_and_the_range() {
        let p = parse(json!({"name": "x", "subtractive": {"filter": {"cutoff": 5}}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(err.contains("subtractive.filter.cutoff") && err.contains("20"), "{err}");

        let p = parse(json!({"name": "x", "subtractive": {"osc2": {"octave": 3}}})).unwrap();
        assert!(p.validate().unwrap_err().contains("subtractive.osc2.octave"));

        let p = parse(json!({"name": "x", "subtractive": {"filter": {"slope": 18}}})).unwrap();
        assert!(p.validate().unwrap_err().contains("slope"));

        let p = parse(json!({"name": "x", "subtractive": {"env": {"sustain": 2}}})).unwrap();
        assert!(p.validate().unwrap_err().contains("subtractive.env.sustain"));

        let p = parse(json!({"name": "", "subtractive": {}})).unwrap();
        assert!(p.validate().unwrap_err().contains("name"));

        let p = parse(json!({"name": "silent"})).unwrap();
        assert!(p.validate().unwrap_err().contains("no engines"));
    }

    #[test]
    fn percussion_rejects_parameters_of_another_kind() {
        let p = parse(json!({"name": "k", "percussion": {"kind": "kick", "punch": 0.9}})).unwrap();
        assert!(p.validate().is_ok());
        let p = parse(json!({"name": "k", "percussion": {"kind": "kick", "snap": 0.9}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(err.contains("snap") && err.contains("snare"), "{err}");
        let p = parse(json!({"name": "k", "percussion": {"kind": "swoosh", "sweep": [200.0, 2000.0]}})).unwrap();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn synth_ref_is_a_name_or_an_inline_patch() {
        let r: SynthRef = serde_json::from_value(json!("minimoog_bass")).unwrap();
        assert!(matches!(r, SynthRef::Name(n) if n == "minimoog_bass"));
        let r: SynthRef = serde_json::from_value(json!({"name": "x", "subtractive": {}})).unwrap();
        assert!(matches!(r, SynthRef::Inline(_)));
    }

    #[test]
    fn release_seconds_and_pitched_engine_come_from_the_engines() {
        let p = parse(json!({"name": "x", "subtractive": {"env": {"release": 2.5}}})).unwrap();
        assert_eq!(p.release_seconds(), 2.5);
        assert!(p.has_pitched_engine());
        let p = parse(json!({"name": "k", "percussion": {"kind": "kick"}})).unwrap();
        assert_eq!(p.release_seconds(), 0.0);
        assert!(!p.has_pitched_engine());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod patch;` and `pub use patch::*;` to `src/expressive/mod.rs`. `cargo test patch:: 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement the types**

Above the tests module:

```rust
fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn default_cutoff() -> f32 {
    1000.0
}
fn default_resonance() -> f32 {
    0.2
}
fn default_slope() -> u8 {
    12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchCategory {
    Bass,
    Pad,
    Lead,
    Keys,
    Drums,
    Fx,
}

impl PatchCategory {
    pub const ALL: [PatchCategory; 6] = [
        PatchCategory::Bass,
        PatchCategory::Pad,
        PatchCategory::Lead,
        PatchCategory::Keys,
        PatchCategory::Drums,
        PatchCategory::Fx,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            PatchCategory::Bass => "bass",
            PatchCategory::Pad => "pad",
            PatchCategory::Lead => "lead",
            PatchCategory::Keys => "keys",
            PatchCategory::Drums => "drums",
            PatchCategory::Fx => "fx",
        }
    }
}

/// One agent-defined instrument: engines, their envelopes and a shared effects chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<PatchCategory>,
    #[serde(default = "one")]
    pub level: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtractive: Option<Subtractive>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percussion: Option<Percussion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Subtractive {
    pub level: f32,
    pub osc1: Osc,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osc2: Option<Osc2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
    pub env: Adsr,
}

impl Default for Subtractive {
    fn default() -> Self {
        Self {
            level: 1.0,
            osc1: Osc::default(),
            osc2: None,
            filter: None,
            env: Adsr::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Osc {
    pub wave: Wave,
    pub pulse_width: f32,
}

impl Default for Osc {
    fn default() -> Self {
        Self {
            wave: Wave::Saw,
            pulse_width: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Osc2 {
    pub wave: Wave,
    pub pulse_width: f32,
    pub mix: f32,
    pub detune_cents: f32,
    pub octave: i8,
}

impl Default for Osc2 {
    fn default() -> Self {
        Self {
            wave: Wave::Saw,
            pulse_width: 0.5,
            mix: 0.5,
            detune_cents: 0.0,
            octave: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    #[default]
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    #[serde(rename = "type", default)]
    pub kind: FilterKind,
    #[serde(default = "default_cutoff")]
    pub cutoff: f32,
    #[serde(default = "default_resonance")]
    pub resonance: f32,
    #[serde(default = "default_slope")]
    pub slope: u8,
    #[serde(default)]
    pub env_amount: f32,
    #[serde(default)]
    pub env: Adsr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PercussionKind {
    Kick,
    Snare,
    Hihat,
    Cymbal,
    Zap,
    Swoosh,
    Chime,
    Burst,
}

impl PercussionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PercussionKind::Kick => "kick",
            PercussionKind::Snare => "snare",
            PercussionKind::Hihat => "hihat",
            PercussionKind::Cymbal => "cymbal",
            PercussionKind::Zap => "zap",
            PercussionKind::Swoosh => "swoosh",
            PercussionKind::Chime => "chime",
            PercussionKind::Burst => "burst",
        }
    }

    /// Default `frequency` for this kind (body, tone, base, start, fundamental or centre).
    pub fn default_frequency(&self) -> f32 {
        match self {
            PercussionKind::Kick => 60.0,
            PercussionKind::Snare => 200.0,
            PercussionKind::Hihat => 8000.0,
            PercussionKind::Cymbal => 4000.0,
            PercussionKind::Zap => 800.0,
            PercussionKind::Swoosh => 1000.0,
            PercussionKind::Chime => 880.0,
            PercussionKind::Burst => 1000.0,
        }
    }
}

/// Flat on purpose: serde cannot combine `flatten` with `deny_unknown_fields`,
/// so every kind's parameters are optional fields here and `validate`
/// rejects the ones that do not belong to `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Percussion {
    pub kind: PercussionKind,
    #[serde(default = "one")]
    pub level: f32,
    /// Body / tone / base / start / fundamental / centre frequency, by kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<f32>,
    // kick
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punch: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustain: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click_freq: Option<f32>,
    // snare
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buzz: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_amount: Option<f32>,
    // hihat, cymbal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike_intensity: Option<f32>,
    // zap
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harmonic_content: Option<f32>,
    // swoosh, burst
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intensity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<[f32; 2]>,
    // chime
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harmonic_count: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inharmonicity: Option<f32>,
    // burst
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<f32>,
}

impl Percussion {
    /// Which optional parameters each kind accepts (besides `level` and `frequency`).
    fn allowed(kind: PercussionKind) -> &'static [&'static str] {
        match kind {
            PercussionKind::Kick => &["punch", "sustain", "click_freq"],
            PercussionKind::Snare => &["snap", "buzz", "noise_amount"],
            PercussionKind::Hihat => &["metallic", "decay", "brightness"],
            PercussionKind::Cymbal => &["size", "metallic", "strike_intensity"],
            PercussionKind::Zap => &["energy", "decay", "harmonic_content"],
            PercussionKind::Swoosh => &["direction", "intensity", "sweep"],
            PercussionKind::Chime => &["harmonic_count", "decay", "inharmonicity"],
            PercussionKind::Burst => &["bandwidth", "intensity", "shape"],
        }
    }

    /// Every optional parameter that is set, by name.
    fn set_parameters(&self) -> Vec<&'static str> {
        let mut set = Vec::new();
        let f = [
            ("punch", self.punch),
            ("sustain", self.sustain),
            ("click_freq", self.click_freq),
            ("snap", self.snap),
            ("buzz", self.buzz),
            ("noise_amount", self.noise_amount),
            ("metallic", self.metallic),
            ("decay", self.decay),
            ("brightness", self.brightness),
            ("size", self.size),
            ("strike_intensity", self.strike_intensity),
            ("energy", self.energy),
            ("harmonic_content", self.harmonic_content),
            ("direction", self.direction),
            ("intensity", self.intensity),
            ("inharmonicity", self.inharmonicity),
            ("bandwidth", self.bandwidth),
            ("shape", self.shape),
        ];
        for (name, value) in f {
            if value.is_some() {
                set.push(name);
            }
        }
        if self.sweep.is_some() {
            set.push("sweep");
        }
        if self.harmonic_count.is_some() {
            set.push("harmonic_count");
        }
        set
    }

    fn validate(&self, path: &str) -> Result<(), String> {
        check_range(&format!("{path}.level"), self.level, 0.0, 1.0)?;
        if let Some(f) = self.frequency {
            check_range(&format!("{path}.frequency"), f, 20.0, 20000.0)?;
        }
        let allowed = Self::allowed(self.kind);
        for name in self.set_parameters() {
            if !allowed.contains(&name) {
                let owner = [
                    PercussionKind::Kick,
                    PercussionKind::Snare,
                    PercussionKind::Hihat,
                    PercussionKind::Cymbal,
                    PercussionKind::Zap,
                    PercussionKind::Swoosh,
                    PercussionKind::Chime,
                    PercussionKind::Burst,
                ]
                .iter()
                .find(|k| Self::allowed(**k).contains(&name))
                .map(|k| k.as_str())
                .unwrap_or("another kind");
                return Err(format!(
                    "{path}.{name} does not apply to kind '{}' (it belongs to {owner}); allowed: {}",
                    self.kind.as_str(),
                    allowed.join(", ")
                ));
            }
        }
        for (name, value) in [
            ("punch", self.punch),
            ("sustain", self.sustain),
            ("snap", self.snap),
            ("buzz", self.buzz),
            ("noise_amount", self.noise_amount),
            ("metallic", self.metallic),
            ("brightness", self.brightness),
            ("size", self.size),
            ("strike_intensity", self.strike_intensity),
            ("energy", self.energy),
            ("harmonic_content", self.harmonic_content),
            ("intensity", self.intensity),
            ("inharmonicity", self.inharmonicity),
            ("shape", self.shape),
        ] {
            if let Some(v) = value {
                check_range(&format!("{path}.{name}"), v, 0.0, 1.0)?;
            }
        }
        if let Some(d) = self.direction {
            check_range(&format!("{path}.direction"), d, -1.0, 1.0)?;
        }
        if let Some(d) = self.decay {
            check_range(&format!("{path}.decay"), d, 0.01, 10.0)?;
        }
        if let Some(c) = self.click_freq {
            check_range(&format!("{path}.click_freq"), c, 20.0, 20000.0)?;
        }
        if let Some(b) = self.bandwidth {
            check_range(&format!("{path}.bandwidth"), b, 1.0, 20000.0)?;
        }
        if let Some([a, b]) = self.sweep {
            check_range(&format!("{path}.sweep[0]"), a, 20.0, 20000.0)?;
            check_range(&format!("{path}.sweep[1]"), b, 20.0, 20000.0)?;
        }
        if let Some(n) = self.harmonic_count
            && !(1..=16).contains(&n)
        {
            return Err(format!("{path}.harmonic_count must be 1 to 16, got {n}"));
        }
        Ok(())
    }
}

/// A patch reference on a note: a stored name or a one-off inline patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SynthRef {
    Name(String),
    Inline(Patch),
}

fn check_range(path: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if value.is_nan() || value < min || value > max {
        return Err(format!("{path} must be {min} to {max}, got {value}"));
    }
    Ok(())
}

impl Patch {
    /// Lowercase name used for lookup and grouping.
    pub fn key(&self) -> String {
        self.name.trim().to_lowercase()
    }

    /// Longest amplitude release of any enabled engine; percussion has none.
    pub fn release_seconds(&self) -> f32 {
        self.subtractive
            .as_ref()
            .filter(|s| s.level > 0.0)
            .map(|s| s.env.release)
            .unwrap_or(0.0)
    }

    /// True when at least one enabled engine takes its pitch from the note.
    pub fn has_pitched_engine(&self) -> bool {
        self.subtractive.as_ref().is_some_and(|s| s.level > 0.0)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        check_range("level", self.level, 0.0, 1.0)?;
        if self.subtractive.is_none() && self.percussion.is_none() {
            return Err(format!(
                "patch '{}' has no engines: add \"subtractive\" or \"percussion\"",
                self.name
            ));
        }
        if let Some(sub) = &self.subtractive {
            check_range("subtractive.level", sub.level, 0.0, 1.0)?;
            check_range("subtractive.osc1.pulse_width", sub.osc1.pulse_width, 0.1, 0.9)?;
            if let Some(o2) = &sub.osc2 {
                check_range("subtractive.osc2.pulse_width", o2.pulse_width, 0.1, 0.9)?;
                check_range("subtractive.osc2.mix", o2.mix, 0.0, 1.0)?;
                check_range("subtractive.osc2.detune_cents", o2.detune_cents, -100.0, 100.0)?;
                if !(-2..=2).contains(&o2.octave) {
                    return Err(format!(
                        "subtractive.osc2.octave must be -2 to 2, got {}",
                        o2.octave
                    ));
                }
            }
            if let Some(f) = &sub.filter {
                check_range("subtractive.filter.cutoff", f.cutoff, 20.0, 20000.0)?;
                check_range("subtractive.filter.resonance", f.resonance, 0.0, 1.0)?;
                if f.slope != 12 && f.slope != 24 {
                    return Err(format!(
                        "subtractive.filter.slope must be 12 or 24, got {}",
                        f.slope
                    ));
                }
                check_range("subtractive.filter.env_amount", f.env_amount, -1.0, 1.0)?;
                f.env.validate("subtractive.filter.env")?;
            }
            sub.env.validate("subtractive.env")?;
        }
        if let Some(perc) = &self.percussion {
            perc.validate("percussion")?;
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test patch:: 2>&1 | tail -20
```

Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/patch.rs src/expressive/mod.rs
git commit -m "feat(expressive): patch data model with validation"
```

---

### Task 4: Percussion renders from a `Percussion` config, gains chime and burst

**Files:**
- Modify: `src/expressive/percussion.rs`
- Modify: `src/expressive/synth.rs` (temporary adapter so the old path still compiles)

**Interfaces:**
- Consumes: `Percussion`, `PercussionKind` (Task 3).
- Produces: `pub fn render(sample_rate: f32, config: &Percussion, sample_count: usize) -> Vec<f32>` (mono, already enveloped, peak roughly ≤ 1 before `level`).

- [ ] **Step 1: Write the failing tests**

Replace the tests module at the bottom of `src/expressive/percussion.rs` (keep any existing test bodies that still apply, re-expressed on the new API) with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::PercussionKind;
    use crate::expressive::test_util::{goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn cfg(kind: PercussionKind) -> Percussion {
        Percussion {
            kind,
            level: 1.0,
            frequency: None,
            punch: None,
            sustain: None,
            click_freq: None,
            snap: None,
            buzz: None,
            noise_amount: None,
            metallic: None,
            decay: None,
            brightness: None,
            size: None,
            strike_intensity: None,
            energy: None,
            harmonic_content: None,
            direction: None,
            intensity: None,
            sweep: None,
            harmonic_count: None,
            inharmonicity: None,
            bandwidth: None,
            shape: None,
        }
    }

    #[test]
    fn every_kind_renders_finite_bounded_audio() {
        for kind in [
            PercussionKind::Kick,
            PercussionKind::Snare,
            PercussionKind::Hihat,
            PercussionKind::Cymbal,
            PercussionKind::Zap,
            PercussionKind::Swoosh,
            PercussionKind::Chime,
            PercussionKind::Burst,
        ] {
            let s = render(SR, &cfg(kind), (0.5 * SR) as usize);
            assert_eq!(s.len(), (0.5 * SR) as usize);
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.5), "{kind:?}");
            assert!(rms(&s) > 0.01, "{kind:?} is silent");
        }
    }

    #[test]
    fn kick_pitch_sweeps_downward() {
        let s = render(SR, &cfg(PercussionKind::Kick), (0.5 * SR) as usize);
        let early = zero_crossing_rate(&s[..2205], SR);
        let late = zero_crossing_rate(&s[8820..13230], SR);
        assert!(early > late * 1.3, "early {early} vs late {late}");
    }

    #[test]
    fn frequency_moves_the_kick_body() {
        let mut low = cfg(PercussionKind::Kick);
        low.frequency = Some(45.0);
        let mut high = cfg(PercussionKind::Kick);
        high.frequency = Some(90.0);
        let l = render(SR, &low, (0.5 * SR) as usize);
        let h = render(SR, &high, (0.5 * SR) as usize);
        assert!(zero_crossing_rate(&h[4410..], SR) > zero_crossing_rate(&l[4410..], SR) * 1.5);
    }

    #[test]
    fn chime_has_a_partial_at_its_fundamental_that_decays() {
        let mut c = cfg(PercussionKind::Chime);
        c.frequency = Some(880.0);
        let s = render(SR, &c, SR as usize);
        let early = goertzel_power(&s[..8820], 880.0, SR);
        let late = goertzel_power(&s[35280..], 880.0, SR);
        assert!(early > goertzel_power(&s[..8820], 1100.0, SR) * 10.0, "fundamental present");
        assert!(early > late * 4.0, "decays: {early} vs {late}");
    }

    #[test]
    fn swoosh_sweeps_between_its_endpoints() {
        let mut up = cfg(PercussionKind::Swoosh);
        up.sweep = Some([200.0, 4000.0]);
        let s = render(SR, &up, SR as usize);
        let early = zero_crossing_rate(&s[..4410], SR);
        let late = zero_crossing_rate(&s[39690..], SR);
        assert!(late > early * 2.0, "rises: {early} -> {late}");
    }
}
```

- [ ] **Step 2: Run to verify failure**

`cargo test percussion 2>&1 | tail`. Expected: `render` signature mismatch errors.

- [ ] **Step 3: Implement**

In `src/expressive/percussion.rs`:

1. Change the import of `SynthType` to `use crate::expressive::{Percussion, PercussionKind};`.
2. Replace `pub fn render(sample_rate, synth_type, frequency, sample_count) -> Option<Vec<f32>>` with:

```rust
/// Render one percussive hit of `sample_count` samples. Every kind carries its
/// own envelope; the caller only applies `level`.
pub fn render(sample_rate: f32, p: &Percussion, sample_count: usize) -> Vec<f32> {
    let freq = p.frequency.unwrap_or(p.kind.default_frequency());
    match p.kind {
        PercussionKind::Kick => kick(
            sample_rate,
            p.punch.unwrap_or(0.8),
            p.sustain.unwrap_or(0.3),
            p.click_freq.unwrap_or(8000.0),
            freq,
            sample_count,
        ),
        PercussionKind::Snare => snare(
            sample_rate,
            p.snap.unwrap_or(0.7),
            p.buzz.unwrap_or(0.6),
            freq,
            p.noise_amount.unwrap_or(0.8),
            sample_count,
        ),
        PercussionKind::Hihat => hihat(
            sample_rate,
            p.metallic.unwrap_or(0.8),
            p.decay.unwrap_or(0.15),
            p.brightness.unwrap_or(0.9),
            freq,
            sample_count,
        ),
        PercussionKind::Cymbal => cymbal(
            sample_rate,
            p.size.unwrap_or(0.7),
            p.metallic.unwrap_or(0.9),
            p.strike_intensity.unwrap_or(0.8),
            freq,
            sample_count,
        ),
        PercussionKind::Zap => zap(
            sample_rate,
            freq,
            p.energy.unwrap_or(0.8),
            p.decay.unwrap_or(0.3),
            p.harmonic_content.unwrap_or(0.7),
            sample_count,
        ),
        PercussionKind::Swoosh => {
            let [start, end] = p.sweep.unwrap_or([200.0, 2000.0]);
            swoosh(
                sample_rate,
                p.direction.unwrap_or(0.0),
                p.intensity.unwrap_or(0.7),
                (start, end),
                sample_count,
            )
        }
        PercussionKind::Chime => chime(
            sample_rate,
            freq,
            p.harmonic_count.unwrap_or(5),
            p.decay.unwrap_or(0.5),
            p.inharmonicity.unwrap_or(0.1),
            sample_count,
        ),
        PercussionKind::Burst => burst(
            sample_rate,
            freq,
            p.bandwidth.unwrap_or(500.0),
            p.intensity.unwrap_or(0.8),
            p.shape.unwrap_or(0.5),
            sample_count,
        ),
    }
}
```

Keep the existing private `kick`, `snare`, `hihat`, `cymbal`, `zap`, `swoosh` functions as they are (check the `swoosh` parameter order against its current signature at `src/expressive/percussion.rs:271` and adapt the call above to match). Add `chime` and `burst`, ported from the `Oscillator::sample` arms in `synth.rs` (lines ~487-520), with the envelope in seconds based on `sample_count`:

```rust
fn chime(
    sample_rate: f32,
    fundamental: f32,
    harmonic_count: u8,
    decay: f32,
    inharmonicity: f32,
    sample_count: usize,
) -> Vec<f32> {
    let count = harmonic_count.max(1) as usize;
    let norm = 1.0 / (count as f32).sqrt();
    (0..sample_count)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let mut out = 0.0;
            for n in 1..=count {
                let partial = fundamental * n as f32 * (1.0 + inharmonicity * (n as f32 - 1.0) * 0.01);
                let env = (-t * (1.0 / decay.max(0.01)) * (1.0 + n as f32 * 0.1)).exp();
                out += (TAU * partial * t).sin() * env / n as f32;
            }
            out * norm
        })
        .collect()
}

fn burst(
    sample_rate: f32,
    center_freq: f32,
    bandwidth: f32,
    intensity: f32,
    shape: f32,
    sample_count: usize,
) -> Vec<f32> {
    let mut rng = rand::rng();
    let mut tone = PhaseAccumulator::new(sample_rate);
    let mut lp = 0.0f32;
    let alpha = (TAU * (center_freq + bandwidth) / sample_rate).min(0.99);
    let duration = (sample_count as f32 / sample_rate).max(0.1);
    (0..sample_count)
        .map(|i| {
            let progress = i as f32 / sample_rate / duration;
            let env = if shape < 0.5 {
                (-progress * 8.0).exp()
            } else {
                (-(progress * 3.0).powi(2)).exp()
            };
            lp += alpha * (noise(&mut rng) - lp);
            (lp * 1.5 + tone.next(center_freq) * 0.3) * env * intensity
        })
        .collect()
}
```

Add `use crate::expressive::oscillator::PhaseAccumulator; use std::f32::consts::TAU;` at the top. Note the chime decay semantics: the old code used `decay` as a rate multiplier; here `decay` is seconds-ish (time constant), matching the spec's "decay" field on chime. Adjust the test threshold if the measured ratio differs, but keep the direction (early louder than late).

3. In `src/expressive/synth.rs`, `generate_synthesized_samples` currently calls `percussion::render(sr, &params.synth_type, params.frequency, sample_count)`. Replace that call with a temporary adapter so the old path compiles until Task 10 removes it:

```rust
let mut samples = match percussion_config(&params.synth_type, params.frequency) {
    Some(cfg) => percussion::render(sr, &cfg, sample_count),
    None => { /* existing oscillator branch unchanged */ }
};
```

with, near the bottom of `synth.rs`:

```rust
/// Bridge from the legacy `SynthType` percussion variants (removed in the
/// patch migration) to the new config struct.
fn percussion_config(synth_type: &SynthType, frequency: f32) -> Option<crate::expressive::Percussion> {
    use crate::expressive::{Percussion, PercussionKind};
    let base = |kind| Percussion {
        kind,
        level: 1.0,
        frequency: Some(frequency),
        punch: None, sustain: None, click_freq: None, snap: None, buzz: None,
        noise_amount: None, metallic: None, decay: None, brightness: None, size: None,
        strike_intensity: None, energy: None, harmonic_content: None, direction: None,
        intensity: None, sweep: None, harmonic_count: None, inharmonicity: None,
        bandwidth: None, shape: None,
    };
    Some(match synth_type {
        SynthType::Kick { punch, sustain, click_freq, body_freq } => Percussion {
            punch: Some(*punch), sustain: Some(*sustain), click_freq: Some(*click_freq),
            frequency: Some(*body_freq), ..base(PercussionKind::Kick)
        },
        SynthType::Snare { snap, buzz, tone_freq, noise_amount } => Percussion {
            snap: Some(*snap), buzz: Some(*buzz), noise_amount: Some(*noise_amount),
            frequency: Some(*tone_freq), ..base(PercussionKind::Snare)
        },
        SynthType::HiHat { metallic, decay, brightness } => Percussion {
            metallic: Some(*metallic), decay: Some(*decay), brightness: Some(*brightness),
            ..base(PercussionKind::Hihat)
        },
        SynthType::Cymbal { size, metallic, strike_intensity } => Percussion {
            size: Some(*size), metallic: Some(*metallic), strike_intensity: Some(*strike_intensity),
            ..base(PercussionKind::Cymbal)
        },
        SynthType::Zap { energy, decay, harmonic_content } => Percussion {
            energy: Some(*energy), decay: Some(*decay), harmonic_content: Some(*harmonic_content),
            ..base(PercussionKind::Zap)
        },
        SynthType::Swoosh { direction, intensity, frequency_sweep } => Percussion {
            direction: Some(*direction), intensity: Some(*intensity),
            sweep: Some([frequency_sweep.0, frequency_sweep.1]), ..base(PercussionKind::Swoosh)
        },
        _ => return None,
    })
}
```

(`cargo fmt` will reflow the struct literals.)

- [ ] **Step 4: Run the tests**

```bash
cargo test percussion 2>&1 | tail -20 && cargo test 2>&1 | tail -5
```

Expected: 5 percussion tests pass; the existing `kick_pitch_sweeps_downward` in `synth.rs` still passes through the adapter.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/percussion.rs src/expressive/synth.rs
git commit -m "refactor(expressive): percussion renders from a Percussion config, adds chime and burst"
```

---

### Task 5: `Voice` trait, `Modulation`, and the subtractive engine

**Files:**
- Create: `src/expressive/engines/mod.rs`, `src/expressive/engines/subtractive.rs`, `src/expressive/engines/percussion.rs`
- Modify: `src/expressive/mod.rs`

**Interfaces:**
- Consumes: `GateEnvelope`, `Adsr` (Task 1); `PhaseAccumulator`, `wave_sample`, `Wave` (Task 2); `Subtractive`, `Filter`, `FilterKind`, `Percussion` (Task 3); `percussion::render` (Task 4); `Svf`, `SvfMode` (`effects.rs`).
- Produces:
  ```rust
  // engines/mod.rs
  #[derive(Clone, Copy)] pub struct Modulation { pub pitch_ratio: f32, pub cutoff_ratio: f32, pub amplitude: f32 } // Default: all 1.0
  pub trait Voice { fn gate_off(&mut self); fn is_active(&self) -> bool; fn tick(&mut self, mods: &Modulation) -> (f32, f32); }
  // engines/subtractive.rs
  pub struct SubtractiveVoice; impl SubtractiveVoice { pub fn new(cfg: &Subtractive, frequency: f32, sample_rate: f32) -> Self }
  // engines/percussion.rs
  pub struct PercussionVoice; impl PercussionVoice { pub fn new(cfg: &Percussion, gate_seconds: f32, sample_rate: f32) -> Self }
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/engines/mod.rs`:

```rust
//! One `Voice` per note per enabled engine. The renderer sums voices, so a
//! voice only has to produce its own signal and report when it has finished.

pub mod percussion;
pub mod subtractive;

pub use percussion::PercussionVoice;
pub use subtractive::SubtractiveVoice;

/// Per-sample modulation inputs shared by all voices (the LFO writes these in a later PR).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modulation {
    pub pitch_ratio: f32,
    pub cutoff_ratio: f32,
    pub amplitude: f32,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            cutoff_ratio: 1.0,
            amplitude: 1.0,
        }
    }
}

pub trait Voice {
    /// The note ended; start the release (percussion ignores this).
    fn gate_off(&mut self);
    /// False once the voice has nothing more to output.
    fn is_active(&self) -> bool;
    /// One stereo sample, already scaled by the engine's `level`.
    fn tick(&mut self, mods: &Modulation) -> (f32, f32);
}
```

Create `src/expressive/engines/subtractive.rs` with the tests module:

```rust
//! Two oscillators, an optional 12/24 dB state-variable filter with its own
//! envelope, and an amplitude envelope.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::{PhaseAccumulator, wave_sample};
use crate::expressive::{FilterKind, GateEnvelope, Subtractive, Svf, SvfMode};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};
    use crate::expressive::{Adsr, Filter, Osc, Osc2, Wave};

    const SR: f32 = 44100.0;

    fn render(cfg: &Subtractive, freq: f32, gate: f32, total: f32) -> Vec<f32> {
        let mut v = SubtractiveVoice::new(cfg, freq, SR);
        let mods = Modulation::default();
        let gate_end = (gate * SR) as usize;
        (0..(total * SR) as usize)
            .map(|i| {
                if i == gate_end {
                    v.gate_off();
                }
                v.tick(&mods).0
            })
            .collect()
    }

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    #[test]
    fn pitch_follows_the_requested_frequency() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 330.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 660.0).abs() < 10.0);
    }

    #[test]
    fn release_sounds_past_the_gate_and_then_stops() {
        let cfg = Subtractive {
            env: Adsr {
                release: 0.5,
                ..fast()
            },
            ..Default::default()
        };
        let mut v = SubtractiveVoice::new(&cfg, 220.0, SR);
        let mods = Modulation::default();
        for _ in 0..4410 {
            v.tick(&mods);
        }
        v.gate_off();
        let after: Vec<f32> = (0..4410).map(|_| v.tick(&mods).0).collect();
        assert!(rms(&after) > 0.1, "still audible 0.1 s after gate");
        assert!(v.is_active());
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active(), "silent after the release");
    }

    #[test]
    fn low_pass_filter_removes_high_harmonics() {
        let open = Subtractive {
            env: fast(),
            ..Default::default()
        };
        let closed = Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 300.0,
                resonance: 0.1,
                slope: 12,
                env_amount: 0.0,
                env: Adsr::default(),
            }),
            env: fast(),
            ..Default::default()
        };
        let a = render(&open, 110.0, 1.0, 1.0);
        let b = render(&closed, 110.0, 1.0, 1.0);
        let h10 = 1100.0;
        assert!(
            db(goertzel_power(&b, h10, SR) / goertzel_power(&a, h10, SR)) < -15.0,
            "10th harmonic attenuated"
        );
    }

    #[test]
    fn slope_24_attenuates_more_than_12() {
        let mk = |slope| Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 300.0,
                resonance: 0.1,
                slope,
                env_amount: 0.0,
                env: Adsr::default(),
            }),
            env: fast(),
            ..Default::default()
        };
        let a = render(&mk(12), 110.0, 1.0, 1.0);
        let b = render(&mk(24), 110.0, 1.0, 1.0);
        let p = |s: &[f32]| goertzel_power(s, 2200.0, SR);
        assert!(db(p(&b) / p(&a)) < -6.0, "24 dB is steeper");
    }

    #[test]
    fn positive_filter_envelope_opens_then_closes_the_cutoff() {
        let cfg = Subtractive {
            filter: Some(Filter {
                kind: FilterKind::LowPass,
                cutoff: 200.0,
                resonance: 0.1,
                slope: 12,
                env_amount: 1.0,
                env: Adsr {
                    attack: 0.001,
                    decay: 0.4,
                    sustain: 0.0,
                    release: 0.1,
                },
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 110.0, 1.0, 1.0);
        let early = goertzel_power(&s[..4410], 1100.0, SR);
        let late = goertzel_power(&s[26460..30870], 1100.0, SR);
        assert!(db(early / late) > 12.0, "bright at first, dark later: {}", db(early / late));
    }

    #[test]
    fn detuned_second_oscillator_beats_at_the_detune_rate() {
        // 220 Hz with osc2 at +50 cents beats at about 220 * (2^(50/1200) - 1) = 6.5 Hz.
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            osc2: Some(Osc2 {
                wave: Wave::Sine,
                pulse_width: 0.5,
                mix: 0.5,
                detune_cents: 50.0,
                octave: 0,
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 220.0, 2.0, 2.0);
        // RMS over 50 ms windows follows the beat.
        let win = 2205;
        let env: Vec<f32> = s.chunks(win).map(rms).collect();
        let (min, max) = env
            .iter()
            .skip(1)
            .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(max > min * 3.0, "amplitude beats: min {min} max {max}");
    }

    #[test]
    fn octave_offset_shifts_osc2_pitch() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            osc2: Some(Osc2 {
                wave: Wave::Sine,
                pulse_width: 0.5,
                mix: 1.0,
                detune_cents: 0.0,
                octave: 1,
            }),
            env: fast(),
            ..Default::default()
        };
        let s = render(&cfg, 220.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
    }

    #[test]
    fn modulation_pitch_ratio_and_amplitude_are_applied() {
        let cfg = Subtractive {
            osc1: Osc {
                wave: Wave::Sine,
                pulse_width: 0.5,
            },
            env: fast(),
            ..Default::default()
        };
        let mut v = SubtractiveVoice::new(&cfg, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
        };
        let s: Vec<f32> = (0..SR as usize).map(|_| v.tick(&mods).0).collect();
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
        assert!((s.iter().cloned().fold(0.0f32, f32::max) - 0.5).abs() < 0.05);
    }
}
```

Create `src/expressive/engines/percussion.rs`:

```rust
//! A percussive hit pre-rendered at note-on; ignores gate-off.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{Percussion, percussion};

pub struct PercussionVoice {
    samples: Vec<f32>,
    pos: usize,
    level: f32,
}

impl PercussionVoice {
    /// Renders `gate_seconds` of the hit (the hit's own envelope shapes it).
    pub fn new(cfg: &Percussion, gate_seconds: f32, sample_rate: f32) -> Self {
        let count = ((gate_seconds.max(0.05)) * sample_rate) as usize;
        Self {
            samples: percussion::render(sample_rate, cfg, count),
            pos: 0,
            level: cfg.level,
        }
    }
}

impl Voice for PercussionVoice {
    fn gate_off(&mut self) {}

    fn is_active(&self) -> bool {
        self.pos < self.samples.len()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let s = self.samples.get(self.pos).copied().unwrap_or(0.0) * self.level * mods.amplitude;
        self.pos += 1;
        (s, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::PercussionKind;

    #[test]
    fn plays_out_regardless_of_gate_off_and_then_deactivates() {
        let cfg: Percussion = serde_json::from_str(r#"{"kind": "kick", "level": 0.5}"#).unwrap();
        let mut v = PercussionVoice::new(&cfg, 0.2, 44100.0);
        assert_eq!(cfg.kind, PercussionKind::Kick);
        v.gate_off();
        let mods = Modulation::default();
        let mut peak = 0.0f32;
        for _ in 0..(0.2 * 44100.0) as usize {
            assert!(v.is_active());
            peak = peak.max(v.tick(&mods).0.abs());
        }
        assert!(!v.is_active());
        assert!(peak > 0.1 && peak <= 0.5 * 1.5, "level applied: {peak}");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod engines;` to `src/expressive/mod.rs` (no glob re-export; engines are referenced as `crate::expressive::engines::...`). `cargo test engines 2>&1 | tail`. Expected: `SubtractiveVoice` not found.

- [ ] **Step 3: Implement `SubtractiveVoice`**

Above the tests in `engines/subtractive.rs`:

```rust
pub struct SubtractiveVoice {
    cfg: Subtractive,
    sample_rate: f32,
    frequency: f32,
    osc1: PhaseAccumulator,
    osc2: PhaseAccumulator,
    amp_env: GateEnvelope,
    filter_env: Option<GateEnvelope>,
    svf1: Option<Svf>,
    svf2: Option<Svf>,
    rng: rand::rngs::ThreadRng,
}

fn svf_mode(kind: FilterKind) -> SvfMode {
    match kind {
        FilterKind::LowPass => SvfMode::LowPass,
        FilterKind::HighPass => SvfMode::HighPass,
        FilterKind::BandPass => SvfMode::BandPass,
    }
}

impl SubtractiveVoice {
    pub fn new(cfg: &Subtractive, frequency: f32, sample_rate: f32) -> Self {
        let (filter_env, svf1, svf2) = match &cfg.filter {
            Some(f) => {
                let q = 0.5 + f.resonance.clamp(0.0, 1.0) * 9.5;
                let make = || Svf::new(sample_rate, f.cutoff, q, svf_mode(f.kind));
                (
                    Some(GateEnvelope::new(&f.env, sample_rate)),
                    Some(make()),
                    (f.slope == 24).then(make),
                )
            }
            None => (None, None, None),
        };
        Self {
            cfg: cfg.clone(),
            sample_rate,
            frequency,
            osc1: PhaseAccumulator::new(sample_rate),
            osc2: PhaseAccumulator::new(sample_rate),
            amp_env: GateEnvelope::new(&cfg.env, sample_rate),
            filter_env,
            svf1,
            svf2,
            rng: rand::rng(),
        }
    }
}

impl Voice for SubtractiveVoice {
    fn gate_off(&mut self) {
        self.amp_env.gate_off();
        if let Some(env) = &mut self.filter_env {
            env.gate_off();
        }
    }

    fn is_active(&self) -> bool {
        self.amp_env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let freq = self.frequency * mods.pitch_ratio;
        let dt1 = freq / self.sample_rate;
        let p1 = self.osc1.next_unit(freq);
        let mut s = wave_sample(self.cfg.osc1.wave, p1, dt1, self.cfg.osc1.pulse_width, &mut self.rng);

        if let Some(o2) = &self.cfg.osc2
            && o2.mix > 0.0
        {
            let f2 = freq * 2f32.powi(o2.octave as i32) * 2f32.powf(o2.detune_cents / 1200.0);
            let p2 = self.osc2.next_unit(f2);
            let s2 = wave_sample(o2.wave, p2, f2 / self.sample_rate, o2.pulse_width, &mut self.rng);
            s = s * (1.0 - o2.mix) + s2 * o2.mix;
        }

        if let (Some(f), Some(svf1)) = (&self.cfg.filter, &mut self.svf1) {
            let env = self.filter_env.as_mut().map(|e| e.next()).unwrap_or(0.0);
            let cutoff = (f.cutoff * 2f32.powf(f.env_amount * env * 4.0) * mods.cutoff_ratio)
                .clamp(20.0, 20000.0);
            svf1.set_cutoff(cutoff);
            s = svf1.process(s);
            if let Some(svf2) = &mut self.svf2 {
                svf2.set_cutoff(cutoff);
                s = svf2.process(s);
            }
        }

        let amp = self.amp_env.next() * self.cfg.level * mods.amplitude;
        let out = s * amp;
        (out, out)
    }
}
```

`Svf::set_cutoff` recomputes `tan` each sample; that is fine for offline rendering.

- [ ] **Step 4: Run the tests**

```bash
cargo test engines 2>&1 | tail -25
```

Expected: 9 passed (8 subtractive, 1 percussion). If `detuned_second_oscillator_beats_at_the_detune_rate` is marginal, widen the window count rather than the ratio: the beat must be clearly visible.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/engines src/expressive/mod.rs
git commit -m "feat(expressive): Voice trait with subtractive and percussion engines"
```

---

### Task 6: Per-patch renderer

**Files:**
- Create: `src/expressive/render.rs`
- Modify: `src/expressive/mod.rs`

**Interfaces:**
- Consumes: `Patch` (Task 3), `SubtractiveVoice`, `PercussionVoice`, `Voice`, `Modulation` (Task 5), `EffectsChain` (`effects.rs`).
- Produces:
  ```rust
  pub const EFFECT_TAIL_SECONDS: f32 = 1.0;   // moved here from synth.rs
  pub struct NoteEvent { pub start: f32, pub duration: f32, pub frequency: f32, pub velocity: f32 } // seconds relative to the buffer start, velocity 0..1
  pub fn render_patch(patch: &Patch, notes: &[NoteEvent], sample_rate: f32) -> Vec<[f32; 2]>;
  pub fn render_length_seconds(patch: &Patch, notes: &[NoteEvent]) -> f32;
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/render.rs`:

```rust
//! Renders every note of one patch into a single stereo buffer: voices are
//! summed per sample, then the patch's effects chain runs once over the sum.

use crate::expressive::engines::{Modulation, PercussionVoice, SubtractiveVoice, Voice};
use crate::expressive::{EffectsChain, Patch};

/// Extra silence rendered after the last release so reverbs and delays can ring out.
pub const EFFECT_TAIL_SECONDS: f32 = 1.0;

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
        let wet = patch(json!({"name": "w", "subtractive": {"env": {"release": 0.5}},
            "effects": [{"type": "reverb"}]}));
        assert!((render_length_seconds(&wet, &notes) - 3.5).abs() < 1e-4);
        assert_eq!(render_patch(&dry, &notes, SR).len(), (2.5 * SR) as usize);
    }

    #[test]
    fn a_note_starts_at_its_offset_and_rings_through_its_release() {
        let p = patch(json!({"name": "p", "subtractive": {"env": {"attack": 0.001, "release": 0.3}}}));
        let buf = left(&render_patch(&p, &[note(0.5, 0.5, 220.0)], SR));
        assert!(rms(&buf[..(0.45 * SR) as usize]) < 1e-6, "silent before start");
        assert!(rms(&buf[(0.6 * SR) as usize..(0.9 * SR) as usize]) > 0.1, "sounding");
        assert!(rms(&buf[(1.1 * SR) as usize..(1.2 * SR) as usize]) > 0.01, "release audible");
        // The buffer ends exactly when the release does (1.0 s gate + 0.3 s release).
        assert_eq!(buf.len(), (1.3 * SR) as usize);
        assert!(rms(&buf[(1.295 * SR) as usize..]) < 0.05, "faded out by the end");
    }

    #[test]
    fn overlapping_notes_sum() {
        let p = patch(json!({"name": "p", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}));
        let one = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let two = left(&render_patch(&p, &[note(0.0, 1.0, 220.0), note(0.0, 1.0, 220.0)], SR));
        let r1 = rms(&one[4410..40000]);
        let r2 = rms(&two[4410..40000]);
        assert!((r2 / r1 - 2.0).abs() < 0.05, "two identical voices double the amplitude");
    }

    #[test]
    fn velocity_and_patch_level_scale_amplitude() {
        let p = patch(json!({"name": "p", "level": 0.5, "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}));
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
        assert!((peak(&full) - 0.5).abs() < 0.03, "level 0.5: {}", peak(&full));
        assert!((peak(&soft) - 0.25).abs() < 0.03, "velocity halves it: {}", peak(&soft));
    }

    #[test]
    fn effects_run_once_over_the_summed_signal() {
        let dry = patch(json!({"name": "d", "subtractive": {"env": {"release": 0.01}}}));
        let wet = patch(json!({"name": "w", "subtractive": {"env": {"release": 0.01}},
            "effects": [{"type": "reverb", "room_size": 0.9, "wet_level": 0.8, "intensity": 1.0}]}));
        let n = [note(0.0, 0.5, 220.0)];
        let d = left(&render_patch(&dry, &n, SR));
        let w = left(&render_patch(&wet, &n, SR));
        let tail = (0.7 * SR) as usize..(1.2 * SR) as usize;
        assert!(rms(&d[tail.clone()]) < 1e-4, "dry is silent after release");
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
        let p = patch(json!({"name": "both", "subtractive": {"level": 0.5, "env": {"release": 0.01}},
            "percussion": {"kind": "kick", "level": 0.5}}));
        let a = left(&render_patch(&p, &[note(0.0, 0.5, 110.0)], SR));
        let sub_only = patch(json!({"name": "s", "subtractive": {"level": 0.5, "env": {"release": 0.01}}}));
        let b = left(&render_patch(&sub_only, &[note(0.0, 0.5, 110.0)], SR));
        assert!(rms(&a[..2205]) > rms(&b[..2205]) * 1.2, "kick adds energy at the start");
    }

    #[test]
    fn empty_notes_render_nothing() {
        let p = patch(json!({"name": "p", "subtractive": {}}));
        assert!(render_patch(&p, &[], SR).is_empty());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod render;` and `pub use render::*;` to `src/expressive/mod.rs`. Remove `const EFFECT_TAIL_SECONDS` from `synth.rs` and import it from `render` there (`use crate::expressive::render::EFFECT_TAIL_SECONDS;`). `cargo test render:: 2>&1 | tail`. Expected: missing `NoteEvent`, `render_patch`.

- [ ] **Step 3: Implement**

Above the tests in `render.rs`:

```rust
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
    let last_gate = notes
        .iter()
        .map(|n| n.start.max(0.0) + n.duration.max(0.0))
        .fold(0.0f32, f32::max);
    let effect_tail = if patch.effects.iter().any(|e| e.enabled) {
        EFFECT_TAIL_SECONDS
    } else {
        0.0
    };
    last_gate + patch.release_seconds() + effect_tail
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
```

Note: `gate_off` must run even when `gate_end == start` (a zero-length note). The order above handles it because `i == v.gate_end` is checked after the `i < v.start` skip on the same sample.

- [ ] **Step 4: Run the tests**

```bash
cargo test render:: 2>&1 | tail -25 && cargo test 2>&1 | tail -5
```

Expected: 8 passed; the rest of the suite unaffected.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/render.rs src/expressive/mod.rs src/expressive/synth.rs
git commit -m "feat(expressive): per-patch stereo renderer"
```

---

### Task 7: Built-in patch library

**Files:**
- Create: `src/expressive/patches/*.json` (list below)
- Modify: `src/expressive/patch.rs` (add `PatchLibrary`)

**Interfaces:**
- Produces:
  ```rust
  pub struct PatchLibrary;
  impl PatchLibrary {
      pub fn new() -> Self;                       // parses every embedded file, panics on a bad file (covered by test)
      pub fn get(&self, name: &str) -> Option<&Patch>;   // case-insensitive, trims
      pub fn catalog(&self) -> Vec<(PatchCategory, Vec<&Patch>)>;  // sorted by name within category, only non-empty categories
      pub fn names(&self) -> Vec<&str>;           // sorted
      pub fn count(&self) -> usize;
  }
  ```

- [ ] **Step 1: Write the failing tests**

Append to the tests module in `src/expressive/patch.rs`:

```rust
    #[test]
    fn every_builtin_patch_parses_validates_and_renders_cleanly() {
        use crate::expressive::render::{NoteEvent, render_patch};
        let lib = PatchLibrary::new();
        assert!(lib.count() >= 28, "expected the migrated presets, got {}", lib.count());
        for name in lib.names() {
            let p = lib.get(name).unwrap();
            p.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(p.category.is_some(), "{name} needs a category");
            assert!(!p.description.is_empty(), "{name} needs a description");
            let note = NoteEvent {
                start: 0.0,
                duration: 0.5,
                frequency: if p.has_pitched_engine() { 261.63 } else { 60.0 },
                velocity: 100.0 / 127.0,
            };
            let buf = render_patch(p, &[note], 44100.0);
            let peak = buf
                .iter()
                .flat_map(|s| s.iter())
                .fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak.is_finite(), "{name} produced NaN/inf");
            // SYNTH_BUS_GAIN (0.5) is applied later; stay under the 0.8 clipper knee after it.
            assert!(peak * 0.5 <= 0.8, "{name} peaks at {peak}, too hot for the bus");
            assert!(peak > 0.02, "{name} is nearly silent (peak {peak})");
        }
    }

    #[test]
    fn library_lookup_is_case_insensitive_and_catalog_is_grouped() {
        let lib = PatchLibrary::new();
        assert!(lib.get("Minimoog_Bass").is_some());
        assert!(lib.get(" tr_808_kick ").is_some());
        assert!(lib.get("nope").is_none());
        let catalog = lib.catalog();
        let cats: Vec<PatchCategory> = catalog.iter().map(|(c, _)| *c).collect();
        assert!(cats.contains(&PatchCategory::Bass) && cats.contains(&PatchCategory::Drums));
        for (_, patches) in &catalog {
            assert!(!patches.is_empty());
            assert!(patches.windows(2).all(|w| w[0].name <= w[1].name), "sorted by name");
        }
    }
```

- [ ] **Step 2: Run to verify failure**

`cargo test patch::tests::library 2>&1 | tail`. Expected: `PatchLibrary` not found.

- [ ] **Step 3: Implement `PatchLibrary`**

Add to `patch.rs`:

```rust
use std::collections::HashMap;

/// Built-in patches, embedded at compile time from `patches/*.json`.
const BUILTIN_PATCHES: &[&str] = &[
    // bass
    include_str!("patches/minimoog_bass.json"),
    include_str!("patches/minimoog_bass_bright.json"),
    include_str!("patches/tb_303_acid.json"),
    include_str!("patches/tb_303_acid_squelchy.json"),
    include_str!("patches/odyssey_bite.json"),
    include_str!("patches/jupiter_bass.json"),
    include_str!("patches/saw_bass.json"),
    include_str!("patches/square_bass.json"),
    include_str!("patches/sub_bass.json"),
    include_str!("patches/rubber_bass.json"),
    // lead
    include_str!("patches/prophet_lead.json"),
    // pads (subtractive approximations until the granular/LFO PR)
    include_str!("patches/jp_8_strings.json"),
    include_str!("patches/ob_brass.json"),
    include_str!("patches/analog_wash.json"),
    include_str!("patches/d_50_fantasia.json"),
    include_str!("patches/crystal_pad.json"),
    include_str!("patches/space_pad.json"),
    include_str!("patches/dark_pad.json"),
    include_str!("patches/choir_pad.json"),
    include_str!("patches/wind_pad.json"),
    include_str!("patches/dream_pad.json"),
    include_str!("patches/warm_pad.json"),
    // drums
    include_str!("patches/tr_808_kick.json"),
    include_str!("patches/tr_909_snare.json"),
    include_str!("patches/tr_909_hihat.json"),
    include_str!("patches/tr_808_hihat.json"),
    include_str!("patches/crash_cymbal.json"),
    // fx
    include_str!("patches/sci_fi_zap.json"),
    include_str!("patches/sweep_up.json"),
    include_str!("patches/chime.json"),
    include_str!("patches/burst.json"),
];

pub struct PatchLibrary {
    patches: HashMap<String, Patch>,
}

impl Default for PatchLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchLibrary {
    pub fn new() -> Self {
        let mut patches = HashMap::new();
        for source in BUILTIN_PATCHES {
            let patch: Patch = serde_json::from_str(source)
                .unwrap_or_else(|e| panic!("built-in patch does not parse: {e}\n{source}"));
            patch
                .validate()
                .unwrap_or_else(|e| panic!("built-in patch '{}' is invalid: {e}", patch.name));
            patches.insert(patch.key(), patch);
        }
        Self { patches }
    }

    pub fn get(&self, name: &str) -> Option<&Patch> {
        self.patches.get(&name.trim().to_lowercase())
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.patches.values().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names
    }

    pub fn count(&self) -> usize {
        self.patches.len()
    }

    pub fn catalog(&self) -> Vec<(PatchCategory, Vec<&Patch>)> {
        PatchCategory::ALL
            .iter()
            .filter_map(|category| {
                let mut patches: Vec<&Patch> = self
                    .patches
                    .values()
                    .filter(|p| p.category == Some(*category))
                    .collect();
                if patches.is_empty() {
                    return None;
                }
                patches.sort_by(|a, b| a.name.cmp(&b.name));
                Some((*category, patches))
            })
            .collect()
    }
}
```

- [ ] **Step 4: Write the patch files**

Create `src/expressive/patches/`. Each file is one `Patch` object. Patch names are the snake_case file stem. Port values from the Rust presets, which are still on disk at this point in `src/expressive/presets/categories/{bass,pads,leads,drums,effects}.rs` (and forever at `git show e321ff0:src/expressive/presets/categories/bass.rs`). Mapping rules:

| Rust preset field | Patch field |
|---|---|
| `SynthType::Sawtooth / Square{pulse_width} / Triangle / Sine / Noise` | `subtractive.osc1.wave` (+ `pulse_width`) |
| `SynthType::Pad { warmth, movement, .. }` | `osc1.wave: "saw"`, `osc2: {"wave": "saw", "mix": 0.5, "detune_cents": 7 + movement*10}`, `filter.cutoff: 600 + (1 - warmth) * 2400` low_pass, slope 24 |
| `SynthType::Texture { .. }` (Dream Pad) | `osc1.wave: "triangle"`, `osc2: {"wave": "noise", "mix": 0.15}`, low_pass 1200 |
| `envelope: create_envelope(a, d, s, r)` | `subtractive.env` |
| `filter: create_filter(cutoff, resonance, type)` | `subtractive.filter` (`resonance` keeps the 0..1 scale; `type` snake_case) |
| `amplitude` | `level` (patch-level) |
| `synth_params.effects` followed by `signature_effects` | `effects` array, in that order, converted to the flat form (`{"type": "reverb", "room_size": .., "intensity": ..}`); read `create_reverb`, `create_chorus` and `create_signature_effects_for_*` in `src/expressive/presets/library.rs` for the exact numbers |
| variation `bright`/`squelchy` with meaningful overrides | separate patch (`minimoog_bass_bright.json`, `tb_303_acid_squelchy.json`); other variations dropped |
| drum `SynthType::Kick {..}` etc. | `percussion` with `kind` and the same parameter names; `body_freq`/`tone_freq` become `frequency` |
| `description`, `inspiration` | `description` = `"{description} ({inspiration})"` |
| `category` | `category` (`Effects` becomes `fx`) |

Four fully worked files (write the remaining ones the same way):

`minimoog_bass.json`
```json
{
  "name": "minimoog_bass",
  "description": "Classic warm Moog bass with ladder-style low-pass and punch (Moog Minimoog Model D)",
  "category": "bass",
  "level": 0.8,
  "subtractive": {
    "osc1": {"wave": "saw"},
    "osc2": {"wave": "square", "mix": 0.25, "detune_cents": 4, "octave": 0},
    "filter": {"type": "low_pass", "cutoff": 700, "resonance": 0.05, "slope": 24,
               "env_amount": 0.4, "env": {"attack": 0.005, "decay": 0.35, "sustain": 0.2, "release": 0.3}},
    "env": {"attack": 0.01, "decay": 0.3, "sustain": 0.7, "release": 0.5}
  },
  "effects": [
    {"type": "reverb", "room_size": 0.3, "dampening": 0.5, "wet_level": 0.15, "intensity": 0.15}
  ]
}
```
(then append the bass signature effects from `create_signature_effects_for_bass` in the flat form).

`warm_pad.json` (new; the spec's example)
```json
{
  "name": "warm_pad",
  "description": "Slow detuned saw pad with a filter sweep",
  "category": "pad",
  "level": 0.7,
  "subtractive": {
    "osc1": {"wave": "saw"},
    "osc2": {"wave": "saw", "mix": 0.4, "detune_cents": 12, "octave": 0},
    "filter": {"type": "low_pass", "cutoff": 800, "resonance": 0.3, "slope": 24,
               "env_amount": 0.6, "env": {"attack": 1.5, "decay": 2.0, "sustain": 0.3, "release": 3.0}},
    "env": {"attack": 0.8, "decay": 1.0, "sustain": 0.7, "release": 2.5}
  },
  "effects": [
    {"type": "chorus", "rate": 0.6, "depth": 0.4, "intensity": 0.3},
    {"type": "reverb", "room_size": 0.7, "dampening": 0.4, "wet_level": 0.35, "intensity": 0.35}
  ]
}
```

`tr_808_kick.json`
```json
{
  "name": "tr_808_kick",
  "description": "Deep booming analog kick with long sustain (Roland TR-808)",
  "category": "drums",
  "level": 0.9,
  "percussion": {"kind": "kick", "frequency": 55, "punch": 0.6, "sustain": 0.8, "click_freq": 6000}
}
```
(copy the exact `punch`/`sustain`/`click_freq`/`body_freq` values from `drums.rs`; the numbers above are illustrative of the shape, the file must hold the real ones.)

`chime.json` (new, replaces `synth_type: "chime"`)
```json
{
  "name": "chime",
  "description": "Bell with inharmonic partials; pitch from frequency, default 880 Hz",
  "category": "fx",
  "level": 0.8,
  "percussion": {"kind": "chime", "frequency": 880, "harmonic_count": 5, "decay": 0.5, "inharmonicity": 0.1},
  "effects": [{"type": "reverb", "room_size": 0.6, "intensity": 0.3}]
}
```

`burst.json`: `{"kind": "burst", "frequency": 1000, "bandwidth": 500, "intensity": 0.8, "shape": 0.5}`, category `fx`.
`sweep_up.json`: from `effects.rs` "Sweep Up" (`swoosh`, `sweep` from its `frequency_sweep`).
`sci_fi_zap.json`: from "Sci-Fi Zap" (`zap`, `frequency` 800).

Run the library test after each few files; the test names the offending patch.

- [ ] **Step 5: Run the tests**

```bash
cargo test patch:: 2>&1 | tail -20
```

Expected: all pass, including the render/headroom check for every file. If a patch peaks too hot, lower its `level`; if too quiet, raise it (target peak 0.5 to 1.0 before the bus gain).

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/patch.rs src/expressive/patches
git commit -m "feat(expressive): built-in patch library ported from the Rust presets"
```

---

### Task 8: Stereo buffers in the engine

**Files:**
- Modify: `src/midi/engine.rs` (`PlayCommand.buffers`, `ScheduledBuffer`, `render_frames`, tests), `src/midi/translate.rs` (R2D2 and synthesis buffers become stereo)

**Interfaces:**
- Produces: `PlayCommand.buffers: Vec<(u64, Vec<[f32; 2]>)>`.

- [ ] **Step 1: Update the engine tests**

In `src/midi/engine.rs` tests, buffers are built as `vec![(0, vec![0.25; 10])]`, `vec![(0, vec![0.5; SAMPLE_RATE as usize])]`, and via `fn play(buffers: Vec<(u64, Vec<f32>)>, ...)`. Change each to stereo, e.g. `vec![(0, vec![[0.25, 0.25]; 10])]`, and change the helper signature to `Vec<(u64, Vec<[f32; 2]>)>`. Replace the `assert_eq!(left, right, "mono buffers must be centered")` assertion with a test that a buffer with different channels comes out on the right sides:

```rust
    #[test]
    fn stereo_buffers_keep_their_channels() {
        let (mut engine, _h) = MidiEngine::new(None);
        engine.apply(play(vec![(0, vec![[0.25, -0.5]; CHUNK_FRAMES])], PlayMode::Replace));
        let (mut l, mut r) = (vec![0.0; CHUNK_FRAMES], vec![0.0; CHUNK_FRAMES]);
        // Skip the lead-in.
        let mut rendered = 0;
        while rendered < LEAD_FRAMES as usize {
            engine.render_unclipped(&mut l, &mut r);
            rendered += CHUNK_FRAMES;
        }
        engine.render_unclipped(&mut l, &mut r);
        assert!(l.iter().any(|&x| (x - 0.25).abs() < 1e-6));
        assert!(r.iter().any(|&x| (x + 0.5).abs() < 1e-6));
    }
```

(Adapt the lead-in handling to match how the existing buffer tests in that module skip `LEAD_FRAMES`; copy their approach.)

- [ ] **Step 2: Run to verify failure**

`cargo test engine 2>&1 | tail`. Expected: type errors.

- [ ] **Step 3: Implement**

In `engine.rs`:
- `pub buffers: Vec<(u64, Vec<[f32; 2]>)>` with doc "Pre-rendered stereo buffers (R2D2, synthesis) already at bus level."
- `struct ScheduledBuffer { start: u64, samples: Vec<[f32; 2]>, pos: usize }`
- In `render_frames`, replace the inner zip with:
  ```rust
  for ((l, r), s) in left[offset..offset + count]
      .iter_mut()
      .zip(right[offset..offset + count].iter_mut())
      .zip(samples)
  {
      *l += s[0];
      *r += s[1];
  }
  ```

In `translate.rs`, where R2D2 samples are pushed (`buffers.push((seconds_to_frames(start), samples))`), convert: `buffers.push((seconds_to_frames(start), samples.into_iter().map(|s| [s, s]).collect()))`. Do the same for the synthesis branch (it is rewritten in Task 9, but must compile now).

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi/engine.rs src/midi/translate.rs
git commit -m "feat(engine): stereo pre-rendered buffers"
```

---

### Task 9: `synth` on notes, patch resolution in the translator

Additive step: the new path works alongside the old one so every commit builds.

**Files:**
- Modify: `src/midi/mod.rs` (`SimpleNote.synth`, helpers), `src/midi/translate.rs`, `src/midi/player.rs`, `src/demos.rs` (call-site signature only)

**Interfaces:**
- Consumes: `SynthRef`, `Patch`, `PatchLibrary` (Tasks 3, 7), `NoteEvent`, `render_patch` (Task 6).
- Produces:
  ```rust
  // mod.rs
  pub struct SimpleNote { /* ... */ #[serde(default)] pub synth: Option<SynthRef>, /* ... */ }
  impl SimpleNote { pub fn is_synthesis(&self) -> bool /* synth.is_some() */; pub fn validate_synth(&self) -> Result<(), String>; }
  // translate.rs
  impl Translator { pub fn translate(&self, sequence: SimpleSequence, mode: PlayMode, session_patches: &HashMap<String, Patch>) -> Result<Translation, String>; }
  // player.rs
  impl MidiPlayer { pub fn play(&mut self, sequence: SimpleSequence, mode: PlayMode, session_patches: &HashMap<String, Patch>) -> Result<Duration, String>; }
  ```

- [ ] **Step 1: Write the failing translator tests**

Add to the tests module in `src/midi/translate.rs` (alongside `synthesis_notes_become_buffers_at_bus_level`):

```rust
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
            serde_json::from_value(json!({"name": "Mine", "percussion": {"kind": "snare"}})).unwrap();
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
        assert!(err.contains("nope") && err.contains("Mine") && err.contains("list_sounds"), "{err}");
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
        assert!(t.translate(seq(vec![k]), PlayMode::Replace, &no_session()).is_ok());
    }

    #[test]
    fn an_invalid_inline_patch_is_an_error_naming_the_field() {
        let t = Translator::new(Err("no soundfont".into()));
        let bad = json!({"name": "hot", "subtractive": {"filter": {"cutoff": 99999}}});
        let err = t
            .translate(seq(vec![patch_note(bad, 60, 0.0, 0.2)]), PlayMode::Replace, &no_session())
            .unwrap_err();
        assert!(err.contains("subtractive.filter.cutoff"), "{err}");
    }
```

Add `use serde_json::json;` to the tests module if missing. Update every existing call `translate(seq, mode)` in the tests (and in `player.rs`) to pass `&no_session()` / `&HashMap::new()`.

- [ ] **Step 2: Run to verify failure**

`cargo test translate 2>&1 | tail`. Expected: no field `synth`, wrong arity.

- [ ] **Step 3: Implement the note field**

In `src/midi/mod.rs`, inside `SimpleNote` after `effects_preset`:

```rust
    /// Agent-defined synth patch: a name from define_synth / the built-in
    /// library, or an inline patch object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synth: Option<crate::expressive::SynthRef>,
```

Add `synth: None` to `Default`. Change `is_synthesis` to `self.synth.is_some() || self.synth_type.is_some()` (temporarily, until Task 10 drops `synth_type`). Add:

```rust
    /// Validate the patch reference: an inline patch must validate, and R2D2 keeps its own voice.
    pub fn validate_synth(&self) -> Result<(), String> {
        match &self.synth {
            None => Ok(()),
            Some(_) if self.note_type == "r2d2" => {
                Err("a note cannot have both note_type \"r2d2\" and synth".to_string())
            }
            Some(crate::expressive::SynthRef::Inline(p)) => p.validate(),
            Some(crate::expressive::SynthRef::Name(n)) if n.trim().is_empty() => {
                Err("synth name must not be empty".to_string())
            }
            Some(_) => Ok(()),
        }
    }
```

- [ ] **Step 4: Implement resolution and rendering in the translator**

In `src/midi/translate.rs`:

1. Add `patch_library: PatchLibrary` to `Translator` (construct with `PatchLibrary::new()`), import `crate::expressive::{Patch, PatchLibrary, SynthRef, NoteEvent, render_patch}` and `std::collections::HashMap`.
2. Change the signature to `translate(&self, sequence, mode, session_patches: &HashMap<String, Patch>)`.
3. Add a resolver:

```rust
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
```

4. In the per-note loop, before the R2D2 branch, handle `note.synth`: validate with `note.validate_synth()?`, resolve, compute `frequency` from `note.note` (`440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)`), or error `"Note {i}: synth '{name}' is pitched, so it needs a MIDI note"` when the patch `has_pitched_engine()` and `note.note` is `None` (percussion-only patches use 0.0). Collect into `groups: Vec<(String, Patch, Vec<(f64 /*abs start*/, NoteEvent)>)>` keyed by `serde_json::to_string(patch)` (inline) or the patch key (named). Keep insertion order with a `Vec` plus a `HashMap<String, usize>` index.
5. After the loop, for each group: `let first = min start`; build `NoteEvent { start: (abs - first) as f32, .. }`; `let mut samples = render_patch(&patch, &events, SAMPLE_RATE as f32)`; scale each channel by `SYNTH_BUS_GAIN`; `buffers.push((seconds_to_frames(Duration::from_secs_f64(first)), samples))`; `note_end = note_end.max(first + render_length_seconds(&patch, &events))`.
6. The existing `synth_type` branch stays for now (Task 10 deletes it); make its condition `else if note.synth_type.is_some()` so the two paths do not collide.
7. `midi_effects` filter: exclude notes with `synth.is_some()` too.

In `src/midi/player.rs`, thread `session_patches` through `play` into `translate`. Update `src/demos.rs` call sites to pass `&std::collections::HashMap::new()` (they are rewritten in Task 10; this keeps the build green).

- [ ] **Step 5: Run the tests**

```bash
cargo test 2>&1 | tail -8
```

Expected: all pass, including the 5 new translator tests.

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/midi src/demos.rs
git commit -m "feat(midi): synth patch references on notes, rendered per patch"
```

---

### Task 10: Remove the old synthesis path and Rust presets

**Files:**
- Modify: `src/midi/mod.rs` (remove `synth_*`/`preset_*` fields, `validate_synthesis`, `validate_preset`, `is_preset`; add `deny_unknown_fields`), `src/midi/translate.rs` (remove `apply_preset_to_note`, `convert_simple_note_to_synth_params`, `PresetLibrary`), `src/expressive/synth.rs` (remove `SynthParams`, `SynthType`, `NoiseColor`, `EnvelopeParams`, `FilterParams`, `FilterType`, `DX7Operator`, `Oscillator`, `generate_synthesized_samples`, the adapter, and their tests), `src/expressive/mod.rs`
- Delete: `src/expressive/presets/` (whole directory)
- Modify: `src/demos.rs`, `src/main.rs`, `src/midi/player.rs` tests, `src/server/mcp.rs` (only what is needed to compile: `validate_notes`, `describe_sources`, `list_sounds` preset/synthesis sections, `SYNTH_TYPES`), `CLAUDE.md`

The intermediate steps of this task do not compile; commit once at the end.

- [ ] **Step 1: Write the failing test for strictness**

In `src/midi/mod.rs` tests:

```rust
    #[test]
    fn notes_reject_unknown_and_removed_fields_by_name() {
        for (field, value) in [
            ("synth_type", json!("sine")),
            ("preset_name", json!("Minimoog Bass")),
            ("synth_attack", json!(0.1)),
            ("colour", json!("blue")),
        ] {
            let mut v = json!({"note": 60, "start_time": 0.0, "duration": 1.0});
            v[field] = value;
            let err = serde_json::from_value::<SimpleNote>(v).unwrap_err().to_string();
            assert!(err.contains(field), "{field}: {err}");
        }
    }

    #[test]
    fn r2d2_and_midi_notes_still_parse_with_every_documented_field() {
        let r2d2 = json!({"note_type": "r2d2", "r2d2_emotion": "Happy", "r2d2_intensity": 0.8,
            "r2d2_complexity": 2, "r2d2_pitch_range": [200.0, 800.0], "r2d2_context": "hi",
            "start_time": 0.0, "duration": 1.0, "effects": [{"type": "reverb"}]});
        assert!(serde_json::from_value::<SimpleNote>(r2d2).is_ok());
        let midi = json!({"note": 60, "velocity": 90, "channel": 2, "instrument": 5, "reverb": 40,
            "chorus": 10, "volume": 100, "pan": 64, "balance": 64, "expression": 100, "sustain": 0,
            "musical_time": {"bar": 1, "beat": 1, "tick": 0}, "musical_duration": "quarter",
            "effects_preset": "studio"});
        assert!(serde_json::from_value::<SimpleNote>(midi).is_ok());
    }
```

- [ ] **Step 2: Remove fields and validators**

In `src/midi/mod.rs`:
- Add `#[serde(deny_unknown_fields)]` on `SimpleNote`.
- Delete the `synth_*` and `preset_*` fields, their `Default` entries, `is_preset`, `validate_synthesis`, `validate_preset`, and the doc comments listing synth types.
- `is_synthesis` becomes `self.synth.is_some()`.
- Remove the `deserialize_null_default` uses that no longer have a field (keep the function if MIDI/R2D2 fields still use it).

- [ ] **Step 3: Remove the old renderer**

In `src/expressive/synth.rs`, keep only: `ExpressiveSynth` struct, `Default`, `impl ExpressiveSynth { SAMPLE_RATE, new, generate_r2d2_samples_with_contour }`, its imports, and the R2D2 tests (`descending_r2d2_contour_keeps_descending`). Delete everything else, including `percussion_config`. Run `rg "SynthType|SynthParams|EnvelopeParams|FilterParams|DX7Operator|PresetLibrary|ClassicSynthPreset|preset_name|synth_type" src tests` and fix every hit:

- `src/expressive/mod.rs`: drop `pub mod presets;` and `pub use presets::*;`. `git rm -r src/expressive/presets`.
- `src/midi/translate.rs`: delete `apply_preset_to_note`, `convert_simple_note_to_synth_params`, the `synth_type` branch, the `PresetLibrary` field and import, `ExpressiveSynth` only stays for R2D2. Delete the tests `presets_are_applied_and_unknown_presets_are_errors`, `synthesis_notes_become_buffers_at_bus_level`, `midi_notes_need_a_soundfont_but_synthesis_does_not`, `a_negative_duration_on_a_synthesis_note_does_not_panic`, and re-create the last three on the patch path:

```rust
    #[test]
    fn patch_notes_become_buffers_at_bus_level() {
        let t = Translator::new(Err("no soundfont".into()));
        let tr = t
            .translate(
                seq(vec![patch_note(json!({"name": "s", "level": 1.0, "subtractive": {"osc1": {"wave": "sine"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}), 69, 0.0, 0.5)]),
                PlayMode::Replace,
                &no_session(),
            )
            .unwrap();
        let peak = tr.command.buffers[0].1.iter().fold(0.0f32, |m, s| m.max(s[0].abs()));
        assert!(peak <= SYNTH_BUS_GAIN * 100.0 / 127.0 + 0.01 && peak > SYNTH_BUS_GAIN * 0.5);
    }

    #[test]
    fn midi_notes_need_a_soundfont_but_patches_do_not() {
        let t = Translator::new(Err("no soundfont".into()));
        assert!(t.translate(seq(vec![patch_note(json!("sub_bass"), 36, 0.0, 0.2)]), PlayMode::Replace, &no_session()).is_ok());
        let midi = SimpleNote { note: Some(60), ..Default::default() };
        assert!(t.translate(seq(vec![midi]), PlayMode::Replace, &no_session()).unwrap_err().contains("SoundFont"));
    }

    #[test]
    fn a_negative_duration_on_a_patch_note_does_not_panic() {
        let t = Translator::new(Err("no soundfont".into()));
        let n = patch_note(json!("sub_bass"), 36, 0.0, -1.0);
        assert!(t.translate(seq(vec![n]), PlayMode::Replace, &no_session()).is_ok());
    }
```

- `src/midi/player.rs` `level_tests`: replace `fn preset(name, notes)` with `fn patch(name: &str, notes: &[u8]) -> SimpleSequence` setting `synth: Some(SynthRef::Name(name.into()))`, and point the callers at `"minimoog_bass"`, `"jp_8_strings"`, `"tr_808_kick"` (check the existing calls and keep their intent). `measure` passes `&HashMap::new()` to `translate`.

- `src/server/mcp.rs` (compile-only changes; the real tool work is Task 11): in `validate_notes` replace the `synthesis`/`preset` checks with `("synth", note.validate_synth())`; in `describe_sources` replace the preset/custom-synthesis lines with `if notes.iter().any(|n| n.is_synthesis()) { parts.push("synth patches"); }` and change the MIDI predicate to `n.note_type != "r2d2" && !n.is_synthesis()`; delete `SYNTH_TYPES` and the `presets`/`synthesis` sections of `handle_list_sounds` (Task 11 adds `synths`); delete the `synth_*` and `preset_*` properties from `note_schema()` (Task 11 adds `synth`).

- [ ] **Step 4: Rewrite the demos**

Replace `src/demos.rs` with three demos using patches, and cut `main.rs` to three demo commands. In `src/main.rs`, the `Commands` enum keeps `Server`, `Setup`, and:

```rust
    /// Play every built-in synth patch (listen-by-ear check)
    #[command(name = "test-synths")]
    TestSynths,
    /// Play the synthesized drum patches
    #[command(name = "test-drums")]
    TestDrums,
    /// Play a MIDI piano dry, then through effect chains
    #[command(name = "test-effects")]
    TestEffects,
```

(match the attribute style already used on the existing variants) and the `match` arms call `demos::test_synths()`, `demos::test_drums()`, `demos::test_effects()`.

New `src/demos.rs`:

```rust
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
            let duration = player.play(patch_notes(&patch.name, phrase), PlayMode::Replace, &none)?;
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
    for (label, fx) in [("dry", None), ("reverb", Some(reverb)), ("delay", Some(delay))] {
        println!("🎹 piano chord: {label}");
        let duration = player.play(chord(fx), PlayMode::Replace, &none)?;
        sleep(duration);
    }
    Ok(())
}
```

Update `CLAUDE.md`'s "Listen-by-ear demos" bullets to the three commands.

- [ ] **Step 5: Build, fix, and run everything**

```bash
cargo build 2>&1 | grep -E "^(error|warning)" | head -30
cargo test 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Iterate until clean. Integration tests referencing `synth_type`/`preset_name` (`unknown_preset_is_a_tool_error_not_a_piano`, `custom_effects_chains_are_accepted_in_both_forms`, `consecutive_plays_layer_or_replace_and_stop_reports_the_count`, `list_sounds_catalog_names_everything`) will fail; they are rewritten in Task 12. It is acceptable for this commit to leave exactly those integration tests red, but all unit tests must pass and clippy must be clean. State this in the commit message.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add -A
git commit -m "refactor!: remove synth_* fields and Rust presets in favour of patches

Integration tests that used the removed fields are rewritten in the
following commit; unit tests and clippy are clean."
```

---

### Task 11: `define_synth` tool, `synth` schema, `list_sounds` synths section

**Files:**
- Modify: `src/server/mcp.rs`

**Interfaces:**
- Consumes: `Patch`, `PatchLibrary`, `SynthRef` (Tasks 3, 7); `MidiPlayer::play(seq, mode, &session)` (Task 9).
- Produces: `ServerState.synths: HashMap<String, Patch>`; tool `define_synth`; `note_schema().synth`; `list_sounds` section `synths`.

- [ ] **Step 1: Write the failing unit tests**

`mcp.rs` has no unit tests for handlers today; add a small tests module at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(state: &mut ServerState, tool: &str, args: Value) -> JsonRpcResponse {
        dispatch_tool(
            state,
            ToolCallParams {
                name: tool.to_string(),
                arguments: args,
            },
            Some(json!(1)),
        )
    }

    fn text(r: &JsonRpcResponse) -> String {
        r.result.as_ref().unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn define_synth_stores_a_validated_patch_and_echoes_usage() {
        let mut state = ServerState::new();
        let r = call(&mut state, "define_synth", json!({"name": "Blip", "subtractive": {"osc1": {"wave": "square"}}}));
        assert!(r.error.is_none(), "{:?}", r.error);
        let t = text(&r);
        assert!(t.contains("Blip") && t.contains("\"synth\": \"Blip\""), "{t}");
        assert!(state.synths.contains_key("blip"));
    }

    #[test]
    fn define_synth_rejects_unknown_fields_as_invalid_params_and_ranges_as_tool_errors() {
        let mut state = ServerState::new();
        let r = call(&mut state, "define_synth", json!({"name": "x", "subtractive": {"cutoff": 1}}));
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("cutoff"));

        let r = call(&mut state, "define_synth", json!({"name": "x", "subtractive": {"filter": {"cutoff": 1}}}));
        assert!(r.error.is_none());
        assert_eq!(r.result.as_ref().unwrap()["isError"], true);
        assert!(text(&r).contains("subtractive.filter.cutoff"));
        assert!(state.synths.is_empty());
    }

    #[test]
    fn list_sounds_synths_section_names_builtins_and_session_patches() {
        let mut state = ServerState::new();
        call(&mut state, "define_synth", json!({"name": "mine", "percussion": {"kind": "snare"}}));
        let r = handle_list_sounds(&state, json!({"section": "synths"}), Some(json!(1)));
        let t = text(&r);
        assert!(t.contains("minimoog_bass") && t.contains("tr_808_kick") && t.contains("mine"), "{t}");
        assert!(!t.contains("Acoustic Grand Piano"));
    }

    #[test]
    fn tools_list_has_seven_tools_and_the_note_schema_has_synth() {
        let r = handle_tools_list(Some(json!(1)));
        let tools = r.result.unwrap()["tools"].clone();
        assert_eq!(tools.as_array().unwrap().len(), 7);
        let names: Vec<&str> = tools.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"define_synth"));
        let schema = note_schema();
        assert!(schema["properties"]["synth"].is_object());
        assert!(schema["properties"].get("synth_type").is_none());
        assert!(schema["properties"].get("preset_name").is_none());
    }
}
```

Check the actual field names of `JsonRpcResponse`'s error struct (`code`, `message`) and `ToolCallParams` (`name`, `arguments`) in the file and adjust the test to match.

- [ ] **Step 2: Run to verify failure**

`cargo test server::mcp 2>&1 | tail`. Expected: no field `synths`, `define_synth` unknown, `handle_list_sounds` arity.

- [ ] **Step 3: Implement**

In `src/server/mcp.rs`:

1. `ServerState` gains `synths: HashMap<String, Patch>` (key = `patch.key()`), initialised empty. Import `crate::expressive::{Patch, PatchLibrary}`.

2. `start_playback` passes `&state.synths` to `player.play(sequence, mode, &synths)`. Because `state.player()` borrows `state` mutably, clone the map first: `let synths = state.synths.clone();`.

3. Add a `patch_schema()` function returning the JSON schema for a patch (used by `define_synth` and inline `synth`). Field descriptions carry ranges and defaults from the spec:

```rust
/// JSON schema for a synth patch, shared by define_synth and inline `synth` on notes.
fn patch_schema() -> Value {
    let env = |what: &str| json!({
        "type": "object",
        "description": format!("{what} envelope in seconds (0.001-10) and sustain 0-1. Defaults: attack 0.01, decay 0.1, sustain 0.8, release 0.3"),
        "properties": {
            "attack": {"type": "number", "minimum": 0, "maximum": 10},
            "decay": {"type": "number", "minimum": 0, "maximum": 10},
            "sustain": {"type": "number", "minimum": 0, "maximum": 1},
            "release": {"type": "number", "minimum": 0, "maximum": 10}
        },
        "additionalProperties": false
    });
    let wave = json!({"type": "string", "enum": ["sine", "saw", "square", "triangle", "noise"], "default": "saw"});
    json!({
        "type": "object",
        "description": "A synth patch. Include at least one engine (subtractive or percussion); several may layer.",
        "properties": {
            "name": {"type": "string", "description": "Patch name; notes reference it with \"synth\": \"<name>\""},
            "description": {"type": "string"},
            "category": {"type": "string", "enum": ["bass", "pad", "lead", "keys", "drums", "fx"]},
            "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1, "description": "Patch output level; velocity 127 plays at this level"},
            "subtractive": {
                "type": "object",
                "description": "Two oscillators, optional state-variable filter with its own envelope, amplitude envelope",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "osc1": {"type": "object", "properties": {"wave": wave, "pulse_width": {"type": "number", "minimum": 0.1, "maximum": 0.9, "default": 0.5}}, "additionalProperties": false},
                    "osc2": {"type": "object", "description": "Second oscillator blended with osc1",
                        "properties": {"wave": wave, "pulse_width": {"type": "number", "minimum": 0.1, "maximum": 0.9},
                            "mix": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "0 = only osc1, 1 = only osc2"},
                            "detune_cents": {"type": "number", "minimum": -100, "maximum": 100, "default": 0, "description": "5-15 thickens, 50+ beats audibly"},
                            "octave": {"type": "integer", "minimum": -2, "maximum": 2, "default": 0}},
                        "additionalProperties": false},
                    "filter": {"type": "object",
                        "properties": {"type": {"type": "string", "enum": ["low_pass", "high_pass", "band_pass"], "default": "low_pass"},
                            "cutoff": {"type": "number", "minimum": 20, "maximum": 20000, "default": 1000},
                            "resonance": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.2},
                            "slope": {"type": "integer", "enum": [12, 24], "default": 12, "description": "dB per octave"},
                            "env_amount": {"type": "number", "minimum": -1, "maximum": 1, "default": 0, "description": "Filter envelope depth; 1 sweeps up four octaves, -1 down"},
                            "env": env("Filter")},
                        "additionalProperties": false},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
            "percussion": {
                "type": "object",
                "description": "One-shot hit with its own envelope; ignores the note's pitch. Only the parameters of the chosen kind are allowed.",
                "properties": {
                    "kind": {"type": "string", "enum": ["kick", "snare", "hihat", "cymbal", "zap", "swoosh", "chime", "burst"]},
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "frequency": {"type": "number", "minimum": 20, "maximum": 20000, "description": "Body (kick 60), tone (snare 200), base (hihat 8000, cymbal 4000), start (zap 800), fundamental (chime 880) or centre (burst 1000)"},
                    "punch": {"type": "number", "minimum": 0, "maximum": 1, "description": "kick"},
                    "sustain": {"type": "number", "minimum": 0, "maximum": 1, "description": "kick"},
                    "click_freq": {"type": "number", "minimum": 20, "maximum": 20000, "description": "kick"},
                    "snap": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "buzz": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "noise_amount": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "metallic": {"type": "number", "minimum": 0, "maximum": 1, "description": "hihat, cymbal"},
                    "decay": {"type": "number", "minimum": 0.01, "maximum": 10, "description": "hihat, zap, chime (seconds)"},
                    "brightness": {"type": "number", "minimum": 0, "maximum": 1, "description": "hihat"},
                    "size": {"type": "number", "minimum": 0, "maximum": 1, "description": "cymbal"},
                    "strike_intensity": {"type": "number", "minimum": 0, "maximum": 1, "description": "cymbal"},
                    "energy": {"type": "number", "minimum": 0, "maximum": 1, "description": "zap"},
                    "harmonic_content": {"type": "number", "minimum": 0, "maximum": 1, "description": "zap"},
                    "direction": {"type": "number", "minimum": -1, "maximum": 1, "description": "swoosh"},
                    "intensity": {"type": "number", "minimum": 0, "maximum": 1, "description": "swoosh, burst"},
                    "sweep": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "swoosh [start_hz, end_hz]"},
                    "harmonic_count": {"type": "integer", "minimum": 1, "maximum": 16, "description": "chime"},
                    "inharmonicity": {"type": "number", "minimum": 0, "maximum": 1, "description": "chime"},
                    "bandwidth": {"type": "number", "minimum": 1, "maximum": 20000, "description": "burst"},
                    "shape": {"type": "number", "minimum": 0, "maximum": 1, "description": "burst: 0 sharp, 1 smooth"}
                },
                "required": ["kind"],
                "additionalProperties": false
            },
            "effects": effects_schema()
        },
        "required": ["name"],
        "additionalProperties": false
    })
}
```

Extract the existing `effects` property definition from `note_schema()` into `fn effects_schema() -> Value` and use it in both places.

4. In `note_schema()`, add:

```rust
            "synth": {
                "description": "🎛️ Synth patch for this note: the name of a built-in or define_synth patch, or an inline patch object. Pitch comes from `note`; percussion patches ignore it.",
                "oneOf": [
                    {"type": "string"},
                    patch_schema()
                ]
            },
```

5. Add the `define_synth` tool to `handle_tools_list` before `play_notes`:

```rust
        {
            "name": "define_synth",
            "description": "Define a reusable synth patch for this session, then play it with \"synth\": \"<name>\" on notes in play_notes, define_sequence_pattern or play_sequence. Engines: subtractive (two oscillators, filter with envelope) and percussion (kick/snare/hihat/cymbal/zap/swoosh/chime/burst). An ordered effects chain applies once to all notes of the patch, so reverb and delay tails are shared.

Examples:
- Bass: {\"name\": \"rubber_bass\", \"category\": \"bass\", \"subtractive\": {\"osc1\": {\"wave\": \"saw\"}, \"osc2\": {\"wave\": \"square\", \"mix\": 0.3, \"detune_cents\": 6}, \"filter\": {\"type\": \"low_pass\", \"cutoff\": 500, \"resonance\": 0.4, \"slope\": 24, \"env_amount\": 0.7, \"env\": {\"attack\": 0.005, \"decay\": 0.25, \"sustain\": 0.1, \"release\": 0.2}}, \"env\": {\"attack\": 0.005, \"decay\": 0.3, \"sustain\": 0.6, \"release\": 0.15}}, \"effects\": [{\"type\": \"distortion\", \"drive\": 3, \"intensity\": 0.4}, {\"type\": \"compressor\", \"threshold\": -18, \"ratio\": 4, \"intensity\": 1}]}
- Pad: {\"name\": \"glass_pad\", \"category\": \"pad\", \"level\": 0.7, \"subtractive\": {\"osc1\": {\"wave\": \"triangle\"}, \"osc2\": {\"wave\": \"saw\", \"mix\": 0.35, \"detune_cents\": 9}, \"filter\": {\"cutoff\": 900, \"env_amount\": 0.5, \"env\": {\"attack\": 1.2, \"decay\": 2, \"sustain\": 0.4, \"release\": 3}}, \"env\": {\"attack\": 0.9, \"decay\": 1, \"sustain\": 0.8, \"release\": 2.5}}, \"effects\": [{\"type\": \"chorus\", \"rate\": 0.5, \"depth\": 0.4, \"intensity\": 0.3}, {\"type\": \"reverb\", \"room_size\": 0.8, \"intensity\": 0.4}]}
- Drum: {\"name\": \"tight_kick\", \"category\": \"drums\", \"percussion\": {\"kind\": \"kick\", \"frequency\": 50, \"punch\": 0.9, \"sustain\": 0.2}}

Call list_sounds with section \"synths\" to see the built-in patches, which double as worked examples.",
            "inputSchema": patch_schema()
        },
```

Update `play_notes`'s description: replace "synthesis (19 types)" with "synth patches (built-in or define_synth)" and the kick example with `[{"synth": "tr_808_kick", "duration": 0.5}]`, plus one inline example: `[{"synth": {"name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}}, "note": 84, "duration": 0.1}]`. Update `define_sequence_pattern`'s and `play_sequence`'s descriptions where they mention presets.

6. Handler and dispatch:

```rust
        "define_synth" => handle_define_synth(state, tool_params.arguments, id),
```

```rust
fn handle_define_synth(state: &mut ServerState, arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    let patch: Patch = match serde_json::from_value(arguments) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, format!("Failed to parse synth patch: {}", e));
        }
    };
    if let Err(e) = patch.validate() {
        return JsonRpcResponse::tool_error(id, format!("Invalid synth patch '{}': {}", patch.name, e));
    }
    let mut engines = Vec::new();
    if patch.subtractive.as_ref().is_some_and(|s| s.level > 0.0) {
        engines.push("subtractive");
    }
    if let Some(p) = patch.percussion.as_ref().filter(|p| p.level > 0.0) {
        engines.push(p.kind.as_str());
    }
    let shadowed = PatchLibrary::new().get(&patch.name).is_some();
    let mut details = format!(
        "🎛️ Defined synth '{}': {} engine(s) [{}], {} effect(s){}",
        patch.name,
        engines.len(),
        engines.join(", "),
        patch.effects.iter().filter(|e| e.enabled).count(),
        if shadowed { " (shadows the built-in patch of the same name for this session)" } else { "" }
    );
    if !patch.description.is_empty() {
        details.push_str(&format!("\n{}", patch.description));
    }
    details.push_str(&format!(
        "\n\nPlay it with play_notes, e.g. {{\"notes\": [{{\"synth\": \"{}\", \"note\": 48, \"duration\": 1}}]}}",
        patch.name
    ));
    tracing::info!("Stored synth patch '{}'", patch.name);
    state.synths.insert(patch.key(), patch);
    JsonRpcResponse::tool_text(id, details)
}
```

Construct `PatchLibrary` once per `ServerState` instead if `PatchLibrary::new()` shows up in profiles; parsing 31 small JSON files per call is fine for now.

7. `handle_list_sounds(state: &ServerState, arguments, id)` (pass `state` from `dispatch_tool`), section enum gains `"synths"` and loses `"presets"`/`"synthesis"`:

```rust
    if want("synths") {
        let library = PatchLibrary::new();
        out.push_str(&format!(
            "# Synth patches ({} built-in) — use \"synth\": \"<name>\" on a note, or define_synth for your own\n",
            library.count()
        ));
        for (category, patches) in library.catalog() {
            out.push_str(&format!("\n## {} ({})\n", category.as_str(), patches.len()));
            for patch in patches {
                out.push_str(&format!("- {} — {}\n", patch.name, patch.description));
            }
        }
        if !state.synths.is_empty() {
            let mut mine: Vec<&Patch> = state.synths.values().collect();
            mine.sort_by(|a, b| a.name.cmp(&b.name));
            out.push_str(&format!("\n## defined this session ({})\n", mine.len()));
            for patch in mine {
                out.push_str(&format!("- {} — {}\n", patch.name, patch.description));
            }
        }
        out.push_str("\nPercussion kinds for inline patches: kick, snare, hihat, cymbal, zap, swoosh, chime, burst.\n\n");
    }
```

Update the `list_sounds` tool schema enum to `["all", "synths", "instruments", "drums", "r2d2", "effects"]` and its description. In the `drums` section, change the trailing sentence to "Synthesized drums are the tr_808_kick, tr_909_snare, tr_909_hihat, tr_808_hihat and crash_cymbal patches, or a percussion patch of your own."

- [ ] **Step 4: Run the tests**

```bash
cargo test server::mcp 2>&1 | tail -15
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/server/mcp.rs
git commit -m "feat(mcp): define_synth tool, synth on notes, synths catalog"
```

---

### Task 12: Integration tests through the server binary

**Files:**
- Modify: `tests/integration/mcp_protocol.rs`

- [ ] **Step 1: Rewrite the tests that used removed fields**

Using the existing `TestServer` helper (`start`, `call`) in that file:

- `unknown_preset_is_a_tool_error_not_a_piano` → rename to `unknown_synth_is_a_tool_error_not_a_piano`; the note becomes `{"synth": "definitely_not_a_patch", "note": 60, "start_time": 0.0, "duration": 0.2}`; keep the existing "audio device unavailable or rejected" tolerance, but when it is a rejection assert the text contains `definitely_not_a_patch`.
- `custom_effects_chains_are_accepted_in_both_forms`: replace `"synth_type": "sawtooth"` with `"synth": "saw_bass"`.
- `consecutive_plays_layer_or_replace_and_stop_reports_the_count`: `let note = json!([{"synth": "sub_bass", "note": 36, "duration": 3.0}]);`.
- `list_sounds_catalog_names_everything`: needles become `["minimoog_bass", "tr_808_kick", "Acoustic Grand Piano", "Closed Hi-Hat", "Happy", "studio", "define_synth"]`; the second call uses `{"section": "synths"}` and asserts it contains `sub_bass` and not `Acoustic Grand Piano`.
- `test_mcp_tools_list`: if it asserts a tool count or name list, add `define_synth` (7 tools).

- [ ] **Step 2: Add new tests**

```rust
#[test]
fn define_synth_then_play_by_name() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 10, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {
            "name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("blip") && r["result"]["isError"] != true, "{text}");

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 11, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": "blip", "note": 84, "start_time": 0.0, "duration": 0.1}]}}
    }));
    // Either playback started, or the CI box has no audio device; never a parse error.
    assert!(r["error"].is_null(), "{r}");
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Playback started") || text.contains("Audio output unavailable"), "{text}");
}

#[test]
fn inline_synth_patches_are_accepted_and_invalid_ones_are_tool_errors() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 12, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "k", "percussion": {"kind": "kick", "punch": 0.9}}, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 13, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "k", "percussion": {"kind": "kick", "snap": 0.9}}, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("snap"));
}

#[test]
fn removed_synth_fields_are_invalid_params() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 14, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth_type": "sine", "synth_frequency": 440, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("synth_type"));
}

#[test]
fn define_synth_with_an_unknown_field_is_invalid_params() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 15, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "subtractive": {"cutoff": 500}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("cutoff"));
}

#[test]
fn patterns_can_carry_synth_patches() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 16, "method": "tools/call",
        "params": {"name": "define_sequence_pattern", "arguments": {
            "name": "kick4", "pattern_bars": 1, "notes": [
                {"synth": "tr_808_kick", "musical_time": {"bar": 1, "beat": 1, "tick": 0}, "musical_duration": "quarter"},
                {"synth": "tr_808_kick", "musical_time": {"bar": 1, "beat": 3, "tick": 0}, "musical_duration": "quarter"}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 17, "method": "tools/call",
        "params": {"name": "play_sequence", "arguments": {"patterns": [{"pattern_name": "kick4", "start_bar": 1, "repeat_count": 2}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
}
```

- [ ] **Step 3: Run the integration tests**

```bash
cargo test --test integration_tests 2>&1 | tail -20
```

Expected: all pass (playback-dependent tests tolerate a missing audio device as the existing ones do).

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add tests
git commit -m "test: integration coverage for define_synth and synth patches"
```

---

### Task 13: README and CLAUDE.md

**Files:**
- Modify: `README.md`, `CLAUDE.md`

- [ ] **Step 1: README**

Replace the sections "🎹 NEW: Classic Synthesizer Preset Examples" and "🎛️ NEW: Custom Synthesis Examples" (lines ~248-322) with one section:

````markdown
## 🎛️ Synth Patches (agent-defined instruments)

Every synthesized sound is a **patch**: a JSON object with one or more engines, envelopes and an effects chain. Use a built-in patch by name, define your own once with `define_synth`, or pass a patch inline on a note. All notes of a patch in one call share its effects, so reverb and delay tails are real.

### Built-in patch by name
```json
{"notes": [
  {"synth": "minimoog_bass", "note": 36, "velocity": 120, "start_time": 0, "duration": 1},
  {"synth": "minimoog_bass", "note": 43, "velocity": 100, "start_time": 1, "duration": 1},
  {"synth": "tr_808_kick", "start_time": 0, "duration": 0.5},
  {"synth": "tr_909_snare", "start_time": 0.5, "duration": 0.3}
]}
```
Call `list_sounds` with `{"section": "synths"}` for the full list (bass, pad, lead, drums, fx).

### Define your own
```json
{"name": "rubber_bass", "category": "bass",
 "subtractive": {
   "osc1": {"wave": "saw"},
   "osc2": {"wave": "square", "mix": 0.3, "detune_cents": 6},
   "filter": {"type": "low_pass", "cutoff": 500, "resonance": 0.4, "slope": 24,
              "env_amount": 0.7, "env": {"attack": 0.005, "decay": 0.25, "sustain": 0.1, "release": 0.2}},
   "env": {"attack": 0.005, "decay": 0.3, "sustain": 0.6, "release": 0.15}},
 "effects": [{"type": "distortion", "drive": 3, "intensity": 0.4},
             {"type": "compressor", "threshold": -18, "ratio": 4, "intensity": 1}]}
```
Then `{"notes": [{"synth": "rubber_bass", "note": 36, "duration": 0.5}]}`.

### Engines
- **subtractive**: `osc1`/`osc2` (`sine|saw|square|triangle|noise`, `pulse_width`, osc2 `mix`, `detune_cents`, `octave`), `filter` (`low_pass|high_pass|band_pass`, `cutoff`, `resonance` 0-1, `slope` 12|24, `env_amount` -1..1 with its own `env`), amplitude `env`.
- **percussion**: `kind` `kick|snare|hihat|cymbal|zap|swoosh|chime|burst` with that kind's parameters (`punch`, `snap`, `metallic`, `sweep`, ...) and a `frequency`. Ignores the note's pitch.
- **effects**: ordered chain of `reverb`, `delay`, `chorus`, `filter`, `compressor`, `distortion`, each with an `intensity` 0-1.
````

Also fix the "Ultimate Victory Celebration" example (line ~329) to use `"synth": "jp_8_strings"` and `"synth": "chime"`, remove "synthesis (19 types)"/"29 presets" phrasing wherever it appears (`rg -n "preset|synth_type|19 types" README.md`), and update the tool list to seven tools with `define_synth`.

- [ ] **Step 2: CLAUDE.md**

- Tools list: "Seven tools", add `- define_synth - store a validated synth patch for the session; notes reference it by name via synth`.
- Audio pipeline step 2: "Synthesis notes are grouped by patch, rendered on the tool thread by `render_patch` into one stereo buffer per patch (voices summed, the patch's effects chain applied once, `SYNTH_BUS_GAIN` applied) and scheduled as buffers."
- Step 1: drop "applies presets (a missing preset is an error)"; write "resolves `synth` references (session patches, then built-ins, then inline; an unknown name is an error)".
- Synthesis section: replace the `synth.rs` bullet with the new module list (`patch.rs`, `envelope.rs`, `oscillator.rs`, `engines/`, `render.rs`, `patches/*.json`; `synth.rs` is R2D2 only) and remove the `presets/` bullet.
- Data model: "`SimpleNote` is one flat struct covering MIDI, R2D2 and a `synth` patch reference; it rejects unknown fields."
- Demos: the three commands from Task 10.

- [ ] **Step 3: Verify and commit**

```bash
rg -n "preset_name|synth_type|synth_attack|29 classic|19 types" README.md CLAUDE.md src tests || echo "clean"
cargo test 2>&1 | tail -5
git add README.md CLAUDE.md
git commit -m "docs: synth patches replace presets and synth_* fields"
```

Expected: `clean`, all tests pass.

---

### Task 14: Final verification and PR

- [ ] **Step 1: Full verification**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -15
cargo run -- test-synths   # listen once; every built-in patch should sound and none should distort
```

- [ ] **Step 2: Update the spec status line**

In `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md`, change `Status:` to `PR 1 (foundation) implemented; PRs 2-4 pending`. Commit with `docs: spec status after PR 1`.

- [ ] **Step 3: Open the PR**

Use the `superpowers:finishing-a-development-branch` skill. PR title: `feat: agent-defined synth patches (foundation)`. Body: link the spec and this plan, list the removed fields and the migration (`preset_name: "Minimoog Bass"` becomes `synth: "minimoog_bass"`; `synth_type: "kick"` becomes `synth: "tr_808_kick"` or an inline percussion patch), and note that FM/wavetable (PR 2), granular/LFO (PR 3) and Time Fracture (PR 4) follow.

---

## Self-review notes

- Spec coverage for PR 1: §1 model (Tasks 3, 7), §2 tools (Task 11, 12), §3 rendering and stereo buffers (Tasks 6, 8, 9), §4 subtractive and percussion DSP (Tasks 4, 5), §5 built-ins (Task 7), §6 validation (Tasks 3, 9, 11), §7 tests (each task; wavetable/FM/granular/LFO/Time Fracture tests belong to PRs 2-4), §8 sequencing (this plan is PR 1 including the minimal demo rewrite). `deny_unknown_fields` on `SimpleNote` is Task 10.
- Deferred to later PRs by design: `fm`, `wavetable`, `granular`, `lfo` fields (a patch using them is a `-32602` today, which the schema makes visible), Time Fracture delay fields, DX7 E.Piano / DX7 Slap Bass / TX81Z Lately patches.
- Names used consistently: `Adsr`, `GateEnvelope`, `Wave`, `wave_sample`, `Patch`, `Subtractive`, `Osc`, `Osc2`, `Filter`, `FilterKind`, `Percussion`, `PercussionKind`, `SynthRef`, `PatchCategory`, `PatchLibrary`, `Voice`, `Modulation`, `SubtractiveVoice`, `PercussionVoice`, `NoteEvent`, `render_patch`, `render_length_seconds`, `EFFECT_TAIL_SECONDS`, `Translator::translate(seq, mode, &session)`, `MidiPlayer::play(seq, mode, &session)`, `ServerState.synths`, `handle_define_synth`, `patch_schema`, `effects_schema`.
