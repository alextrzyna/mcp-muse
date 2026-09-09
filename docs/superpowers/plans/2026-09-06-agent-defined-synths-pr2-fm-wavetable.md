# Agent-Defined Synths, PR 2 (FM and Wavetable Engines) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `fm` (4-operator, 4 algorithms) and `wavetable` (8 band-limited tables with morph) engines to synth patches, expose them in the `define_synth` schema, and ship the FM presets that PR 1 could not port plus new wavetable patches.

**Architecture:** Two new `Voice` implementations under `src/expressive/engines/` plug into the existing `render_patch` loop; two new optional engine sections on `Patch` with validation; a `wavetables.rs` module builds per-octave band-limited tables once (`OnceLock`) from partial lists. Nothing on the audio thread changes.

**Tech Stack:** Rust 2024, serde/serde_json, existing `GateEnvelope`, `PhaseAccumulator`, `test_util` measurements. Commands: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.

**Spec:** `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md` (§1 `fm` and `wavetable`, §4 DSP table, §5 built-ins, §8 PR 2).

## Global Constraints

- DSP is verified by measurement (`src/expressive/test_util.rs`: `goertzel_power`, `rms`, `zero_crossing_rate`, `db`), never by ear.
- Never allocate or log inside per-sample loops on the audio thread (`src/midi/engine.rs`); this PR does not touch it. Offline rendering (`render_patch`) may allocate before the loop, not inside it.
- Patch JSON is snake_case; every patch struct carries `#[serde(deny_unknown_fields)]`; validation errors name the field path and range (`check_range` in `patch.rs`) and surface as `-32602`.
- Envelope defaults: attack 0.01, decay 0.1, sustain 0.8, release 0.3 (`Adsr::default`).
- Spec §1 `fm`: `algorithm` `stack | pairs | fan_in | parallel`; `operators` 1 to 4 entries of `ratio` 0.25..16, `level` 0..1, `detune_cents` -100..100, `env`; operator 1 (first entry) is always a carrier; a modulator's `level` is its modulation depth, a carrier's `level` its output gain; `feedback` 0..1 on the last operator.
- Spec §1 `wavetable`: `table` `basic | warm | bright | digital | vocal | pwm | organ | noise`; `morph` 0..1 blends toward the next table in that order (wrapping); `env`; tables are band-limited per octave so high notes alias less than a naive table.
- Built-in patches peak ≤ 1.6 before the bus gain at velocity 100/127 (the existing `every_builtin_patch_parses_validates_and_renders_cleanly` test enforces it).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass before every commit.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jxcj13gt2pyZbmDvvC1Pep
  ```
- Work in the worktree `.claude/worktrees/feat-synth-fm-wavetable` (branch `worktree-feat-synth-fm-wavetable`, from `main` 31f8e0f).

---

## File map

| File | Responsibility |
|---|---|
| `src/expressive/patch.rs` | `Fm`, `Operator`, `FmAlgorithm`, `Wavetable`, `TableName`; new `Patch.fm` / `Patch.wavetable`; validation; `release_seconds` / `has_pitched_engine` over all engines; `BUILTIN_PATCHES` additions |
| `src/expressive/engines/fm.rs` (new) | `FmVoice` |
| `src/expressive/wavetables.rs` (new) | partial lists for the 8 tables, per-octave band-limited mip tables, `OnceLock` cache, `sample(table, level, phase)` |
| `src/expressive/engines/wavetable.rs` (new) | `WavetableVoice` |
| `src/expressive/engines/mod.rs` | module list and re-exports |
| `src/expressive/mod.rs` | `pub mod wavetables;` |
| `src/expressive/render.rs` | spawn FM and wavetable voices |
| `src/expressive/patches/*.json` | 8 new patches |
| `src/server/mcp.rs` | `patch_schema` gains `fm` and `wavetable`; descriptions; `handle_define_synth` engine list; `list_sounds` hint |
| `src/demos.rs` | keys phrase |
| `tests/integration/mcp_protocol.rs` | FM/wavetable through the binary |
| `README.md`, `CLAUDE.md`, spec | docs |

---

### Task 1: FM data model and validation

**Files:**
- Modify: `src/expressive/patch.rs`

**Interfaces:**
- Consumes: `Adsr` (has `validate(&self, path: &str)`), `check_range(path, value, min, max)` (private helper already in `patch.rs`).
- Produces:
  ```rust
  #[serde(rename_all = "snake_case")] pub enum FmAlgorithm { Stack, Pairs, FanIn, Parallel }  // Default = Stack
  impl FmAlgorithm { pub fn as_str(&self) -> &'static str; pub fn modulators(&self, op: usize) -> &'static [usize]; pub fn carriers(&self) -> &'static [usize]; }
  pub struct Operator { pub ratio: f32, pub level: f32, pub detune_cents: f32, pub env: Adsr }  // Default: 1.0, 1.0, 0.0, Adsr::default()
  pub struct Fm { pub level: f32, pub algorithm: FmAlgorithm, pub feedback: f32, pub operators: Vec<Operator> }  // Default: 1.0, Stack, 0.0, vec![Operator::default()]
  impl Fm { pub fn carrier_release(&self) -> f32; }   // longest release among carrier operators that exist
  pub struct Patch { ..., pub fm: Option<Fm>, ... }
  ```
  `modulators(op)` and `carriers()` use 0-based operator indices (operator 1 = index 0), so `Stack.modulators(0) == [1]`, `Stack.modulators(1) == [2]`, `Stack.modulators(2) == [3]`, `Stack.modulators(3) == []`, `Stack.carriers() == [0]`; `Pairs`: `modulators(0) == [2]`, `modulators(1) == [3]`, carriers `[0, 1]`; `FanIn`: `modulators(0) == [1, 2, 3]`, carriers `[0]`; `Parallel`: no modulators, carriers `[0, 1, 2, 3]`.

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `src/expressive/patch.rs`:

```rust
    #[test]
    fn fm_patch_parses_with_defaults_and_round_trips() {
        let p = parse(json!({"name": "op", "fm": {}})).unwrap();
        let fm = p.fm.as_ref().unwrap();
        assert_eq!(fm.level, 1.0);
        assert_eq!(fm.algorithm, FmAlgorithm::Stack);
        assert_eq!(fm.feedback, 0.0);
        assert_eq!(fm.operators.len(), 1);
        assert_eq!(fm.operators[0].ratio, 1.0);
        assert_eq!(fm.operators[0].level, 1.0);
        assert!(p.validate().is_ok());
        assert!(p.has_pitched_engine());

        let v = json!({"name": "bell", "fm": {"level": 0.8, "algorithm": "fan_in", "feedback": 0.2,
            "operators": [
                {"ratio": 1.0, "level": 1.0, "env": {"release": 2.0}},
                {"ratio": 3.5, "level": 0.6, "detune_cents": 3, "env": {"decay": 0.5, "sustain": 0.0}}
            ]}});
        let p = parse(v).unwrap();
        assert!(p.validate().is_ok());
        let back = serde_json::to_value(&p).unwrap();
        assert_eq!(back["fm"]["algorithm"], "fan_in");
        assert_eq!(back["fm"]["operators"][1]["ratio"], 3.5);
        assert_eq!(p.release_seconds(), 2.0, "release comes from the carrier");
    }

    #[test]
    fn fm_validation_names_the_field() {
        let p = parse(json!({"name": "x", "fm": {"operators": []}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.operators"), "empty operators");
        let five: Vec<serde_json::Value> = (0..5).map(|_| json!({})).collect();
        let p = parse(json!({"name": "x", "fm": {"operators": five}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.operators"), "too many operators");
        let p = parse(json!({"name": "x", "fm": {"operators": [{"ratio": 20}]}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(err.contains("fm.operators[0].ratio") && err.contains("16"), "{err}");
        let p = parse(json!({"name": "x", "fm": {"operators": [{}, {"detune_cents": 150}]}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.operators[1].detune_cents"));
        let p = parse(json!({"name": "x", "fm": {"feedback": 2}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.feedback"));
        let p = parse(json!({"name": "x", "fm": {"operators": [{"env": {"sustain": 3}}]}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.operators[0].env.sustain"));
        assert!(parse(json!({"name": "x", "fm": {"algorithm": "serial"}})).is_err(), "unknown algorithm");
        assert!(parse(json!({"name": "x", "fm": {"mod_index": 2}})).unwrap_err().contains("mod_index"));
    }

    #[test]
    fn fm_algorithm_tables_match_the_spec() {
        assert_eq!(FmAlgorithm::Stack.modulators(0), &[1]);
        assert_eq!(FmAlgorithm::Stack.modulators(2), &[3]);
        assert_eq!(FmAlgorithm::Stack.modulators(3), &[] as &[usize]);
        assert_eq!(FmAlgorithm::Stack.carriers(), &[0]);
        assert_eq!(FmAlgorithm::Pairs.modulators(0), &[2]);
        assert_eq!(FmAlgorithm::Pairs.modulators(1), &[3]);
        assert_eq!(FmAlgorithm::Pairs.carriers(), &[0, 1]);
        assert_eq!(FmAlgorithm::FanIn.modulators(0), &[1, 2, 3]);
        assert_eq!(FmAlgorithm::FanIn.carriers(), &[0]);
        assert_eq!(FmAlgorithm::Parallel.carriers(), &[0, 1, 2, 3]);
        for op in 0..4 {
            assert!(FmAlgorithm::Parallel.modulators(op).is_empty());
        }
    }

    #[test]
    fn release_seconds_is_the_longest_across_engines() {
        let p = parse(json!({"name": "both",
            "subtractive": {"env": {"release": 0.5}},
            "fm": {"operators": [{"env": {"release": 1.5}}, {"env": {"release": 9.0}}]}})).unwrap();
        // operator 2 is a modulator in `stack`, so its 9 s release does not count.
        assert_eq!(p.release_seconds(), 1.5);
    }
```

Also update the existing `validation_names_the_field_and_the_range` test's "no engines" expectation if it checks the exact message: the message now lists all engines (see Step 3).

- [ ] **Step 2: Run to verify failure**

```bash
cargo test patch:: 2>&1 | tail -20
```

Expected: compile errors (`FmAlgorithm`, `fm` field missing).

- [ ] **Step 3: Implement**

In `src/expressive/patch.rs`, after the `Filter` struct:

```rust
/// Operator routing. Indices are 0-based (operator 1 = index 0); operator 1
/// is always a carrier. Modulators always have higher indices than the
/// operators they modulate, so evaluating operators from the last to the
/// first resolves every modulator before it is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FmAlgorithm {
    /// 4 -> 3 -> 2 -> 1; one carrier.
    #[default]
    Stack,
    /// 3 -> 1 and 4 -> 2; carriers 1 and 2.
    Pairs,
    /// 2, 3 and 4 all modulate 1; one carrier.
    FanIn,
    /// Every operator is a carrier (additive).
    Parallel,
}

impl FmAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            FmAlgorithm::Stack => "stack",
            FmAlgorithm::Pairs => "pairs",
            FmAlgorithm::FanIn => "fan_in",
            FmAlgorithm::Parallel => "parallel",
        }
    }

    /// Operators (0-based) that modulate operator `op`.
    pub fn modulators(&self, op: usize) -> &'static [usize] {
        match (self, op) {
            (FmAlgorithm::Stack, 0) => &[1],
            (FmAlgorithm::Stack, 1) => &[2],
            (FmAlgorithm::Stack, 2) => &[3],
            (FmAlgorithm::Pairs, 0) => &[2],
            (FmAlgorithm::Pairs, 1) => &[3],
            (FmAlgorithm::FanIn, 0) => &[1, 2, 3],
            _ => &[],
        }
    }

    /// Operators (0-based) whose output is heard.
    pub fn carriers(&self) -> &'static [usize] {
        match self {
            FmAlgorithm::Stack | FmAlgorithm::FanIn => &[0],
            FmAlgorithm::Pairs => &[0, 1],
            FmAlgorithm::Parallel => &[0, 1, 2, 3],
        }
    }
}

/// One FM operator: a sine at `ratio` times the note frequency with its own envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Operator {
    pub ratio: f32,
    /// Output gain for a carrier; modulation depth for a modulator.
    pub level: f32,
    pub detune_cents: f32,
    pub env: Adsr,
}

impl Default for Operator {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            level: 1.0,
            detune_cents: 0.0,
            env: Adsr::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Fm {
    pub level: f32,
    pub algorithm: FmAlgorithm,
    /// Self-modulation of the last operator, 0 to 1.
    pub feedback: f32,
    /// 1 to 4 operators; the first is always a carrier.
    pub operators: Vec<Operator>,
}

impl Default for Fm {
    fn default() -> Self {
        Self {
            level: 1.0,
            algorithm: FmAlgorithm::Stack,
            feedback: 0.0,
            operators: vec![Operator::default()],
        }
    }
}

impl Fm {
    /// Longest release among the carriers that exist; the sound ends when they do.
    pub fn carrier_release(&self) -> f32 {
        self.algorithm
            .carriers()
            .iter()
            .filter_map(|&c| self.operators.get(c))
            .map(|op| op.env.release)
            .fold(0.0, f32::max)
    }

    fn validate(&self, path: &str) -> Result<(), String> {
        check_range(&format!("{path}.level"), self.level, 0.0, 1.0)?;
        check_range(&format!("{path}.feedback"), self.feedback, 0.0, 1.0)?;
        if self.operators.is_empty() || self.operators.len() > 4 {
            return Err(format!(
                "{path}.operators must have 1 to 4 entries, got {}",
                self.operators.len()
            ));
        }
        for (i, op) in self.operators.iter().enumerate() {
            let p = format!("{path}.operators[{i}]");
            check_range(&format!("{p}.ratio"), op.ratio, 0.25, 16.0)?;
            check_range(&format!("{p}.level"), op.level, 0.0, 1.0)?;
            check_range(&format!("{p}.detune_cents"), op.detune_cents, -100.0, 100.0)?;
            op.env.validate(&format!("{p}.env"))?;
        }
        Ok(())
    }
}
```

Add to `Patch` after `subtractive`:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fm: Option<Fm>,
```

Update the helpers:

```rust
    pub fn release_seconds(&self) -> f32 {
        let sub = self
            .subtractive
            .as_ref()
            .filter(|s| s.level > 0.0)
            .map(|s| s.env.release)
            .unwrap_or(0.0);
        let fm = self
            .fm
            .as_ref()
            .filter(|f| f.level > 0.0)
            .map(|f| f.carrier_release())
            .unwrap_or(0.0);
        sub.max(fm)
    }

    pub fn has_pitched_engine(&self) -> bool {
        self.subtractive.as_ref().is_some_and(|s| s.level > 0.0)
            || self.fm.as_ref().is_some_and(|f| f.level > 0.0)
    }
```

In `validate`, replace the "no engines" check with:

```rust
        if self.subtractive.is_none() && self.percussion.is_none() && self.fm.is_none() {
            return Err(format!(
                "patch '{}' has no engines: add \"subtractive\", \"fm\" or \"percussion\"",
                self.name
            ));
        }
```

and, after the subtractive block:

```rust
        if let Some(fm) = &self.fm {
            fm.validate("fm")?;
        }
```

(Task 4 adds `wavetable` to these three places.)

- [ ] **Step 4: Run the tests**

```bash
cargo test patch:: 2>&1 | tail -20 && cargo test 2>&1 | grep -E "^test result"
```

Expected: 4 new tests pass; the full suite still passes (117 + 4 unit, 31 integration).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/patch.rs
git commit -m "feat(patch): fm engine data model with algorithms and validation"
```

---

### Task 2: FM voice

**Files:**
- Create: `src/expressive/engines/fm.rs`
- Modify: `src/expressive/engines/mod.rs`

**Interfaces:**
- Consumes: `Fm`, `Operator`, `FmAlgorithm` (Task 1); `GateEnvelope::new(&Adsr, sr)`, `gate_off`, `is_active`, `next`; `PhaseAccumulator::new(sr)`, `next_phase(freq) -> radians`; `Voice`, `Modulation`.
- Produces: `pub struct FmVoice; impl FmVoice { pub fn new(cfg: &Fm, frequency: f32, sample_rate: f32) -> Self }` implementing `Voice`; `pub const FM_MOD_DEPTH: f32 = 4.0;` (peak phase deviation in radians at modulator level 1).

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/engines/fm.rs`:

```rust
//! Four-operator FM. Each operator is a sine with its own envelope; the
//! algorithm decides which operators modulate which, and which are heard.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::{Fm, GateEnvelope};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};
    use crate::expressive::{Adsr, FmAlgorithm, Operator};

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn op(ratio: f32, level: f32) -> Operator {
        Operator {
            ratio,
            level,
            detune_cents: 0.0,
            env: fast(),
        }
    }

    fn render(cfg: &Fm, freq: f32, gate: f32, total: f32) -> Vec<f32> {
        let mut v = FmVoice::new(cfg, freq, SR);
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

    fn fm(algorithm: FmAlgorithm, operators: Vec<Operator>) -> Fm {
        Fm {
            level: 1.0,
            algorithm,
            feedback: 0.0,
            operators,
        }
    }

    #[test]
    fn a_single_carrier_is_a_sine_at_the_note_frequency() {
        let s = render(&fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]), 220.0, 1.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 440.0).abs() < 8.0);
        let h2 = goertzel_power(&s, 440.0, SR);
        let f = goertzel_power(&s, 220.0, SR);
        assert!(db(h2 / f) < -40.0, "no harmonics without a modulator");
    }

    #[test]
    fn a_modulator_creates_sidebands_at_carrier_plus_ratio_multiples() {
        // Carrier 220 Hz, modulator at ratio 2 (440 Hz): sidebands at 660, 1100, ...
        let plain = render(&fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]), 220.0, 1.0, 1.0);
        let modulated = render(
            &fm(FmAlgorithm::Stack, vec![op(1.0, 1.0), op(2.0, 0.5)]),
            220.0,
            1.0,
            1.0,
        );
        let side = goertzel_power(&modulated, 660.0, SR);
        assert!(
            db(side / goertzel_power(&plain, 660.0, SR)) > 30.0,
            "660 Hz sideband appears"
        );
    }

    #[test]
    fn parallel_operators_are_pure_partials_without_sidebands() {
        let s = render(
            &fm(FmAlgorithm::Parallel, vec![op(1.0, 1.0), op(2.0, 1.0)]),
            220.0,
            1.0,
            1.0,
        );
        let f = goertzel_power(&s, 220.0, SR);
        let h2 = goertzel_power(&s, 440.0, SR);
        let h3 = goertzel_power(&s, 660.0, SR);
        assert!(db(h2 / f).abs() < 3.0, "both partials present at similar level");
        assert!(db(h3 / f) < -30.0, "no sideband at 660 Hz");
    }

    #[test]
    fn pairs_has_two_carriers_and_fan_in_has_one() {
        // pairs: op3 (idx2) modulates op1; op2 (idx1) is a second carrier at ratio 1.5.
        let pairs = render(
            &fm(
                FmAlgorithm::Pairs,
                vec![op(1.0, 1.0), op(1.5, 1.0), op(2.0, 0.4)],
            ),
            200.0,
            1.0,
            1.0,
        );
        assert!(
            db(goertzel_power(&pairs, 300.0, SR) / goertzel_power(&pairs, 200.0, SR)) > -6.0,
            "second carrier at 300 Hz is heard"
        );
        // fan_in: op2 at ratio 1.5 is a modulator, not a carrier; 300 Hz appears only as a weak sideband.
        let fan = render(
            &fm(
                FmAlgorithm::FanIn,
                vec![op(1.0, 1.0), op(1.5, 0.1), op(2.0, 0.1)],
            ),
            200.0,
            1.0,
            1.0,
        );
        assert!(
            db(goertzel_power(&fan, 300.0, SR) / goertzel_power(&fan, 200.0, SR)) < -10.0,
            "ratio-1.5 operator is not heard directly in fan_in"
        );
    }

    #[test]
    fn modulator_envelope_shapes_the_timbre_over_time() {
        let mut modulator = op(3.0, 0.8);
        modulator.env = Adsr {
            attack: 0.001,
            decay: 0.3,
            sustain: 0.0,
            release: 0.1,
        };
        let s = render(
            &fm(FmAlgorithm::Stack, vec![op(1.0, 1.0), modulator]),
            220.0,
            1.5,
            1.5,
        );
        let early = goertzel_power(&s[..4410], 880.0, SR);
        let late = goertzel_power(&s[44100..48510], 880.0, SR);
        assert!(db(early / late) > 15.0, "sideband fades as the modulator decays");
    }

    #[test]
    fn feedback_adds_harmonics_to_the_last_operator() {
        let mut cfg = fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]);
        let clean = render(&cfg, 220.0, 1.0, 1.0);
        cfg.feedback = 0.8;
        let fed = render(&cfg, 220.0, 1.0, 1.0);
        assert!(
            db(goertzel_power(&fed, 440.0, SR) / goertzel_power(&clean, 440.0, SR)) > 20.0,
            "feedback creates a second harmonic"
        );
    }

    #[test]
    fn release_sounds_past_the_gate_and_then_stops() {
        let mut carrier = op(1.0, 1.0);
        carrier.env = Adsr {
            release: 0.5,
            ..fast()
        };
        let mut v = FmVoice::new(&fm(FmAlgorithm::Stack, vec![carrier]), 220.0, SR);
        let mods = Modulation::default();
        for _ in 0..4410 {
            v.tick(&mods);
        }
        v.gate_off();
        let after: Vec<f32> = (0..4410).map(|_| v.tick(&mods).0).collect();
        assert!(rms(&after) > 0.1);
        assert!(v.is_active());
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
    }

    #[test]
    fn level_pitch_ratio_and_amplitude_modulation_apply() {
        let mut cfg = fm(FmAlgorithm::Stack, vec![op(1.0, 1.0)]);
        cfg.level = 0.5;
        let mut v = FmVoice::new(&cfg, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
        };
        let s: Vec<f32> = (0..SR as usize).map(|_| v.tick(&mods).0).collect();
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 15.0);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - 0.25).abs() < 0.03, "level 0.5 x amplitude 0.5: {peak}");
    }

    #[test]
    fn missing_operators_are_silent_not_a_panic() {
        // Stack with 4 carriers' worth of routing but only 1 operator supplied.
        let s = render(&fm(FmAlgorithm::FanIn, vec![op(1.0, 1.0)]), 220.0, 0.5, 0.5);
        assert!(s.iter().all(|x| x.is_finite()));
        assert!(rms(&s) > 0.3);
    }
}
```

- [ ] **Step 2: Run to verify failure**

In `src/expressive/engines/mod.rs` add `pub mod fm;` and `#[allow(unused_imports)] pub use fm::{FM_MOD_DEPTH, FmVoice};`. Run `cargo test engines::fm 2>&1 | tail`. Expected: `FmVoice` not found.

- [ ] **Step 3: Implement**

Above the tests in `fm.rs`:

```rust
/// Peak phase deviation in radians when a modulator's level and envelope are both 1.
pub const FM_MOD_DEPTH: f32 = 4.0;

const MAX_OPERATORS: usize = 4;

pub struct FmVoice {
    cfg: Fm,
    frequency: f32,
    phases: [PhaseAccumulator; MAX_OPERATORS],
    envs: Vec<GateEnvelope>,
    /// Previous-sample output of each operator, for feedback.
    last_out: [f32; MAX_OPERATORS],
    /// 1/sqrt(number of carriers that exist), so layering carriers does not clip.
    carrier_norm: f32,
}

impl FmVoice {
    pub fn new(cfg: &Fm, frequency: f32, sample_rate: f32) -> Self {
        let envs = cfg
            .operators
            .iter()
            .map(|op| GateEnvelope::new(&op.env, sample_rate))
            .collect();
        let carriers = cfg
            .algorithm
            .carriers()
            .iter()
            .filter(|&&c| c < cfg.operators.len())
            .count()
            .max(1);
        Self {
            cfg: cfg.clone(),
            frequency,
            phases: [PhaseAccumulator::new(sample_rate); MAX_OPERATORS],
            envs,
            last_out: [0.0; MAX_OPERATORS],
            carrier_norm: 1.0 / (carriers as f32).sqrt(),
        }
    }
}

impl Voice for FmVoice {
    fn gate_off(&mut self) {
        for env in &mut self.envs {
            env.gate_off();
        }
    }

    fn is_active(&self) -> bool {
        self.cfg
            .algorithm
            .carriers()
            .iter()
            .filter_map(|&c| self.envs.get(c))
            .any(|e| e.is_active())
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let base = self.frequency * mods.pitch_ratio;
        let count = self.cfg.operators.len();
        let last = count - 1;
        let mut out = [0.0f32; MAX_OPERATORS];

        // Modulators always have higher indices than what they modulate, so a
        // reverse pass has every modulator ready when its carrier needs it.
        for i in (0..count).rev() {
            let op = &self.cfg.operators[i];
            let freq = base * op.ratio * 2f32.powf(op.detune_cents / 1200.0);
            let mut phase = self.phases[i].next_phase(freq);
            for &m in self.cfg.algorithm.modulators(i) {
                if m < count {
                    phase += out[m] * FM_MOD_DEPTH;
                }
            }
            if i == last && self.cfg.feedback > 0.0 {
                phase += self.last_out[i] * self.cfg.feedback;
            }
            let env = self.envs[i].next();
            out[i] = phase.sin() * op.level * env;
        }
        self.last_out = out;

        let sum: f32 = self
            .cfg
            .algorithm
            .carriers()
            .iter()
            .filter(|&&c| c < count)
            .map(|&c| out[c])
            .sum();
        let s = sum * self.carrier_norm * self.cfg.level * mods.amplitude;
        (s, s)
    }
}
```

`PhaseAccumulator` must be `Copy` for the array initialiser; it already derives `Clone, Copy` in `oscillator.rs` (check; if not, build the array with `std::array::from_fn(|_| PhaseAccumulator::new(sample_rate))`).

- [ ] **Step 4: Run the tests**

```bash
cargo test engines::fm 2>&1 | tail -25
```

Expected: 9 passed. If `pairs_has_two_carriers_and_fan_in_has_one`'s fan_in half is marginal, lower the modulator levels in that test (they only need to be small); do not change the algorithm tables.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/engines
git commit -m "feat(engines): four-operator FM voice"
```

---

### Task 3: FM in the renderer and the FM built-in patches

**Files:**
- Modify: `src/expressive/render.rs`, `src/expressive/patch.rs` (`BUILTIN_PATCHES`), `src/demos.rs`
- Create: `src/expressive/patches/dx7_e_piano.json`, `dx7_slap_bass.json`, `tx81z_lately.json`, `fm_bell.json`

**Interfaces:**
- Consumes: `FmVoice::new(&Fm, frequency, sample_rate)` (Task 2), `Patch.fm` (Task 1).
- Produces: `render_patch` spawns an FM voice per note when `patch.fm` has `level > 0`; four new built-in names.

- [ ] **Step 1: Write the failing tests**

In `src/expressive/render.rs` tests:

```rust
    #[test]
    fn fm_engine_renders_and_layers_with_subtractive() {
        let fm_only = patch(json!({"name": "f", "fm": {"operators": [
            {"ratio": 1.0, "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            {"ratio": 2.0, "level": 0.5, "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}]}}));
        let a = left(&render_patch(&fm_only, &[note(0.0, 0.5, 220.0)], SR));
        assert!(rms(&a[441..22050]) > 0.3, "fm voice sounds");
        assert!(a.iter().all(|x| x.is_finite()));

        let both = patch(json!({"name": "b", "level": 0.5,
            "subtractive": {"level": 0.5, "env": {"release": 0.01}},
            "fm": {"level": 0.5, "operators": [{"ratio": 1.0, "env": {"release": 0.01}}]}}));
        let sub_only = patch(json!({"name": "s", "level": 0.5,
            "subtractive": {"level": 0.5, "env": {"release": 0.01}}}));
        let b = left(&render_patch(&both, &[note(0.0, 0.5, 220.0)], SR));
        let s = left(&render_patch(&sub_only, &[note(0.0, 0.5, 220.0)], SR));
        assert!(rms(&b[4410..22050]) > rms(&s[4410..22050]) * 1.2, "fm adds energy");
    }

    #[test]
    fn fm_release_extends_the_buffer() {
        let p = patch(json!({"name": "f", "fm": {"operators": [{"env": {"release": 0.7}}]}}));
        assert!((render_length_seconds(&p, &[note(0.0, 0.5, 220.0)]) - 1.2).abs() < 1e-4);
    }
```

In `src/expressive/patch.rs`, extend `library_lookup_is_case_insensitive_and_catalog_is_grouped` with:

```rust
        assert!(lib.get("dx7_e_piano").is_some());
        assert!(cats.contains(&PatchCategory::Keys), "keys category is back");
        assert!(lib.count() >= 35);
```

(`cats` is the category vector that test already builds.)

- [ ] **Step 2: Run to verify failure**

`cargo test render:: patch:: 2>&1 | tail`. Expected: the FM render test produces silence/short buffer (fails), the library test fails on the missing patch.

- [ ] **Step 3: Implement the renderer hook**

In `render.rs`, import `FmVoice` (`use crate::expressive::engines::{FmVoice, Modulation, PercussionVoice, SubtractiveVoice, Voice};`) and add after the subtractive spawn inside the note loop:

```rust
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
```

`render_length_seconds` already uses `patch.release_seconds()`, which Task 1 extended.

- [ ] **Step 4: Write the patch files**

`src/expressive/patches/dx7_e_piano.json` (Rust source: `git show 844be45:src/expressive/presets/categories/keys.rs`; DX7 algorithm 5 = three 2-op stacks, mapped to `pairs`: index 0 carrier, index 1 second carrier, index 2 modulates index 0). The old DX7 code used the same `4.0` modulation depth, so operator levels carry over unchanged. The Rust preset's low-pass 2800 Hz filter becomes an effects-chain filter, and `create_reverb(0.15)` plus `create_signature_effects_for_keys` (read its numbers in `git show 844be45:src/expressive/presets/library.rs`) follow:

```json
{
  "name": "dx7_e_piano",
  "description": "Authentic DX7 E.Piano 1 - the most famous electric piano of the 80s (Yamaha DX7 ROM 1A 11, algorithm 5)",
  "category": "keys",
  "level": 0.8,
  "fm": {
    "algorithm": "pairs",
    "operators": [
      {"ratio": 1.0, "level": 0.7, "env": {"attack": 0.005, "decay": 0.3, "sustain": 0.6, "release": 3.0}},
      {"ratio": 1.0, "level": 0.4, "detune_cents": 7, "env": {"attack": 0.01, "decay": 0.4, "sustain": 0.5, "release": 2.5}},
      {"ratio": 14.0, "level": 0.3, "env": {"attack": 0.001, "decay": 0.1, "sustain": 0.2, "release": 1.0}}
    ]
  },
  "effects": [
    {"type": "filter", "filter_type": "low_pass", "cutoff": 2800, "resonance": 0.02, "intensity": 1.0},
    {"type": "reverb", "room_size": 0.5, "dampening": 0.3, "wet_level": 0.6, "intensity": 0.15}
  ]
}
```
(append the keys signature effects after the reverb, in flat form).

`dx7_slap_bass.json` (source: `bass.rs` "DX7 Slap Bass", algorithm 16 with two operators = `stack`): operators `[{"ratio": 1.0, "level": 0.9, "env": {"attack": 0.001, "decay": 0.06, "sustain": 0.6, "release": 3.0}}, {"ratio": 2.0, "level": 0.3, "env": {"attack": 0.001, "decay": 0.04, "sustain": 0.2, "release": 2.5}}]`, `level` 0.8, category `bass`, effects: filter low_pass 1800 / resonance 0.05, `create_reverb(0.05)`, then `create_signature_effects_for_modern_clarity`.

`tx81z_lately.json` (source: "TX81Z Lately", two-operator FM with modulator 220 Hz at carrier 110 Hz = ratio 2, modulation index 2.5): with `FM_MOD_DEPTH` 4.0, level = 2.5 / 4.0 = 0.625. Operators `[{"ratio": 1.0, "level": 1.0, "env": {"attack": 0.005, "decay": 0.2, "sustain": 0.6, "release": 0.4}}, {"ratio": 2.0, "level": 0.625, "env": {"attack": 0.005, "decay": 0.2, "sustain": 0.6, "release": 0.4}}]`, `level` 0.85, category `bass`, effects: filter low_pass 1000 / resonance 0.3, `create_reverb(0.12)`, then modern-clarity signature effects.

`fm_bell.json` (new):

```json
{
  "name": "fm_bell",
  "description": "Glassy FM bell: a 3.5-ratio modulator that decays quickly over a long carrier tail",
  "category": "keys",
  "level": 0.7,
  "fm": {
    "algorithm": "stack",
    "feedback": 0.1,
    "operators": [
      {"ratio": 1.0, "level": 1.0, "env": {"attack": 0.002, "decay": 1.5, "sustain": 0.2, "release": 2.5}},
      {"ratio": 3.5, "level": 0.55, "env": {"attack": 0.001, "decay": 0.6, "sustain": 0.0, "release": 0.5}}
    ]
  },
  "effects": [{"type": "reverb", "room_size": 0.7, "dampening": 0.4, "wet_level": 0.5, "intensity": 0.35}]
}
```

Add the four `include_str!` lines to `BUILTIN_PATCHES` under a `// fm` comment. In `src/demos.rs`, give the keys category a mid-register phrase: change the match to

```rust
                "drums" | "fx" => &[(36, 0.0, 0.5), (36, 0.5, 0.5)],
                "pad" => &[(48, 0.0, 3.0), (55, 0.0, 3.0), (60, 0.0, 3.0)],
                "keys" => &[(60, 0.0, 0.6), (64, 0.7, 0.6), (67, 1.4, 1.2)],
                _ => &[(36, 0.0, 0.4), (43, 0.5, 0.4), (48, 1.0, 0.8)],
```

- [ ] **Step 5: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: everything passes, including `every_builtin_patch_parses_validates_and_renders_cleanly` for the four new files (adjust a patch's `level` if it peaks above 1.6, and say so in the commit body).

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/render.rs src/expressive/patch.rs src/expressive/patches src/demos.rs
git commit -m "feat: fm engine in the renderer with DX7 E.Piano, DX7 Slap Bass, TX81Z Lately and fm_bell"
```

---

### Task 4: Wavetable data model and band-limited tables

**Files:**
- Modify: `src/expressive/patch.rs`
- Create: `src/expressive/wavetables.rs`
- Modify: `src/expressive/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  // patch.rs
  #[serde(rename_all = "snake_case")] pub enum TableName { Basic, Warm, Bright, Digital, Vocal, Pwm, Organ, Noise } // Default = Basic
  impl TableName { pub const ALL: [TableName; 8]; pub fn as_str(&self) -> &'static str; pub fn index(&self) -> usize; pub fn next(&self) -> TableName; }
  pub struct Wavetable { pub level: f32, pub table: TableName, pub morph: f32, pub env: Adsr } // Default: 1.0, Basic, 0.0, Adsr::default()
  pub struct Patch { ..., pub wavetable: Option<Wavetable>, ... }
  // wavetables.rs
  pub const TABLE_SIZE: usize = 2048;
  pub const MIP_LEVELS: usize = 10;
  pub const LOWEST_FUNDAMENTAL_HZ: f32 = 27.5;
  pub struct Partial { pub ratio: f32, pub amplitude: f32, pub phase: f32 }
  pub fn partials(table: TableName) -> Vec<Partial>;
  pub fn mip_level(frequency: f32) -> usize;               // clamp(floor(log2(f / 27.5)), 0, 9)
  pub fn max_ratio(level: usize) -> f32;                   // 20000 / (27.5 * 2^(level+1)), highest partial ratio allowed at that level
  pub fn sample(table: TableName, level: usize, phase: f32) -> f32;  // linear interpolation, phase 0..1
  pub fn naive_sample(table: TableName, phase: f32) -> f32; // all partials, for tests
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/wavetables.rs`:

```rust
//! Procedural wavetables, generated once and band-limited per octave so a
//! high note never includes partials above Nyquist.

use crate::expressive::TableName;
use std::f32::consts::TAU;
use std::sync::OnceLock;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms};

    const SR: f32 = 44100.0;

    fn render(table: TableName, level: usize, freq: f32, seconds: f32) -> Vec<f32> {
        let n = (seconds * SR) as usize;
        (0..n)
            .map(|i| sample(table, level, (freq * i as f32 / SR).rem_euclid(1.0)))
            .collect()
    }

    fn render_naive(table: TableName, freq: f32, seconds: f32) -> Vec<f32> {
        let n = (seconds * SR) as usize;
        (0..n)
            .map(|i| naive_sample(table, (freq * i as f32 / SR).rem_euclid(1.0)))
            .collect()
    }

    #[test]
    fn mip_level_follows_octaves_from_a0() {
        assert_eq!(mip_level(20.0), 0);
        assert_eq!(mip_level(27.5), 0);
        assert_eq!(mip_level(54.9), 0);
        assert_eq!(mip_level(55.0), 1);
        assert_eq!(mip_level(440.0), 4);
        assert_eq!(mip_level(20000.0), 9);
        assert!(max_ratio(0) > max_ratio(9));
        assert!(max_ratio(9) >= 1.0, "the fundamental always survives");
    }

    #[test]
    fn every_table_is_finite_normalised_and_has_a_fundamental() {
        for table in TableName::ALL {
            let s = render(table, 4, 440.0, 0.5);
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(s.iter().all(|x| x.is_finite()), "{table:?}");
            assert!(peak > 0.5 && peak <= 1.0, "{table:?} peak {peak}");
            assert!(
                goertzel_power(&s, 440.0, SR) > goertzel_power(&s, 1000.0, SR),
                "{table:?} has its fundamental"
            );
        }
    }

    #[test]
    fn organ_and_warm_have_their_signature_harmonics() {
        let organ = render(TableName::Organ, 3, 220.0, 0.5);
        assert!(db(goertzel_power(&organ, 440.0, SR) / goertzel_power(&organ, 220.0, SR)) > -6.0);
        let warm = render(TableName::Warm, 3, 220.0, 0.5);
        let even = goertzel_power(&warm, 440.0, SR);
        let odd = goertzel_power(&warm, 660.0, SR);
        assert!(db(odd / even) > 20.0, "warm is odd harmonics only");
    }

    #[test]
    fn high_notes_alias_less_than_the_naive_table() {
        // 5000 Hz fundamental: harmonics above the 4th fold back below Nyquist.
        let level = mip_level(5000.0);
        let limited = render(TableName::Bright, level, 5000.0, 0.5);
        let naive = render_naive(TableName::Bright, 5000.0, 0.5);
        // 6th harmonic 30000 Hz folds to 14100 Hz.
        let alias = 14100.0;
        assert!(
            db(goertzel_power(&limited, alias, SR) / goertzel_power(&naive, alias, SR)) < -20.0,
            "band-limited table removes the folded partial"
        );
        assert!(rms(&limited) > 0.1, "still audible");
    }

    #[test]
    fn noise_table_is_broadband_but_periodic() {
        let s = render(TableName::Noise, 2, 110.0, 0.5);
        let lo = goertzel_power(&s, 110.0 * 3.0, SR);
        let hi = goertzel_power(&s, 110.0 * 20.0, SR);
        assert!(db(lo / hi).abs() < 25.0, "energy spread over many harmonics");
    }
}
```

Add to the `patch.rs` tests:

```rust
    #[test]
    fn wavetable_patch_parses_validates_and_names_fields() {
        let p = parse(json!({"name": "w", "wavetable": {}})).unwrap();
        let wt = p.wavetable.as_ref().unwrap();
        assert_eq!(wt.table, TableName::Basic);
        assert_eq!(wt.morph, 0.0);
        assert!(p.validate().is_ok() && p.has_pitched_engine());
        let p = parse(json!({"name": "w", "wavetable": {"table": "organ", "morph": 0.4, "env": {"release": 1.0}}})).unwrap();
        assert_eq!(p.release_seconds(), 1.0);
        assert_eq!(serde_json::to_value(&p).unwrap()["wavetable"]["table"], "organ");
        let p = parse(json!({"name": "w", "wavetable": {"morph": 1.5}})).unwrap();
        assert!(p.validate().unwrap_err().contains("wavetable.morph"));
        assert!(parse(json!({"name": "w", "wavetable": {"table": "sawtooth"}})).is_err());
        assert!(parse(json!({"name": "w", "wavetable": {"position": 0.2}})).unwrap_err().contains("position"));
        assert_eq!(TableName::Noise.next(), TableName::Basic, "morph wraps");
        assert_eq!(TableName::Pwm.index(), 5);
    }
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod wavetables;` to `src/expressive/mod.rs` (no glob re-export; it is addressed as `crate::expressive::wavetables`). `cargo test wavetables patch:: 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement the model**

In `patch.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableName {
    #[default]
    Basic,
    Warm,
    Bright,
    Digital,
    Vocal,
    Pwm,
    Organ,
    Noise,
}

impl TableName {
    pub const ALL: [TableName; 8] = [
        TableName::Basic,
        TableName::Warm,
        TableName::Bright,
        TableName::Digital,
        TableName::Vocal,
        TableName::Pwm,
        TableName::Organ,
        TableName::Noise,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            TableName::Basic => "basic",
            TableName::Warm => "warm",
            TableName::Bright => "bright",
            TableName::Digital => "digital",
            TableName::Vocal => "vocal",
            TableName::Pwm => "pwm",
            TableName::Organ => "organ",
            TableName::Noise => "noise",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|t| t == self).expect("every table is in ALL")
    }

    /// The table `morph` blends toward; wraps from `noise` back to `basic`.
    pub fn next(&self) -> TableName {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Wavetable {
    pub level: f32,
    pub table: TableName,
    /// 0 = this table, 1 = the next table in the list.
    pub morph: f32,
    pub env: Adsr,
}

impl Default for Wavetable {
    fn default() -> Self {
        Self {
            level: 1.0,
            table: TableName::Basic,
            morph: 0.0,
            env: Adsr::default(),
        }
    }
}

impl Wavetable {
    fn validate(&self, path: &str) -> Result<(), String> {
        check_range(&format!("{path}.level"), self.level, 0.0, 1.0)?;
        check_range(&format!("{path}.morph"), self.morph, 0.0, 1.0)?;
        self.env.validate(&format!("{path}.env"))
    }
}
```

Add `pub wavetable: Option<Wavetable>` to `Patch` (serde attrs as for `fm`), include it in `release_seconds` (`.max(wavetable release when level > 0)`), `has_pitched_engine`, the "no engines" check (message: `add "subtractive", "fm", "wavetable" or "percussion"`), and `validate` (`wt.validate("wavetable")?`).

- [ ] **Step 4: Implement the tables**

Above the tests in `wavetables.rs`:

```rust
pub const TABLE_SIZE: usize = 2048;
/// One level per octave from A0 (27.5 Hz) up; level 9 covers 14 kHz and above.
pub const MIP_LEVELS: usize = 10;
pub const LOWEST_FUNDAMENTAL_HZ: f32 = 27.5;
/// Partials above this frequency are left out of every level.
const HIGHEST_PARTIAL_HZ: f32 = 20000.0;

/// One sinusoidal component of a table: `ratio` times the fundamental.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Partial {
    pub ratio: f32,
    pub amplitude: f32,
    /// Phase offset in cycles (0..1).
    pub phase: f32,
}

fn harmonic(n: u32, amplitude: f32) -> Partial {
    Partial {
        ratio: n as f32,
        amplitude,
        phase: 0.0,
    }
}

/// Fourier series of a pulse wave with duty `d` (DC removed), harmonics 1..=max.
fn pulse_partials(duty: f32, max: u32, gain: f32) -> impl Iterator<Item = Partial> {
    (1..=max).map(move |n| {
        let nf = n as f32;
        let amplitude = gain * 2.0 / (nf * std::f32::consts::PI) * (nf * std::f32::consts::PI * duty).sin();
        Partial {
            ratio: nf,
            amplitude,
            phase: -(nf * duty) / 2.0, // centres the pulse so the three duty cycles add coherently
        }
    })
}

/// The additive recipe for each table (mirrors the tryx-fx generators).
pub fn partials(table: TableName) -> Vec<Partial> {
    match table {
        TableName::Basic => vec![harmonic(1, 0.8), harmonic(2, 0.15), harmonic(3, 0.05)],
        TableName::Warm => (1..=15u32)
            .step_by(2)
            .map(|h| harmonic(h, 0.7 / h as f32))
            .collect(),
        TableName::Bright => (1..=20u32).map(|h| harmonic(h, 0.5 / h as f32)).collect(),
        TableName::Digital => vec![
            harmonic(1, 0.4),
            Partial { ratio: 2.5, amplitude: 0.3, phase: 0.0 },
            Partial { ratio: 4.1, amplitude: 0.2, phase: 0.0 },
            Partial { ratio: 7.3, amplitude: 0.1, phase: 0.0 },
        ],
        TableName::Vocal => vec![
            harmonic(1, 0.3),
            harmonic(3, 0.5),
            harmonic(5, 0.4),
            harmonic(7, 0.2),
            harmonic(9, 0.1),
        ],
        TableName::Pwm => {
            let mut v: Vec<Partial> = Vec::new();
            for duty in [0.3, 0.5, 0.7] {
                v.extend(pulse_partials(duty, 48, 0.25));
            }
            v
        }
        TableName::Organ => vec![
            harmonic(1, 0.8),
            harmonic(2, 0.6),
            harmonic(3, 0.5),
            harmonic(4, 0.3),
            harmonic(5, 0.2),
            harmonic(6, 0.15),
            harmonic(8, 0.1),
        ],
        TableName::Noise => (1..=32u32)
            .map(|h| {
                let golden = h as f32 * h as f32 * 1.618_034;
                Partial {
                    ratio: h as f32,
                    amplitude: 0.3 / h as f32,
                    phase: golden - golden.floor(),
                }
            })
            .collect(),
    }
}

/// Mip level for a fundamental: one per octave above A0, clamped to the table.
pub fn mip_level(frequency: f32) -> usize {
    let octaves = (frequency.max(1.0) / LOWEST_FUNDAMENTAL_HZ).log2().floor();
    (octaves.max(0.0) as usize).min(MIP_LEVELS - 1)
}

/// Highest partial ratio that stays below `HIGHEST_PARTIAL_HZ` for every
/// fundamental in this level's octave (the octave's top, not its bottom).
pub fn max_ratio(level: usize) -> f32 {
    let top_of_octave = LOWEST_FUNDAMENTAL_HZ * 2f32.powi(level as i32 + 1);
    (HIGHEST_PARTIAL_HZ / top_of_octave).max(1.0)
}

fn synthesize(partials: &[Partial], max_ratio: f32) -> Vec<f32> {
    let mut table = vec![0.0f32; TABLE_SIZE];
    for p in partials.iter().filter(|p| p.ratio <= max_ratio) {
        for (i, s) in table.iter_mut().enumerate() {
            let phase = i as f32 / TABLE_SIZE as f32;
            *s += p.amplitude * (TAU * (p.ratio * phase + p.phase)).sin();
        }
    }
    // Normalise each level to a peak of 1 so morphing and layering are predictable.
    let peak = table.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        for s in &mut table {
            *s /= peak;
        }
    }
    table
}

struct Tables {
    /// [table][level][sample]
    data: Vec<Vec<Vec<f32>>>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| Tables {
        data: TableName::ALL
            .iter()
            .map(|&t| {
                let parts = partials(t);
                (0..MIP_LEVELS)
                    .map(|level| synthesize(&parts, max_ratio(level)))
                    .collect()
            })
            .collect(),
    })
}

#[inline]
fn interpolate(table: &[f32], phase: f32) -> f32 {
    let pos = phase.rem_euclid(1.0) * TABLE_SIZE as f32;
    let i0 = pos as usize % TABLE_SIZE;
    let i1 = (i0 + 1) % TABLE_SIZE;
    let frac = pos - pos.floor();
    table[i0] * (1.0 - frac) + table[i1] * frac
}

/// One sample of `table` at `level`, unit `phase` 0..1.
#[inline]
pub fn sample(table: TableName, level: usize, phase: f32) -> f32 {
    interpolate(&tables().data[table.index()][level.min(MIP_LEVELS - 1)], phase)
}

/// Every partial regardless of Nyquist; used by tests as the aliasing baseline.
pub fn naive_sample(table: TableName, phase: f32) -> f32 {
    partials(table)
        .iter()
        .map(|p| p.amplitude * (TAU * (p.ratio * phase + p.phase)).sin())
        .sum::<f32>()
}
```

Note the `Digital` table's non-integer ratios are not periodic in one cycle; that is inherited from tryx-fx and is what gives the "digital" character. The band limit still applies by ratio.

- [ ] **Step 5: Run the tests**

```bash
cargo test wavetables patch:: 2>&1 | tail -25
```

Expected: all pass. The first call to `sample` builds 8 × 10 tables (about 5 million sine evaluations, well under a second in debug). If `every_table_is_finite_normalised_and_has_a_fundamental` fails on `peak <= 1.0` due to interpolation overshoot, that cannot happen with linear interpolation; if it fails on `peak > 0.5`, the level's partial filter removed too much — check `max_ratio`.

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/patch.rs src/expressive/wavetables.rs src/expressive/mod.rs
git commit -m "feat: wavetable data model and band-limited procedural tables"
```

---

### Task 5: Wavetable voice, renderer hook, wavetable built-in patches

**Files:**
- Create: `src/expressive/engines/wavetable.rs`
- Modify: `src/expressive/engines/mod.rs`, `src/expressive/render.rs`, `src/expressive/patch.rs` (`BUILTIN_PATCHES`)
- Create: `src/expressive/patches/wt_organ.json`, `wt_vocal_pad.json`, `wt_pwm_lead.json`, `wt_glass_keys.json`

**Interfaces:**
- Consumes: `Wavetable`, `TableName` (Task 4), `wavetables::{mip_level, sample}`, `GateEnvelope`, `PhaseAccumulator::next_unit`, `Voice`, `Modulation`.
- Produces: `pub struct WavetableVoice; impl WavetableVoice { pub fn new(cfg: &Wavetable, frequency: f32, sample_rate: f32) -> Self }` implementing `Voice`.

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/engines/wavetable.rs`:

```rust
//! Plays one band-limited wavetable (or a morph between two neighbours)
//! through an amplitude envelope.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::oscillator::PhaseAccumulator;
use crate::expressive::wavetables::{mip_level, sample};
use crate::expressive::{GateEnvelope, TableName, Wavetable};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::{db, goertzel_power, rms, zero_crossing_rate};
    use crate::expressive::Adsr;

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn cfg(table: TableName, morph: f32) -> Wavetable {
        Wavetable {
            level: 1.0,
            table,
            morph,
            env: fast(),
        }
    }

    fn render(c: &Wavetable, freq: f32, seconds: f32) -> Vec<f32> {
        let mut v = WavetableVoice::new(c, freq, SR);
        let mods = Modulation::default();
        (0..(seconds * SR) as usize).map(|_| v.tick(&mods).0).collect()
    }

    #[test]
    fn plays_at_the_note_frequency() {
        let s = render(&cfg(TableName::Basic, 0.0), 330.0, 1.0);
        assert!((zero_crossing_rate(&s, SR) - 660.0).abs() < 12.0);
        assert!(rms(&s[441..]) > 0.3);
    }

    #[test]
    fn morph_moves_the_spectrum_between_neighbouring_tables() {
        // basic (weak 3rd harmonic) -> warm (strong 3rd harmonic).
        let h3 = |m: f32| {
            let s = render(&cfg(TableName::Basic, m), 220.0, 1.0);
            db(goertzel_power(&s, 660.0, SR) / goertzel_power(&s, 220.0, SR))
        };
        let (a, b, c) = (h3(0.0), h3(0.5), h3(1.0));
        assert!(a < b && b < c, "3rd harmonic grows with morph: {a} {b} {c}");
    }

    #[test]
    fn morph_wraps_from_noise_to_basic() {
        let noise_to_basic = render(&cfg(TableName::Noise, 1.0), 220.0, 1.0);
        let basic = render(&cfg(TableName::Basic, 0.0), 220.0, 1.0);
        let h20 = |s: &[f32]| goertzel_power(s, 220.0 * 20.0, SR);
        assert!(db(h20(&noise_to_basic) / h20(&basic)).abs() < 3.0, "fully morphed = next table");
    }

    #[test]
    fn high_notes_use_a_band_limited_level() {
        let s = render(&cfg(TableName::Bright, 0.0), 5000.0, 0.5);
        // 6th harmonic (30 kHz) would fold to 14.1 kHz; the level chosen at 5 kHz excludes it.
        let alias = goertzel_power(&s, 14100.0, SR);
        let fundamental = goertzel_power(&s, 5000.0, SR);
        assert!(db(alias / fundamental) < -30.0);
    }

    #[test]
    fn release_then_silence_and_modulation_apply() {
        let mut c = cfg(TableName::Organ, 0.0);
        c.env.release = 0.4;
        c.level = 0.5;
        let mut v = WavetableVoice::new(&c, 220.0, SR);
        let mods = Modulation {
            pitch_ratio: 2.0,
            cutoff_ratio: 1.0,
            amplitude: 0.5,
        };
        let s: Vec<f32> = (0..22050).map(|_| v.tick(&mods).0).collect();
        assert!((zero_crossing_rate(&s, SR) - 880.0).abs() < 40.0, "organ has extra crossings; loose bound");
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 0.26 && peak > 0.15, "level x amplitude = 0.25 on a peak-1 table: {peak}");
        v.gate_off();
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod wavetable;` and `#[allow(unused_imports)] pub use wavetable::WavetableVoice;` to `engines/mod.rs`. `cargo test engines::wavetable 2>&1 | tail`. Expected: `WavetableVoice` not found.

- [ ] **Step 3: Implement the voice**

```rust
pub struct WavetableVoice {
    level: f32,
    table: TableName,
    next: TableName,
    morph: f32,
    mip: usize,
    frequency: f32,
    phase: PhaseAccumulator,
    env: GateEnvelope,
}

impl WavetableVoice {
    pub fn new(cfg: &Wavetable, frequency: f32, sample_rate: f32) -> Self {
        Self {
            level: cfg.level,
            table: cfg.table,
            next: cfg.table.next(),
            morph: cfg.morph.clamp(0.0, 1.0),
            // The level is chosen from the note's pitch; LFO vibrato is far
            // smaller than an octave, so it stays valid for the whole note.
            mip: mip_level(frequency),
            frequency,
            phase: PhaseAccumulator::new(sample_rate),
            env: GateEnvelope::new(&cfg.env, sample_rate),
        }
    }
}

impl Voice for WavetableVoice {
    fn gate_off(&mut self) {
        self.env.gate_off();
    }

    fn is_active(&self) -> bool {
        self.env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let p = self.phase.next_unit(self.frequency * mods.pitch_ratio);
        let a = sample(self.table, self.mip, p);
        let s = if self.morph > 0.0 {
            let b = sample(self.next, self.mip, p);
            a * (1.0 - self.morph) + b * self.morph
        } else {
            a
        };
        let out = s * self.env.next() * self.level * mods.amplitude;
        (out, out)
    }
}
```

- [ ] **Step 4: Renderer hook and patches**

In `render.rs`, import `WavetableVoice` and add after the FM spawn:

```rust
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
```

Add a render test:

```rust
    #[test]
    fn wavetable_engine_renders() {
        let p = patch(json!({"name": "w", "wavetable": {"table": "organ", "env": {"release": 0.2}}}));
        let buf = left(&render_patch(&p, &[note(0.0, 0.5, 220.0)], SR));
        assert!(rms(&buf[441..22050]) > 0.3);
        assert_eq!(buf.len(), (0.7 * SR) as usize, "gate + release");
    }
```

Patch files (all new; `wt_` prefix so agents can find them):

`wt_organ.json`
```json
{
  "name": "wt_organ",
  "description": "Drawbar organ from the organ wavetable with a touch of chorus",
  "category": "keys",
  "level": 0.7,
  "wavetable": {"table": "organ", "morph": 0.0, "env": {"attack": 0.005, "decay": 0.05, "sustain": 1.0, "release": 0.15}},
  "effects": [{"type": "chorus", "rate": 0.8, "depth": 0.25, "intensity": 0.3},
              {"type": "reverb", "room_size": 0.4, "dampening": 0.5, "wet_level": 0.3, "intensity": 0.25}]
}
```

`wt_vocal_pad.json`: category `pad`, level 0.6, `{"table": "vocal", "morph": 0.35, "env": {"attack": 1.2, "decay": 1.0, "sustain": 0.8, "release": 2.5}}`, effects chorus (rate 0.4, depth 0.4, intensity 0.35) + reverb (room_size 0.8, dampening 0.4, wet_level 0.5, intensity 0.4).

`wt_pwm_lead.json`: category `lead`, level 0.75, `{"table": "pwm", "morph": 0.2, "env": {"attack": 0.01, "decay": 0.2, "sustain": 0.7, "release": 0.3}}`, effects delay (delay_time 0.375, feedback 0.3, wet_level 0.35, intensity 0.3).

`wt_glass_keys.json`: category `keys`, level 0.7, `{"table": "digital", "morph": 0.5, "env": {"attack": 0.003, "decay": 0.8, "sustain": 0.3, "release": 1.5}}`, effects reverb (room_size 0.6, dampening 0.3, wet_level 0.4, intensity 0.3).

Add the four `include_str!` lines under `// wavetable` in `BUILTIN_PATCHES`, and extend the library test with `assert!(lib.get("wt_organ").is_some()); assert!(lib.count() >= 39);`.

- [ ] **Step 5: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: all pass, including the built-in headroom test for the new files (lower a `level` if needed and say so in the commit body).

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/engines src/expressive/render.rs src/expressive/patch.rs src/expressive/patches
git commit -m "feat: wavetable voice in the renderer with organ, vocal pad, pwm lead and glass keys patches"
```

---

### Task 6: MCP schema, descriptions, integration tests

**Files:**
- Modify: `src/server/mcp.rs`, `tests/integration/mcp_protocol.rs`

**Interfaces:**
- Consumes: `Patch.fm`, `Patch.wavetable`, `FmAlgorithm::as_str`, `TableName::as_str` / `ALL`.
- Produces: `patch_schema()` has `fm` and `wavetable`; `define_synth` reports the engines; descriptions and the `list_sounds` hint mention them.

- [ ] **Step 1: Write the failing tests**

In the `mcp.rs` tests module:

```rust
    #[test]
    fn patch_schema_describes_fm_and_wavetable_and_they_validate_through_define_synth() {
        let schema = patch_schema();
        let fm = &schema["properties"]["fm"]["properties"];
        assert_eq!(fm["algorithm"]["enum"], json!(["stack", "pairs", "fan_in", "parallel"]));
        assert_eq!(fm["operators"]["maxItems"], 4);
        assert!(fm["operators"]["items"]["properties"]["ratio"].is_object());
        let wt = &schema["properties"]["wavetable"]["properties"];
        assert_eq!(wt["table"]["enum"], json!(["basic", "warm", "bright", "digital", "vocal", "pwm", "organ", "noise"]));
        assert!(wt["morph"].is_object());

        let mut state = ServerState::new();
        let r = call(&mut state, "define_synth", json!({"name": "bell", "fm": {"algorithm": "stack",
            "operators": [{"ratio": 1}, {"ratio": 3.5, "level": 0.5}]}}));
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(text(&r).contains("fm"), "{}", text(&r));
        let r = call(&mut state, "define_synth", json!({"name": "org", "wavetable": {"table": "organ"}}));
        assert!(r.error.is_none());
        assert!(text(&r).contains("wavetable"));
        let r = call(&mut state, "define_synth", json!({"name": "bad", "fm": {"operators": [{"ratio": 99}]}}));
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("fm.operators[0].ratio"));
    }
```

(Match the real field names of `JsonRpcResponse`'s error struct as the existing tests in that module do.)

In `tests/integration/mcp_protocol.rs`:

```rust
#[test]
fn fm_and_wavetable_patches_play_by_name_and_inline() {
    let mut server = TestServer::start();
    for name in ["dx7_e_piano", "tx81z_lately", "wt_organ"] {
        let r = server.call(json!({
            "jsonrpc": "2.0", "id": 30, "method": "tools/call",
            "params": {"name": "play_notes", "arguments": {"notes": [
                {"synth": name, "note": 60, "start_time": 0.0, "duration": 0.2}]}}
        }));
        assert!(r["error"].is_null(), "{name}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Playback started") || text.contains("Audio output unavailable"), "{name}: {text}");
    }
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 31, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "inline_fm", "fm": {"algorithm": "fan_in", "operators": [{"ratio": 1}, {"ratio": 2, "level": 0.3}, {"ratio": 5, "level": 0.2}]}},
             "note": 64, "start_time": 0.0, "duration": 0.2}]}}
    }));
    assert!(r["error"].is_null(), "{r}");

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 32, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "synths"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    for needle in ["dx7_e_piano", "dx7_slap_bass", "tx81z_lately", "fm_bell", "wt_organ", "wt_pwm_lead", "## keys"] {
        assert!(text.contains(needle), "catalog missing {needle}");
    }
}

#[test]
fn fm_schema_errors_name_the_operator_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 33, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "fm": {"operators": [{"ratio": 1, "detune": 5}]}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("detune"));
}
```

- [ ] **Step 2: Run to verify failure**

`cargo test server::mcp 2>&1 | tail`. Expected: the schema test fails (no `fm` property).

- [ ] **Step 3: Implement**

In `patch_schema()`, after the `"subtractive"` object and before `"percussion"`:

```rust
            "fm": {
                "type": "object",
                "description": "Four-operator FM. Operator 1 (first entry) is always a carrier; a carrier's level is its volume, a modulator's level is its modulation depth (1 = about 4 radians). Bells: high non-integer ratios with fast-decaying modulators; basses: ratio 1-2 modulators with a short decay.",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "algorithm": {"type": "string", "enum": ["stack", "pairs", "fan_in", "parallel"], "default": "stack",
                        "description": "stack: 4->3->2->1, one carrier. pairs: 3->1 and 4->2, carriers 1 and 2. fan_in: 2, 3, 4 all modulate 1. parallel: all carriers (additive)"},
                    "feedback": {"type": "number", "minimum": 0, "maximum": 1, "default": 0, "description": "Self-modulation of the last operator; adds grit"},
                    "operators": {
                        "type": "array", "minItems": 1, "maxItems": 4,
                        "description": "1 to 4 operators in order; missing operators are silent",
                        "items": {"type": "object", "properties": {
                            "ratio": {"type": "number", "minimum": 0.25, "maximum": 16, "default": 1, "description": "Frequency relative to the note (2 = one octave up, 3.5 = bell-like)"},
                            "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                            "detune_cents": {"type": "number", "minimum": -100, "maximum": 100, "default": 0},
                            "env": env("Operator")
                        }, "additionalProperties": false}
                    }
                },
                "additionalProperties": false
            },
            "wavetable": {
                "type": "object",
                "description": "One of eight band-limited tables, optionally morphed toward the next table in the list, through an amplitude envelope",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "table": {"type": "string", "enum": ["basic", "warm", "bright", "digital", "vocal", "pwm", "organ", "noise"], "default": "basic",
                        "description": "basic: soft sine+; warm: odd harmonics; bright: saw-like; digital: inharmonic/metallic; vocal: formant peaks; pwm: stacked pulse widths; organ: drawbars; noise: dense harmonics with scattered phases"},
                    "morph": {"type": "number", "minimum": 0, "maximum": 1, "default": 0, "description": "0 = the chosen table, 1 = the next table in the list (noise wraps to basic)"},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
```

Update the schema's top-level description to "Include at least one engine (subtractive, fm, wavetable or percussion); several may layer." In the `define_synth` tool description, extend the engines sentence to "Engines: subtractive (two oscillators, filter with envelope), fm (four operators, four algorithms), wavetable (eight band-limited tables with morph) and percussion (...)" and add an example line:

```
- Bell: {\"name\": \"glass_bell\", \"category\": \"keys\", \"fm\": {\"algorithm\": \"stack\", \"operators\": [{\"ratio\": 1, \"env\": {\"attack\": 0.002, \"decay\": 1.5, \"sustain\": 0.2, \"release\": 2.5}}, {\"ratio\": 3.5, \"level\": 0.55, \"env\": {\"decay\": 0.6, \"sustain\": 0}}]}, \"effects\": [{\"type\": \"reverb\", \"room_size\": 0.7, \"intensity\": 0.35}]}
- Organ: {\"name\": \"organ\", \"category\": \"keys\", \"wavetable\": {\"table\": \"organ\", \"env\": {\"attack\": 0.005, \"sustain\": 1, \"release\": 0.15}}}
```

In `handle_define_synth`, extend the engine list:

```rust
    if patch.fm.as_ref().is_some_and(|f| f.level > 0.0) {
        engines.push("fm");
    }
    if patch.wavetable.as_ref().is_some_and(|w| w.level > 0.0) {
        engines.push("wavetable");
    }
```

In `handle_list_sounds`'s `synths` section, change the trailing hint to:

```rust
        out.push_str("\nEngines for inline patches: subtractive, fm (algorithms stack/pairs/fan_in/parallel), wavetable (tables basic/warm/bright/digital/vocal/pwm/organ/noise), percussion (kinds kick/snare/hihat/cymbal/zap/swoosh/chime/burst).\n\n");
```

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: all pass (unit and integration).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/server/mcp.rs tests/integration/mcp_protocol.rs
git commit -m "feat(mcp): fm and wavetable in the patch schema, descriptions and catalog"
```

---

### Task 7: Docs, spec status, final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md`, `examples/api_reference.md`

- [ ] **Step 1: README**

In the "Engines" list under the Synth Patches section, add after the subtractive bullet:

```markdown
- **fm**: four operators (`ratio` 0.25-16, `level`, `detune_cents`, `env`) routed by `algorithm` `stack|pairs|fan_in|parallel`, plus `feedback`. Operator 1 is always a carrier; a modulator's level is its depth.
- **wavetable**: `table` `basic|warm|bright|digital|vocal|pwm|organ|noise`, `morph` 0-1 toward the next table, `env`. Tables are band-limited per octave.
```

Add one FM example to the "Define your own" area (the `glass_bell` from the tool description) and mention that `list_sounds` now has `keys` patches (`dx7_e_piano`, `fm_bell`, `wt_organ`, `wt_glass_keys`).

- [ ] **Step 2: CLAUDE.md**

Update the Synthesis bullet: "`patch.rs` / ... / `engines/` / `wavetables.rs` / `render.rs` / `patches/*.json` - agent-defined synth patches: `Patch` (subtractive, fm, wavetable and percussion engines plus an effects chain) ..." and "`patches/` - 39 built-in patches". Add: "`wavetables.rs` builds eight procedural tables once (`OnceLock`) as ten per-octave band-limited levels; `WavetableVoice` picks the level from the note's pitch."

- [ ] **Step 3: examples/api_reference.md**

Add `fm` and `wavetable` to the patch field table with the same one-line descriptions as the README.

- [ ] **Step 4: Spec**

Set `Status:` to `PR 1 and PR 2 implemented; PRs 3-4 pending`. In §1 `fm`, add: "Missing operators (fewer than the algorithm routes) are silent. Modulation depth is 4 radians at level 1 (`FM_MOD_DEPTH`). The FM engine has no filter; use a `filter` entry in the patch's `effects`." In §1 `wavetable`, add: "Band limiting: ten mip levels, one per octave from 27.5 Hz; a level keeps partials below 20 kHz for every fundamental in its octave."

- [ ] **Step 5: Verify and commit**

```bash
rg -n "31 built-in|four engines|two engines" README.md CLAUDE.md examples || echo "no stale counts"
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | grep -E "^test result"
cargo run -- test-synths   # the controller cannot hear this; the user listens after the PR is up
git add README.md CLAUDE.md examples docs/superpowers/specs
git commit -m "docs: fm and wavetable engines"
```

- [ ] **Step 6: Open the PR**

Use the `superpowers:finishing-a-development-branch` skill. PR title: `feat: fm and wavetable synth engines`. Body: link the spec and this plan; list the 8 new patches; note `FM_MOD_DEPTH` and the mip-level design; list follow-ups (PR 3 granular + LFO, PR 4 Time Fracture).

---

## Self-review notes

- Spec coverage for PR 2: §1 `fm` (Tasks 1, 2, 6), §1 `wavetable` (Tasks 4, 5, 6), §4 FM and wavetable rows (Tasks 2, 4), §5 built-ins including the three FM presets (Tasks 3, 5), §7 tests "FM: each algorithm produces sidebands; parallel none" (Task 2) and "wavetable: band-limited table aliases less than naive" (Tasks 4, 5), §8 PR 2 (this plan).
- Names used consistently: `FmAlgorithm::{Stack, Pairs, FanIn, Parallel}` with `modulators(op)`, `carriers()`, `as_str()`; `Operator { ratio, level, detune_cents, env }`; `Fm { level, algorithm, feedback, operators }` with `carrier_release()`; `FmVoice::new(&Fm, f32, f32)`; `FM_MOD_DEPTH`; `TableName` with `ALL`, `as_str`, `index`, `next`; `Wavetable { level, table, morph, env }`; `wavetables::{TABLE_SIZE, MIP_LEVELS, LOWEST_FUNDAMENTAL_HZ, Partial, partials, mip_level, max_ratio, sample, naive_sample}`; `WavetableVoice::new(&Wavetable, f32, f32)`.
- Deferred by design: LFO targets `morph`/`grain_density` (PR 3), granular engine (PR 3), a per-engine filter for FM/wavetable (use the effects chain).
