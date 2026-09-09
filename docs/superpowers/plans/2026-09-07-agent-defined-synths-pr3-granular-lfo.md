# Agent-Defined Synths, PR 3 (Granular Engine and LFO) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-patch LFO (five shapes, five targets) and a `granular` engine (four sources, grain scheduler, pitch shift, randomness, stereo width) to synth patches, expose both in the `define_synth` schema, and ship the pad, texture and drone patches that need them.

**Architecture:** `src/expressive/lfo.rs` produces a -1..1 control signal; `render_patch` runs one per patch and converts it into the per-sample `Modulation` every voice already reads (two new fields: `morph_offset`, `grain_density_ratio`). `engines/granular.rs` is a new `Voice` with its own single-cycle source buffer and up to 32 overlapping Hann-windowed grains, the first engine that emits true stereo. Nothing on the audio thread changes.

**Tech Stack:** Rust 2024, serde/serde_json, rand 0.10 (`rand::rng()`, `random::<f32>()`), existing `GateEnvelope`, `PhaseAccumulator`, `test_util` measurements.

**Spec:** `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md` (§1 `granular` and `lfo`, §3 step 2 and 3, §4, §5, §7, §8 PR 3).

## Global Constraints

- DSP is verified by measurement (`src/expressive/test_util.rs`: `goertzel_power`, `rms`, `zero_crossing_rate`, `db`), never by ear. Tests that depend on randomness must assert properties that hold for every RNG draw.
- Never allocate or log inside per-sample loops on the audio thread (`src/midi/engine.rs`, untouched here). `render_patch` may allocate before its loop, not inside it; the granular voice pre-allocates its grain slots and source buffer at note-on.
- Patch JSON is snake_case; every patch struct carries `#[serde(deny_unknown_fields)]`; validation errors name the field path and range (`check_range` in `patch.rs`) and surface as `-32602`.
- Spec §1 `lfo`: `rate` 0.1..20 Hz, `depth` 0..1, `wave` `sine | triangle | saw | square | sample_hold`, `target` `off | cutoff | pitch | amplitude | morph | grain_density`; one LFO per patch, free-running from the start of the rendered buffer; depth scaling: cutoff up to 2 octaves, pitch up to 2 semitones, amplitude 0 to 100% tremolo, morph the full 0..1 range, grain density up to 2x.
- Spec §1 `granular`: `source` `harmonics | noise | formant | inharmonic`; `grain_ms` 5..500; `density` 1..50 grains per second; `pitch_semitones` -24..24; `randomness` 0..1; `stereo_width` 0..1; `env`.
- Built-in patches peak ≤ 1.6 before the bus gain at velocity 100/127 (existing library test).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass before every commit.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jxcj13gt2pyZbmDvvC1Pep
  ```
- Work in the worktree `.claude/worktrees/feat-synth-granular-lfo` (branch `worktree-feat-synth-granular-lfo`, from `main` cd56809).

---

## File map

| File | Responsibility |
|---|---|
| `src/expressive/lfo.rs` (new) | `Lfo` oscillator (five shapes, sample-and-hold latch) |
| `src/expressive/patch.rs` | `LfoWave`, `LfoTarget`, `LfoConfig`, `GrainSource`, `Granular`; `Patch.lfo`, `Patch.granular`; validation; engine helpers |
| `src/expressive/engines/mod.rs` | `Modulation` gains `morph_offset`, `grain_density_ratio`; `Modulation::from_lfo`; module list |
| `src/expressive/engines/wavetable.rs` | reads `mods.morph_offset` |
| `src/expressive/engines/granular.rs` (new) | `GranularVoice` |
| `src/expressive/render.rs` | one `Lfo` per patch drives `Modulation` per sample; spawns granular voices |
| `src/expressive/patches/*.json` | 4 new patches, `warm_pad` gains an LFO |
| `src/server/mcp.rs` | `patch_schema` gains `lfo` and `granular`; descriptions; engine report; catalog line |
| `tests/integration/mcp_protocol.rs` | LFO and granular through the binary |
| `README.md`, `CLAUDE.md`, `examples/api_reference.md`, spec | docs |

---

### Task 1: LFO oscillator, LFO config, and `Modulation` mapping

**Files:**
- Create: `src/expressive/lfo.rs`
- Modify: `src/expressive/patch.rs`, `src/expressive/engines/mod.rs`, `src/expressive/mod.rs`

**Interfaces:**
- Consumes: `check_range` (private helper in `patch.rs`), `rand::Rng`.
- Produces:
  ```rust
  // patch.rs
  #[serde(rename_all = "snake_case")] pub enum LfoWave { Sine, Triangle, Saw, Square, SampleHold }   // Default = Sine
  #[serde(rename_all = "snake_case")] pub enum LfoTarget { Off, Cutoff, Pitch, Amplitude, Morph, GrainDensity } // Default = Off
  impl LfoTarget { pub const ALL: [LfoTarget; 6]; pub fn as_str(&self) -> &'static str; }
  impl LfoWave { pub const ALL: [LfoWave; 5]; pub fn as_str(&self) -> &'static str; }
  pub struct LfoConfig { pub rate: f32, pub depth: f32, pub wave: LfoWave, pub target: LfoTarget } // Default: 1.0, 0.0, Sine, Off
  impl LfoConfig { pub fn is_active(&self) -> bool /* target != Off && depth > 0 */ }
  pub struct Patch { ..., pub lfo: Option<LfoConfig>, ... }
  // lfo.rs
  pub struct Lfo; impl Lfo { pub fn new(rate: f32, wave: LfoWave, sample_rate: f32) -> Self; pub fn next(&mut self) -> f32 /* -1..1 */ }
  // engines/mod.rs
  pub struct Modulation { pub pitch_ratio: f32, pub cutoff_ratio: f32, pub amplitude: f32, pub morph_offset: f32, pub grain_density_ratio: f32 } // Default 1,1,1,0,1
  impl Modulation { pub fn from_lfo(target: LfoTarget, depth: f32, value: f32) -> Self; }
  ```
  Mapping in `from_lfo` (value `v` in -1..1, depth `d` in 0..1): `Cutoff` → `cutoff_ratio = 2^(2 v d)`; `Pitch` → `pitch_ratio = 2^(2 v d / 12)`; `Amplitude` → `amplitude = 1 - d (1 - v) / 2` (so depth 1 swings 0..1, depth 0.5 swings 0.5..1); `Morph` → `morph_offset = v d / 2` (depth 1 sweeps ±0.5 around the patch's morph); `GrainDensity` → `grain_density_ratio = 2^(v d)` (depth 1 sweeps 0.5x..2x); `Off` → `Default`.

- [ ] **Step 1: Write the failing tests**

Create `src/expressive/lfo.rs`:

```rust
//! Low-frequency oscillator: a -1..1 control signal in one of five shapes,
//! advanced once per sample and mapped onto a `Modulation` by the renderer.

use crate::expressive::LfoWave;
use rand::Rng;
use std::f32::consts::TAU;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::test_util::zero_crossing_rate;

    const SR: f32 = 1000.0;

    fn run(wave: LfoWave, rate: f32, seconds: f32) -> Vec<f32> {
        let mut lfo = Lfo::new(rate, wave, SR);
        (0..(seconds * SR) as usize).map(|_| lfo.next()).collect()
    }

    #[test]
    fn every_shape_stays_within_minus_one_to_one() {
        for wave in LfoWave::ALL {
            let s = run(wave, 3.0, 2.0);
            assert!(s.iter().all(|x| (-1.0..=1.0).contains(x)), "{wave:?}");
        }
    }

    #[test]
    fn sine_and_triangle_cross_zero_at_twice_the_rate() {
        for wave in [LfoWave::Sine, LfoWave::Triangle] {
            let s = run(wave, 2.0, 5.0);
            assert!((zero_crossing_rate(&s, SR) - 4.0).abs() < 0.5, "{wave:?}");
        }
    }

    #[test]
    fn saw_ramps_up_and_resets_once_per_cycle() {
        let s = run(LfoWave::Saw, 1.0, 2.0);
        assert!(s[0] < -0.99 && s[499] > 0.99, "ramps from -1 to +1 over one second");
        assert!(s[1000] < -0.99, "resets at the cycle boundary");
    }

    #[test]
    fn square_holds_plus_one_then_minus_one() {
        let s = run(LfoWave::Square, 1.0, 1.0);
        assert!(s[..500].iter().all(|&x| x == 1.0));
        assert!(s[500..].iter().all(|&x| x == -1.0));
    }

    #[test]
    fn sample_hold_holds_one_value_per_cycle_and_changes_between_cycles() {
        let s = run(LfoWave::SampleHold, 2.0, 5.0);
        for cycle in 0..10 {
            let c = &s[cycle * 500..(cycle + 1) * 500];
            assert!(c.iter().all(|&x| x == c[0]), "held within cycle {cycle}");
        }
        let firsts: Vec<f32> = (0..10).map(|c| s[c * 500]).collect();
        assert!(firsts.windows(2).any(|w| w[0] != w[1]), "changes between cycles");
    }
}
```

Append to the tests module in `src/expressive/patch.rs`:

```rust
    #[test]
    fn lfo_config_parses_with_defaults_validates_and_reports_activity() {
        let p = parse(json!({"name": "x", "subtractive": {}, "lfo": {}})).unwrap();
        let lfo = p.lfo.as_ref().unwrap();
        assert_eq!((lfo.rate, lfo.depth), (1.0, 0.0));
        assert_eq!(lfo.wave, LfoWave::Sine);
        assert_eq!(lfo.target, LfoTarget::Off);
        assert!(!lfo.is_active());
        assert!(p.validate().is_ok());

        let p = parse(json!({"name": "x", "subtractive": {},
            "lfo": {"rate": 0.3, "depth": 0.2, "wave": "sample_hold", "target": "cutoff"}})).unwrap();
        assert!(p.lfo.as_ref().unwrap().is_active());
        assert_eq!(serde_json::to_value(&p).unwrap()["lfo"]["target"], "cutoff");

        let p = parse(json!({"name": "x", "subtractive": {}, "lfo": {"rate": 50}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(err.contains("lfo.rate") && err.contains("20"), "{err}");
        let p = parse(json!({"name": "x", "subtractive": {}, "lfo": {"depth": 2}})).unwrap();
        assert!(p.validate().unwrap_err().contains("lfo.depth"));
        assert!(parse(json!({"name": "x", "subtractive": {}, "lfo": {"target": "filter"}})).is_err());
        assert!(parse(json!({"name": "x", "subtractive": {}, "lfo": {"speed": 1}})).unwrap_err().contains("speed"));
        assert_eq!(LfoTarget::ALL.len(), 6);
        assert_eq!(LfoTarget::GrainDensity.as_str(), "grain_density");
        assert_eq!(LfoWave::SampleHold.as_str(), "sample_hold");
    }
```

Append to the tests in `src/expressive/engines/mod.rs` (create the module's tests block if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::LfoTarget;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn modulation_from_lfo_maps_each_target_with_the_spec_ranges() {
        let d = Modulation::default();
        assert!(close(d.morph_offset, 0.0) && close(d.grain_density_ratio, 1.0));

        let m = Modulation::from_lfo(LfoTarget::Cutoff, 1.0, 1.0);
        assert!(close(m.cutoff_ratio, 4.0), "two octaves up at full depth");
        let m = Modulation::from_lfo(LfoTarget::Cutoff, 1.0, -1.0);
        assert!(close(m.cutoff_ratio, 0.25));

        let m = Modulation::from_lfo(LfoTarget::Pitch, 1.0, 1.0);
        assert!(close(m.pitch_ratio, 2f32.powf(2.0 / 12.0)), "two semitones");
        let m = Modulation::from_lfo(LfoTarget::Pitch, 0.5, 1.0);
        assert!(close(m.pitch_ratio, 2f32.powf(1.0 / 12.0)));

        let m = Modulation::from_lfo(LfoTarget::Amplitude, 1.0, -1.0);
        assert!(close(m.amplitude, 0.0), "full tremolo reaches silence");
        let m = Modulation::from_lfo(LfoTarget::Amplitude, 1.0, 1.0);
        assert!(close(m.amplitude, 1.0));
        let m = Modulation::from_lfo(LfoTarget::Amplitude, 0.5, -1.0);
        assert!(close(m.amplitude, 0.5));

        let m = Modulation::from_lfo(LfoTarget::Morph, 1.0, 1.0);
        assert!(close(m.morph_offset, 0.5));
        let m = Modulation::from_lfo(LfoTarget::GrainDensity, 1.0, 1.0);
        assert!(close(m.grain_density_ratio, 2.0));
        let m = Modulation::from_lfo(LfoTarget::GrainDensity, 1.0, -1.0);
        assert!(close(m.grain_density_ratio, 0.5));

        let m = Modulation::from_lfo(LfoTarget::Off, 1.0, 1.0);
        assert_eq!(m, Modulation::default());
        let m = Modulation::from_lfo(LfoTarget::Pitch, 0.0, 1.0);
        assert_eq!(m, Modulation::default(), "zero depth is identity");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod lfo;` and `#[allow(unused_imports)] pub use lfo::*;` to `src/expressive/mod.rs`. Run `cargo test lfo modulation_from_lfo 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement the config types**

In `src/expressive/patch.rs`, after the `Wavetable` struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LfoWave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    SampleHold,
}

impl LfoWave {
    pub const ALL: [LfoWave; 5] = [
        LfoWave::Sine,
        LfoWave::Triangle,
        LfoWave::Saw,
        LfoWave::Square,
        LfoWave::SampleHold,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            LfoWave::Sine => "sine",
            LfoWave::Triangle => "triangle",
            LfoWave::Saw => "saw",
            LfoWave::Square => "square",
            LfoWave::SampleHold => "sample_hold",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LfoTarget {
    #[default]
    Off,
    Cutoff,
    Pitch,
    Amplitude,
    Morph,
    GrainDensity,
}

impl LfoTarget {
    pub const ALL: [LfoTarget; 6] = [
        LfoTarget::Off,
        LfoTarget::Cutoff,
        LfoTarget::Pitch,
        LfoTarget::Amplitude,
        LfoTarget::Morph,
        LfoTarget::GrainDensity,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            LfoTarget::Off => "off",
            LfoTarget::Cutoff => "cutoff",
            LfoTarget::Pitch => "pitch",
            LfoTarget::Amplitude => "amplitude",
            LfoTarget::Morph => "morph",
            LfoTarget::GrainDensity => "grain_density",
        }
    }
}

/// One low-frequency oscillator per patch, free-running from the start of
/// the rendered buffer, routed to a single target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LfoConfig {
    pub rate: f32,
    pub depth: f32,
    pub wave: LfoWave,
    pub target: LfoTarget,
}

impl Default for LfoConfig {
    fn default() -> Self {
        Self {
            rate: 1.0,
            depth: 0.0,
            wave: LfoWave::Sine,
            target: LfoTarget::Off,
        }
    }
}

impl LfoConfig {
    pub fn is_active(&self) -> bool {
        self.target != LfoTarget::Off && self.depth > 0.0
    }

    fn validate(&self, path: &str) -> Result<(), String> {
        check_range(&format!("{path}.rate"), self.rate, 0.1, 20.0)?;
        check_range(&format!("{path}.depth"), self.depth, 0.0, 1.0)
    }
}
```

Add to `Patch` after `percussion`:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lfo: Option<LfoConfig>,
```

and in `Patch::validate`, after the percussion block: `if let Some(lfo) = &self.lfo { lfo.validate("lfo")?; }`. The LFO is not an engine, so the "no engines" check and `has_pitched_engine` ignore it.

- [ ] **Step 4: Implement the oscillator**

Above the tests in `src/expressive/lfo.rs`:

```rust
pub struct Lfo {
    wave: LfoWave,
    /// Cycles per sample.
    increment: f32,
    /// Unit phase 0..1.
    phase: f32,
    held: f32,
    rng: rand::rngs::ThreadRng,
}

impl Lfo {
    pub fn new(rate: f32, wave: LfoWave, sample_rate: f32) -> Self {
        let mut rng = rand::rng();
        let held = rng.random::<f32>() * 2.0 - 1.0;
        Self {
            wave,
            increment: rate.max(0.0) / sample_rate,
            phase: 0.0,
            held,
            rng,
        }
    }

    /// The value for the current sample, then advance one sample.
    #[inline]
    pub fn next(&mut self) -> f32 {
        let p = self.phase;
        let value = match self.wave {
            LfoWave::Sine => (p * TAU).sin(),
            LfoWave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            LfoWave::Saw => 2.0 * p - 1.0,
            LfoWave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWave::SampleHold => self.held,
        };
        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.held = self.rng.random::<f32>() * 2.0 - 1.0;
        }
        value
    }
}
```

Note `Triangle` starts at -1 (phase 0), peaks at +1 (phase 0.5); `Sine` starts at 0 rising. Both cross zero twice per cycle as the test expects.

- [ ] **Step 5: Extend `Modulation`**

In `src/expressive/engines/mod.rs` replace the `Modulation` struct and its `Default` with:

```rust
/// Per-sample modulation inputs shared by all voices; the renderer derives
/// them from the patch's LFO each sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modulation {
    pub pitch_ratio: f32,
    pub cutoff_ratio: f32,
    pub amplitude: f32,
    /// Added to a wavetable voice's morph position, then clamped to 0..1.
    pub morph_offset: f32,
    /// Multiplies a granular voice's grain rate.
    pub grain_density_ratio: f32,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            cutoff_ratio: 1.0,
            amplitude: 1.0,
            morph_offset: 0.0,
            grain_density_ratio: 1.0,
        }
    }
}

impl Modulation {
    /// Map an LFO sample (`value` in -1..1) at `depth` onto `target` with the
    /// spec's ranges: cutoff ±2 octaves, pitch ±2 semitones, amplitude down to
    /// silence, morph ±0.5, grain density 0.5x..2x.
    pub fn from_lfo(target: LfoTarget, depth: f32, value: f32) -> Self {
        let d = depth.clamp(0.0, 1.0);
        let v = value.clamp(-1.0, 1.0);
        let mut m = Self::default();
        if d == 0.0 {
            return m;
        }
        match target {
            LfoTarget::Off => {}
            LfoTarget::Cutoff => m.cutoff_ratio = 2f32.powf(2.0 * v * d),
            LfoTarget::Pitch => m.pitch_ratio = 2f32.powf(2.0 * v * d / 12.0),
            LfoTarget::Amplitude => m.amplitude = 1.0 - d * (1.0 - v) / 2.0,
            LfoTarget::Morph => m.morph_offset = v * d / 2.0,
            LfoTarget::GrainDensity => m.grain_density_ratio = 2f32.powf(v * d),
        }
        m
    }
}
```

Add `use crate::expressive::LfoTarget;` at the top of `engines/mod.rs`. Any struct literal of `Modulation` elsewhere in tests (e.g. `Modulation { pitch_ratio: 2.0, cutoff_ratio: 1.0, amplitude: 0.5 }` in the engine tests) must gain `..Modulation::default()`; fix each compile error that way.

- [ ] **Step 6: Run the tests**

```bash
cargo test lfo modulation_from_lfo patch:: engines:: 2>&1 | tail -30 && cargo test 2>&1 | grep -E "^test result"
```

Expected: all pass (147 + 7 unit, 34 integration).

- [ ] **Step 7: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/lfo.rs src/expressive/patch.rs src/expressive/engines/mod.rs src/expressive/engines src/expressive/mod.rs
git commit -m "feat: lfo oscillator, lfo config and modulation mapping"
```

---

### Task 2: LFO in the renderer; wavetable morph follows the LFO

**Files:**
- Modify: `src/expressive/render.rs`, `src/expressive/engines/wavetable.rs`
- Modify: `src/expressive/patches/warm_pad.json`

**Interfaces:**
- Consumes: `Lfo::new(rate, wave, sample_rate)`, `Lfo::next()`, `Modulation::from_lfo(target, depth, value)`, `LfoConfig::is_active()`, `Patch.lfo`.
- Produces: `render_patch` computes `mods` per sample from the patch's LFO (identity when absent or inactive); `WavetableVoice::tick` uses `(self.morph + mods.morph_offset).clamp(0.0, 1.0)`.

- [ ] **Step 1: Write the failing tests**

In `src/expressive/render.rs` tests:

```rust
    /// Zero-crossing rate in successive 50 ms windows.
    fn zcr_windows(s: &[f32]) -> Vec<f32> {
        s.chunks(2205)
            .map(|w| crate::expressive::test_util::zero_crossing_rate(w, SR))
            .collect()
    }

    #[test]
    fn pitch_lfo_moves_the_pitch_up_and_down_at_the_lfo_rate() {
        let p = patch(json!({"name": "vib", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 2.0, "depth": 1.0, "wave": "sine", "target": "pitch"}}));
        let s = left(&render_patch(&p, &[note(0.0, 2.0, 440.0)], SR));
        let z = zcr_windows(&s);
        let (lo, hi) = z.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        // 440 Hz ± 2 semitones = 392..494 Hz, i.e. 784..988 crossings/s.
        assert!(hi > 940.0 && lo < 830.0, "pitch swings: {lo}..{hi}");
        let dry = patch(json!({"name": "dry", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}}));
        let zd = zcr_windows(&left(&render_patch(&dry, &[note(0.0, 2.0, 440.0)], SR)));
        let (lo, hi) = zd.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(hi - lo < 20.0, "no LFO, no swing: {lo}..{hi}");
    }

    #[test]
    fn cutoff_lfo_varies_high_harmonic_energy() {
        let p = patch(json!({"name": "wah", "subtractive": {"osc1": {"wave": "saw"},
            "filter": {"type": "low_pass", "cutoff": 400, "resonance": 0.2},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 1.0, "depth": 1.0, "wave": "square", "target": "cutoff"}}));
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 110.0)], SR));
        // Square LFO: first half cycle cutoff x4 (1600 Hz), second half /4 (100 Hz).
        let open = crate::expressive::test_util::goertzel_power(&s[2205..20000], 1100.0, SR);
        let closed = crate::expressive::test_util::goertzel_power(&s[24255..42000], 1100.0, SR);
        assert!(crate::expressive::test_util::db(open / closed) > 20.0, "10th harmonic follows the LFO");
    }

    #[test]
    fn amplitude_lfo_is_a_tremolo() {
        let p = patch(json!({"name": "trem", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 4.0, "depth": 1.0, "wave": "sine", "target": "amplitude"}}));
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let env: Vec<f32> = s[2205..].chunks(441).map(rms).collect();
        let (lo, hi) = env.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(lo < hi * 0.2, "amplitude dips near silence: {lo} vs {hi}");
    }

    #[test]
    fn morph_lfo_moves_the_wavetable_between_tables() {
        // basic -> warm: the 3rd harmonic grows with morph.
        let p = patch(json!({"name": "mw", "wavetable": {"table": "basic", "morph": 0.5,
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "lfo": {"rate": 1.0, "depth": 1.0, "wave": "square", "target": "morph"}}));
        let s = left(&render_patch(&p, &[note(0.0, 1.0, 220.0)], SR));
        let h3 = |w: &[f32]| crate::expressive::test_util::goertzel_power(w, 660.0, SR)
            / crate::expressive::test_util::goertzel_power(w, 220.0, SR);
        let first = h3(&s[2205..20000]);   // morph 1.0 (warm)
        let second = h3(&s[24255..42000]); // morph 0.0 (basic)
        assert!(crate::expressive::test_util::db(first / second) > 10.0, "{first} vs {second}");
    }

    #[test]
    fn an_inactive_lfo_is_identity() {
        let with = patch(json!({"name": "a", "subtractive": {"env": {"release": 0.01}},
            "lfo": {"rate": 5.0, "depth": 0.0, "target": "pitch"}}));
        let without = patch(json!({"name": "b", "subtractive": {"env": {"release": 0.01}}}));
        let a = left(&render_patch(&with, &[note(0.0, 0.5, 220.0)], SR));
        let b = left(&render_patch(&without, &[note(0.0, 0.5, 220.0)], SR));
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6));
    }
```

In `src/expressive/engines/wavetable.rs` tests:

```rust
    #[test]
    fn morph_offset_modulation_shifts_toward_the_next_table_and_clamps() {
        let base = cfg(TableName::Basic, 0.0);
        let mut v0 = WavetableVoice::new(&base, 220.0, SR);
        let mut v1 = WavetableVoice::new(&base, 220.0, SR);
        let plain = Modulation::default();
        let pushed = Modulation {
            morph_offset: 0.5,
            ..Modulation::default()
        };
        let a: Vec<f32> = (0..SR as usize).map(|_| v0.tick(&plain).0).collect();
        let b: Vec<f32> = (0..SR as usize).map(|_| v1.tick(&pushed).0).collect();
        let h3 = |s: &[f32]| goertzel_power(s, 660.0, SR) / goertzel_power(s, 220.0, SR);
        assert!(db(h3(&b) / h3(&a)) > 6.0, "offset moves toward warm");
        // Negative offset from morph 0 clamps to 0: identical to plain.
        let mut v2 = WavetableVoice::new(&base, 220.0, SR);
        let neg = Modulation {
            morph_offset: -0.5,
            ..Modulation::default()
        };
        let c: Vec<f32> = (0..SR as usize).map(|_| v2.tick(&neg).0).collect();
        assert!(a.iter().zip(&c).all(|(x, y)| (x - y).abs() < 1e-6));
    }
```

- [ ] **Step 2: Run to verify failure**

`cargo test render:: engines::wavetable 2>&1 | tail`. Expected: the LFO tests fail (no modulation applied); `morph_offset` test fails.

- [ ] **Step 3: Implement**

`src/expressive/engines/wavetable.rs` `tick`:

```rust
    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let p = self.phase.next_unit(self.frequency * mods.pitch_ratio);
        let morph = (self.morph + mods.morph_offset).clamp(0.0, 1.0);
        let a = sample(self.table, self.mip, p);
        let s = if morph > 0.0 {
            let b = sample(self.next, self.mip, p);
            a * (1.0 - morph) + b * morph
        } else {
            a
        };
        let out = s * self.env.next() * self.level * mods.amplitude;
        (out, out)
    }
```

`src/expressive/render.rs`: import `crate::expressive::Lfo` and replace `let mods = Modulation::default();` and the loop with:

```rust
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
```

Update the module doc of `render.rs` to mention the LFO. Then give `warm_pad.json` the spec's example LFO: add `"lfo": {"rate": 0.3, "depth": 0.2, "wave": "sine", "target": "cutoff"}` after `"subtractive"`.

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: all pass. If `cutoff_lfo_varies_high_harmonic_energy` is marginal, the windows are already inside each half cycle; lower the base cutoff to 300 rather than the threshold.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/render.rs src/expressive/engines/wavetable.rs src/expressive/patches/warm_pad.json
git commit -m "feat(render): per-patch lfo drives modulation; wavetable morph follows it"
```

---

### Task 3: Granular data model and voice

**Files:**
- Modify: `src/expressive/patch.rs`
- Create: `src/expressive/engines/granular.rs`
- Modify: `src/expressive/engines/mod.rs`

**Interfaces:**
- Consumes: `GateEnvelope`, `Modulation` (with `grain_density_ratio`, `pitch_ratio`, `amplitude`), `Voice`, `check_range`, `Adsr`.
- Produces:
  ```rust
  // patch.rs
  #[serde(rename_all = "snake_case")] pub enum GrainSource { Harmonics, Noise, Formant, Inharmonic } // Default = Harmonics
  impl GrainSource { pub const ALL: [GrainSource; 4]; pub fn as_str(&self) -> &'static str; }
  pub struct Granular { pub level: f32, pub source: GrainSource, pub grain_ms: f32, pub density: f32, pub pitch_semitones: f32, pub randomness: f32, pub stereo_width: f32, pub env: Adsr }
  // Default: 1.0, Harmonics, 50.0, 10.0, 0.0, 0.2, 0.5, Adsr::default()
  pub struct Patch { ..., pub granular: Option<Granular>, ... }   // joins release_seconds, has_pitched_engine, "no engines", validate
  // engines/granular.rs
  pub const MAX_GRAINS: usize = 32;
  pub const SOURCE_SAMPLES: usize = 4096;
  pub fn source_cycle(source: GrainSource, rng: &mut impl rand::Rng) -> Vec<f32>;  // one cycle, peak-normalised
  pub struct GranularVoice; impl GranularVoice { pub fn new(cfg: &Granular, frequency: f32, sample_rate: f32) -> Self }  // Voice
  ```

- [ ] **Step 1: Write the failing tests**

Append to `patch.rs` tests:

```rust
    #[test]
    fn granular_patch_parses_validates_and_names_fields() {
        let p = parse(json!({"name": "g", "granular": {}})).unwrap();
        let g = p.granular.as_ref().unwrap();
        assert_eq!(g.source, GrainSource::Harmonics);
        assert_eq!((g.grain_ms, g.density, g.pitch_semitones, g.randomness, g.stereo_width),
            (50.0, 10.0, 0.0, 0.2, 0.5));
        assert!(p.validate().is_ok() && p.has_pitched_engine());
        let p = parse(json!({"name": "g", "granular": {"source": "formant", "grain_ms": 120,
            "density": 15, "pitch_semitones": 7, "randomness": 0.7, "stereo_width": 0.9,
            "env": {"release": 3.0}}})).unwrap();
        assert!(p.validate().is_ok());
        assert_eq!(p.release_seconds(), 3.0);
        assert_eq!(serde_json::to_value(&p).unwrap()["granular"]["source"], "formant");
        for (field, value) in [("grain_ms", 1000.0), ("density", 0.5), ("pitch_semitones", 30.0),
            ("randomness", 1.5), ("stereo_width", -0.1), ("level", 2.0)] {
            let p = parse(json!({"name": "g", "granular": {field: value}})).unwrap();
            let err = p.validate().unwrap_err();
            assert!(err.contains(&format!("granular.{field}")), "{field}: {err}");
        }
        assert!(parse(json!({"name": "g", "granular": {"source": "sample"}})).is_err());
        assert!(parse(json!({"name": "g", "granular": {"grain_size": 0.1}})).unwrap_err().contains("grain_size"));
        let p = parse(json!({"name": "silent"})).unwrap();
        assert!(p.validate().unwrap_err().contains("granular"), "no-engines message lists granular");
    }
```

Create `src/expressive/engines/granular.rs`:

```rust
//! Granular synthesis: a cloud of short Hann-windowed grains read from a
//! single-cycle source waveform at the note's pitch, with random start
//! positions and random stereo placement.

use crate::expressive::engines::{Modulation, Voice};
use crate::expressive::{GateEnvelope, GrainSource, Granular};
use rand::Rng;
use std::f32::consts::{FRAC_PI_2, TAU};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expressive::Adsr;
    use crate::expressive::test_util::{goertzel_power, rms, zero_crossing_rate};

    const SR: f32 = 44100.0;

    fn fast() -> Adsr {
        Adsr {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.01,
        }
    }

    fn cfg() -> Granular {
        Granular {
            level: 1.0,
            source: GrainSource::Harmonics,
            grain_ms: 50.0,
            density: 20.0,
            pitch_semitones: 0.0,
            randomness: 0.0,
            stereo_width: 0.0,
            env: fast(),
        }
    }

    fn render(c: &Granular, freq: f32, seconds: f32, mods: &Modulation) -> (Vec<f32>, Vec<f32>) {
        let mut v = GranularVoice::new(c, freq, SR);
        let mut l = Vec::new();
        let mut r = Vec::new();
        for _ in 0..(seconds * SR) as usize {
            let (a, b) = v.tick(mods);
            l.push(a);
            r.push(b);
        }
        (l, r)
    }

    #[test]
    fn every_source_is_a_normalised_single_cycle() {
        let mut rng = rand::rng();
        for source in GrainSource::ALL {
            let cycle = source_cycle(source, &mut rng);
            assert_eq!(cycle.len(), SOURCE_SAMPLES);
            let peak = cycle.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((peak - 1.0).abs() < 1e-3, "{source:?} peak {peak}");
            assert!(cycle.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn a_dense_cloud_of_harmonic_grains_is_pitched_at_the_note() {
        let (l, _) = render(&cfg(), 220.0, 1.0, &Modulation::default());
        assert!(rms(&l[4410..]) > 0.1, "audible");
        let f = goertzel_power(&l[4410..], 220.0, SR);
        let off = goertzel_power(&l[4410..], 330.0, SR);
        assert!(f > off * 10.0, "fundamental dominates a non-harmonic bin");
    }

    #[test]
    fn pitch_semitones_transposes_the_grains() {
        let mut up = cfg();
        up.pitch_semitones = 12.0;
        up.grain_ms = 200.0;
        let (a, _) = render(&cfg(), 220.0, 1.0, &Modulation::default());
        let (b, _) = render(&up, 220.0, 1.0, &Modulation::default());
        let za = zero_crossing_rate(&a[8820..], SR);
        let zb = zero_crossing_rate(&b[8820..], SR);
        assert!(zb > za * 1.6, "an octave up doubles the crossing rate: {za} -> {zb}");
    }

    #[test]
    fn stereo_width_zero_is_mono_and_width_one_is_not() {
        let (l, r) = render(&cfg(), 220.0, 0.5, &Modulation::default());
        assert!(l.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-6), "width 0 is centred");
        let mut wide = cfg();
        wide.stereo_width = 1.0;
        wide.randomness = 0.5;
        let (l, r) = render(&wide, 220.0, 0.5, &Modulation::default());
        let diff: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.05, "width 1 spreads grains across the field");
    }

    #[test]
    fn higher_density_is_louder_and_the_lfo_ratio_scales_it() {
        let mut sparse = cfg();
        sparse.density = 2.0;
        sparse.grain_ms = 20.0;
        let mut dense = sparse.clone();
        dense.density = 40.0;
        let (a, _) = render(&sparse, 220.0, 1.0, &Modulation::default());
        let (b, _) = render(&dense, 220.0, 1.0, &Modulation::default());
        assert!(rms(&b) > rms(&a) * 1.5, "{} vs {}", rms(&a), rms(&b));
        let doubled = Modulation {
            grain_density_ratio: 2.0,
            ..Modulation::default()
        };
        let (c, _) = render(&sparse, 220.0, 1.0, &doubled);
        assert!(rms(&c) > rms(&a) * 1.15, "density ratio 2 adds grains: {} vs {}", rms(&a), rms(&c));
    }

    #[test]
    fn release_then_silence_and_level_apply() {
        let mut c = cfg();
        c.level = 0.5;
        c.env.release = 0.3;
        let mut v = GranularVoice::new(&c, 220.0, SR);
        let mods = Modulation::default();
        let s: Vec<f32> = (0..22050).map(|_| v.tick(&mods).0).collect();
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 0.5 + 1e-3 && peak > 0.1, "level bounds the output: {peak}");
        v.gate_off();
        for _ in 0..(0.5 * SR) as usize {
            v.tick(&mods);
        }
        assert!(!v.is_active());
        assert!(v.tick(&mods).0 == 0.0);
    }

    #[test]
    fn never_exceeds_the_grain_cap_or_produces_nan() {
        let mut c = cfg();
        c.density = 50.0;
        c.grain_ms = 500.0;
        c.randomness = 1.0;
        c.stereo_width = 1.0;
        let (l, r) = render(&c, 55.0, 2.0, &Modulation::default());
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.5));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod granular;` and `#[allow(unused_imports)] pub use granular::{GranularVoice, MAX_GRAINS, SOURCE_SAMPLES};` to `engines/mod.rs`. `cargo test granular 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement the model**

In `patch.rs` after `Wavetable`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrainSource {
    #[default]
    Harmonics,
    Noise,
    Formant,
    Inharmonic,
}

impl GrainSource {
    pub const ALL: [GrainSource; 4] = [
        GrainSource::Harmonics,
        GrainSource::Noise,
        GrainSource::Formant,
        GrainSource::Inharmonic,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            GrainSource::Harmonics => "harmonics",
            GrainSource::Noise => "noise",
            GrainSource::Formant => "formant",
            GrainSource::Inharmonic => "inharmonic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Granular {
    pub level: f32,
    pub source: GrainSource,
    /// Grain length in milliseconds (5 to 500).
    pub grain_ms: f32,
    /// Grains started per second (1 to 50).
    pub density: f32,
    pub pitch_semitones: f32,
    /// Random start position inside the source cycle (0 = always the start).
    pub randomness: f32,
    /// Random stereo placement of each grain (0 = centre, 1 = full width).
    pub stereo_width: f32,
    pub env: Adsr,
}

impl Default for Granular {
    fn default() -> Self {
        Self {
            level: 1.0,
            source: GrainSource::Harmonics,
            grain_ms: 50.0,
            density: 10.0,
            pitch_semitones: 0.0,
            randomness: 0.2,
            stereo_width: 0.5,
            env: Adsr::default(),
        }
    }
}

impl Granular {
    fn validate(&self, path: &str) -> Result<(), String> {
        check_range(&format!("{path}.level"), self.level, 0.0, 1.0)?;
        check_range(&format!("{path}.grain_ms"), self.grain_ms, 5.0, 500.0)?;
        check_range(&format!("{path}.density"), self.density, 1.0, 50.0)?;
        check_range(&format!("{path}.pitch_semitones"), self.pitch_semitones, -24.0, 24.0)?;
        check_range(&format!("{path}.randomness"), self.randomness, 0.0, 1.0)?;
        check_range(&format!("{path}.stereo_width"), self.stereo_width, 0.0, 1.0)?;
        self.env.validate(&format!("{path}.env"))
    }
}
```

Add `pub granular: Option<Granular>` to `Patch` after `wavetable` (same serde attrs); include it in `release_seconds` (`.max(granular release when level > 0)`), `has_pitched_engine`, the "no engines" check (message `add "subtractive", "fm", "wavetable", "granular" or "percussion"`) and `validate` (`g.validate("granular")?`).

- [ ] **Step 4: Implement the voice**

Above the tests in `engines/granular.rs`:

```rust
pub const MAX_GRAINS: usize = 32;
pub const SOURCE_SAMPLES: usize = 4096;

/// One cycle of the source waveform, peak-normalised to 1. `noise` is a
/// fresh random cycle per voice, so it buzzes at the note's pitch.
pub fn source_cycle(source: GrainSource, rng: &mut impl Rng) -> Vec<f32> {
    let mut cycle = vec![0.0f32; SOURCE_SAMPLES];
    for (i, s) in cycle.iter_mut().enumerate() {
        let phase = i as f32 / SOURCE_SAMPLES as f32;
        let h = |n: f32| (TAU * n * phase).sin();
        *s = match source {
            GrainSource::Harmonics => 0.5 * h(1.0) + 0.25 * h(2.0) + 0.15 * h(3.0) + 0.1 * h(5.0),
            GrainSource::Noise => rng.random::<f32>() * 2.0 - 1.0,
            GrainSource::Formant => {
                0.3 * h(1.0) + 0.5 * h(3.0) + 0.4 * h(5.0) + 0.2 * h(7.0) + 0.1 * h(11.0)
            }
            GrainSource::Inharmonic => {
                0.4 * h(1.0) + 0.3 * h(1.414) + 0.2 * h(2.718) + 0.1 * h(3.141)
            }
        };
    }
    let peak = cycle.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        for s in &mut cycle {
            *s /= peak;
        }
    }
    cycle
}

#[derive(Clone, Copy, Default)]
struct Grain {
    active: bool,
    /// Read position in the source cycle, in cycles (fractional).
    pos: f32,
    /// Cycles advanced per sample.
    pos_inc: f32,
    /// 0..1 across the grain's life.
    life: f32,
    life_inc: f32,
    left_gain: f32,
    right_gain: f32,
}

pub struct GranularVoice {
    cfg: Granular,
    sample_rate: f32,
    frequency: f32,
    source: Vec<f32>,
    grains: [Grain; MAX_GRAINS],
    /// Samples since the last grain started.
    since_spawn: f32,
    env: GateEnvelope,
    rng: rand::rngs::ThreadRng,
}

impl GranularVoice {
    pub fn new(cfg: &Granular, frequency: f32, sample_rate: f32) -> Self {
        let mut rng = rand::rng();
        let source = source_cycle(cfg.source, &mut rng);
        Self {
            cfg: cfg.clone(),
            sample_rate,
            frequency,
            source,
            grains: [Grain::default(); MAX_GRAINS],
            // Start with a grain due immediately so the attack is not delayed.
            since_spawn: f32::MAX,
            env: GateEnvelope::new(&cfg.env, sample_rate),
            rng,
        }
    }

    #[inline]
    fn read_source(&self, pos: f32) -> f32 {
        let p = pos.rem_euclid(1.0) * SOURCE_SAMPLES as f32;
        let i0 = p as usize % SOURCE_SAMPLES;
        let i1 = (i0 + 1) % SOURCE_SAMPLES;
        let frac = p - p.floor();
        self.source[i0] * (1.0 - frac) + self.source[i1] * frac
    }

    fn spawn(&mut self, pitch_ratio: f32) {
        let Some(slot) = self.grains.iter().position(|g| !g.active) else {
            return;
        };
        let grain_samples = (self.cfg.grain_ms.max(1.0) * 0.001 * self.sample_rate).max(1.0);
        let semitone_ratio = 2f32.powf(self.cfg.pitch_semitones / 12.0);
        let pan = 0.5 + (self.rng.random::<f32>() - 0.5) * self.cfg.stereo_width;
        self.grains[slot] = Grain {
            active: true,
            pos: self.cfg.randomness * self.rng.random::<f32>(),
            pos_inc: self.frequency * semitone_ratio * pitch_ratio / self.sample_rate,
            life: 0.0,
            life_inc: 1.0 / grain_samples,
            left_gain: (pan * FRAC_PI_2).cos(),
            right_gain: (pan * FRAC_PI_2).sin(),
        };
    }
}

impl Voice for GranularVoice {
    fn gate_off(&mut self) {
        self.env.gate_off();
    }

    fn is_active(&self) -> bool {
        self.env.is_active()
    }

    #[inline]
    fn tick(&mut self, mods: &Modulation) -> (f32, f32) {
        let env = self.env.next();
        if !self.env.is_active() {
            return (0.0, 0.0);
        }
        let density = (self.cfg.density * mods.grain_density_ratio).max(0.01);
        let interval = self.sample_rate / density;
        if self.since_spawn >= interval {
            self.since_spawn = 0.0;
            self.spawn(mods.pitch_ratio);
        }
        self.since_spawn += 1.0;

        let (mut l, mut r) = (0.0f32, 0.0f32);
        let mut active = 0usize;
        for g in &mut self.grains {
            if !g.active {
                continue;
            }
            let window = 0.5 * (1.0 - (TAU * g.life).cos());
            let s = {
                let p = g.pos.rem_euclid(1.0) * SOURCE_SAMPLES as f32;
                let i0 = p as usize % SOURCE_SAMPLES;
                let i1 = (i0 + 1) % SOURCE_SAMPLES;
                let frac = p - p.floor();
                self.source[i0] * (1.0 - frac) + self.source[i1] * frac
            } * window;
            l += s * g.left_gain;
            r += s * g.right_gain;
            active += 1;
            g.pos += g.pos_inc;
            g.life += g.life_inc;
            if g.life >= 1.0 {
                g.active = false;
            }
        }
        if active > 1 {
            let norm = 1.0 / (active as f32).sqrt();
            l *= norm;
            r *= norm;
        }
        let gain = env * self.cfg.level * mods.amplitude;
        (l * gain, r * gain)
    }
}
```

(The inline source read inside `tick` duplicates `read_source` to avoid a borrow conflict with `&mut self.grains`; keep `read_source` only if something else uses it, otherwise delete it.) Note `stereo_width` 0 gives `pan` 0.5 → `cos(π/4) = sin(π/4)`, so left equals right exactly, which the test relies on.

- [ ] **Step 5: Run the tests**

```bash
cargo test granular 2>&1 | tail -30 && cargo test 2>&1 | grep -E "^test result"
```

Expected: all pass. If `higher_density_is_louder...` is marginal, the sqrt normalisation may flatten it: compare `rms` of the whole buffer (sparse grains leave gaps) rather than changing the ratio thresholds; report measured values.

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/patch.rs src/expressive/engines
git commit -m "feat(engines): granular voice with four sources, grain cloud and stereo width"
```

---

### Task 4: Granular in the renderer and the pad, texture and drone patches

**Files:**
- Modify: `src/expressive/render.rs`, `src/expressive/patch.rs` (`BUILTIN_PATCHES`), `src/demos.rs`
- Create: `src/expressive/patches/grain_cloud.json`, `formant_texture.json`, `noise_texture.json`, `drone.json`

- [ ] **Step 1: Write the failing tests**

`render.rs` tests:

```rust
    #[test]
    fn granular_engine_renders_true_stereo() {
        let p = patch(json!({"name": "g", "granular": {"stereo_width": 1.0, "randomness": 0.5,
            "density": 30, "env": {"attack": 0.001, "release": 0.2}}}));
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
        let mk = |lfo: serde_json::Value| patch(json!({"name": "g", "granular": {"density": 4, "grain_ms": 20,
            "randomness": 0.0, "stereo_width": 0.0, "env": {"attack": 0.001, "release": 0.01}}, "lfo": lfo}));
        let steady = mk(json!({"target": "off"}));
        let pumped = mk(json!({"rate": 0.5, "depth": 1.0, "wave": "square", "target": "grain_density"}));
        let a = left(&render_patch(&steady, &[note(0.0, 2.0, 220.0)], SR));
        let b = left(&render_patch(&pumped, &[note(0.0, 2.0, 220.0)], SR));
        // Square LFO at 0.5 Hz: first second at 2x density, second second at 0.5x.
        let first = rms(&b[..44100]);
        let second = rms(&b[44100..88200]);
        assert!(first > second * 1.3, "denser first half: {first} vs {second}");
        let sa = rms(&a[..44100]);
        let sb = rms(&a[44100..88200]);
        assert!((sa / sb - 1.0).abs() < 0.3, "steady without the LFO: {sa} vs {sb}");
    }
```

`patch.rs` library test: add `assert!(lib.get("grain_cloud").is_some() && lib.get("drone").is_some()); assert!(lib.count() >= 43);`.

- [ ] **Step 2: Run to verify failure**

`cargo test render:: patch:: 2>&1 | tail`. Expected: granular render tests fail (no voice spawned), library test fails.

- [ ] **Step 3: Implement**

`render.rs`: import `GranularVoice` and add after the wavetable spawn (before percussion):

```rust
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
```

Patch files:

`grain_cloud.json`
```json
{
  "name": "grain_cloud",
  "description": "Shimmering granular cloud: harmonic grains a fifth up, wide and slowly breathing",
  "category": "pad",
  "level": 0.6,
  "granular": {"source": "harmonics", "grain_ms": 120, "density": 15, "pitch_semitones": 7,
               "randomness": 0.7, "stereo_width": 0.9,
               "env": {"attack": 1.5, "decay": 1.0, "sustain": 0.7, "release": 3.0}},
  "lfo": {"rate": 0.2, "depth": 0.4, "wave": "sine", "target": "grain_density"},
  "effects": [{"type": "reverb", "room_size": 0.9, "dampening": 0.3, "wet_level": 0.5, "intensity": 0.5}]
}
```

`formant_texture.json`: category `pad`, level 0.6, `{"source": "formant", "grain_ms": 80, "density": 25, "randomness": 0.4, "stereo_width": 0.7, "env": {"attack": 0.8, "decay": 0.5, "sustain": 0.8, "release": 2.0}}`, `lfo` `{"rate": 0.15, "depth": 0.5, "wave": "triangle", "target": "amplitude"}`, effects chorus (rate 0.3, depth 0.5, intensity 0.3) + reverb (room_size 0.8, dampening 0.4, wet_level 0.4, intensity 0.4).

`noise_texture.json`: category `fx`, level 0.5, `{"source": "noise", "grain_ms": 30, "density": 40, "randomness": 1.0, "stereo_width": 1.0, "env": {"attack": 0.5, "decay": 0.5, "sustain": 0.6, "release": 1.5}}`, `lfo` `{"rate": 0.3, "depth": 0.6, "wave": "sample_hold", "target": "grain_density"}`, effects filter (`filter_type` low_pass, cutoff 4000, resonance 0.3, intensity 1.0) + reverb (room_size 0.7, intensity 0.3).

`drone.json`: category `pad`, level 0.6, `subtractive` `{"osc1": {"wave": "saw"}, "osc2": {"wave": "square", "mix": 0.35, "detune_cents": 6, "octave": -1}, "filter": {"type": "low_pass", "cutoff": 500, "resonance": 0.4, "slope": 24, "env_amount": 0.3, "env": {"attack": 4.0, "decay": 4.0, "sustain": 0.5, "release": 4.0}}, "env": {"attack": 2.0, "decay": 1.0, "sustain": 0.9, "release": 4.0}}`, `lfo` `{"rate": 0.1, "depth": 0.5, "wave": "triangle", "target": "cutoff"}`, effects reverb (room_size 0.9, dampening 0.5, wet_level 0.4, intensity 0.4).

Add the four `include_str!` lines under `// granular / lfo` in `BUILTIN_PATCHES`. Demos: no change needed (pads and fx phrases exist).

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: all pass, including the library headroom test for the four new files (lower a `level` if a file peaks above 1.6 and say so).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/expressive/render.rs src/expressive/patch.rs src/expressive/patches
git commit -m "feat: granular engine in the renderer with grain_cloud, formant_texture, noise_texture and drone patches"
```

---

### Task 5: MCP schema, descriptions, integration tests

**Files:**
- Modify: `src/server/mcp.rs`, `tests/integration/mcp_protocol.rs`

- [ ] **Step 1: Write the failing tests**

`mcp.rs` tests module:

```rust
    #[test]
    fn patch_schema_describes_lfo_and_granular_and_they_validate_through_define_synth() {
        let schema = patch_schema();
        let lfo = &schema["properties"]["lfo"]["properties"];
        assert_eq!(lfo["wave"]["enum"], json!(["sine", "triangle", "saw", "square", "sample_hold"]));
        assert_eq!(lfo["target"]["enum"], json!(["off", "cutoff", "pitch", "amplitude", "morph", "grain_density"]));
        let g = &schema["properties"]["granular"]["properties"];
        assert_eq!(g["source"]["enum"], json!(["harmonics", "noise", "formant", "inharmonic"]));
        for field in ["grain_ms", "density", "pitch_semitones", "randomness", "stereo_width", "env", "level"] {
            assert!(g[field].is_object(), "{field}");
        }

        let mut state = ServerState::new();
        let r = call(&mut state, "define_synth", json!({"name": "cloud",
            "granular": {"source": "formant", "stereo_width": 0.9},
            "lfo": {"rate": 0.2, "depth": 0.4, "target": "grain_density"}}));
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(text(&r).contains("granular") && text(&r).contains("lfo"), "{}", text(&r));
        let r = call(&mut state, "define_synth", json!({"name": "bad", "granular": {"density": 500}}));
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("granular.density"));
        let r = call(&mut state, "define_synth", json!({"name": "bad2", "subtractive": {}, "lfo": {"rate": 99}}));
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("lfo.rate"));
    }
```

`tests/integration/mcp_protocol.rs`:

```rust
#[test]
fn granular_and_lfo_patches_play_by_name_and_inline() {
    let mut server = TestServer::start();
    for name in ["grain_cloud", "drone", "noise_texture"] {
        let r = server.call(json!({
            "jsonrpc": "2.0", "id": 40, "method": "tools/call",
            "params": {"name": "play_notes", "arguments": {"notes": [
                {"synth": name, "note": 57, "start_time": 0.0, "duration": 0.3}]}}
        }));
        assert!(r["error"].is_null(), "{name}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Playback started") || text.contains("Audio output unavailable"), "{name}: {text}");
    }
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 41, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "wob", "subtractive": {"filter": {"cutoff": 600}},
                       "lfo": {"rate": 4, "depth": 0.8, "wave": "sine", "target": "cutoff"}},
             "note": 45, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 42, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "synths"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    for needle in ["grain_cloud", "formant_texture", "noise_texture", "drone", "granular", "lfo"] {
        assert!(text.contains(needle), "catalog missing {needle}");
    }
}

#[test]
fn lfo_and_granular_errors_name_the_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 43, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "subtractive": {}, "lfo": {"target": "resonance"}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("resonance"));
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 44, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "granular": {"grain_size": 0.1}}, "note": 60, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("grain_size"));
}
```

- [ ] **Step 2: Run to verify failure**

`cargo test server::mcp 2>&1 | tail`. Expected: schema test fails.

- [ ] **Step 3: Implement**

In `patch_schema()`, after `"wavetable"` and before `"percussion"`:

```rust
            "granular": {
                "type": "object",
                "description": "A cloud of short Hann-windowed grains read from a one-cycle source at the note's pitch; the first engine with true stereo. Long grains + low density = smooth; short grains + high density = dense texture.",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "source": {"type": "string", "enum": ["harmonics", "noise", "formant", "inharmonic"], "default": "harmonics",
                        "description": "harmonics: warm; noise: pitched buzz; formant: vowel-like; inharmonic: bell-like"},
                    "grain_ms": {"type": "number", "minimum": 5, "maximum": 500, "default": 50, "description": "Grain length in ms"},
                    "density": {"type": "number", "minimum": 1, "maximum": 50, "default": 10, "description": "Grains started per second"},
                    "pitch_semitones": {"type": "number", "minimum": -24, "maximum": 24, "default": 0},
                    "randomness": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.2, "description": "Random grain start position; higher = more chaotic"},
                    "stereo_width": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "Random stereo placement per grain"},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
```

and after `"percussion"`, before `"effects"`:

```rust
            "lfo": {
                "type": "object",
                "description": "One free-running LFO per patch routed to a single target. Depth 1 = cutoff ±2 octaves, pitch ±2 semitones, amplitude down to silence, morph ±0.5, grain density 0.5x-2x.",
                "properties": {
                    "rate": {"type": "number", "minimum": 0.1, "maximum": 20, "default": 1, "description": "Hz"},
                    "depth": {"type": "number", "minimum": 0, "maximum": 1, "default": 0},
                    "wave": {"type": "string", "enum": ["sine", "triangle", "saw", "square", "sample_hold"], "default": "sine"},
                    "target": {"type": "string", "enum": ["off", "cutoff", "pitch", "amplitude", "morph", "grain_density"], "default": "off",
                        "description": "cutoff: subtractive filter; pitch: every pitched engine (vibrato); amplitude: tremolo; morph: wavetable; grain_density: granular"}
                },
                "additionalProperties": false
            },
```

Update the schema's top-level description to list five engines and mention `lfo`. `define_synth` description: engines sentence adds "granular (grain cloud with stereo width)" and "an optional lfo (cutoff/pitch/amplitude/morph/grain_density)"; add one example line:

```
- Texture: {\"name\": \"cloud\", \"category\": \"pad\", \"level\": 0.6, \"granular\": {\"source\": \"formant\", \"grain_ms\": 120, \"density\": 15, \"pitch_semitones\": 7, \"randomness\": 0.7, \"stereo_width\": 0.9, \"env\": {\"attack\": 1.5, \"release\": 3}}, \"lfo\": {\"rate\": 0.2, \"depth\": 0.4, \"target\": \"grain_density\"}, \"effects\": [{\"type\": \"reverb\", \"room_size\": 0.9, \"intensity\": 0.5}]}
```

`handle_define_synth`: push `"granular"` when its level > 0, and after the engines list append ` + lfo → <target>` to the details when `patch.lfo.as_ref().is_some_and(|l| l.is_active())` (use `LfoTarget::as_str`). `handle_list_sounds` engine line: add `granular (sources {sources})` built from `GrainSource::ALL` and a second sentence `LFO targets: {targets}` from `LfoTarget::ALL`, waves from `LfoWave::ALL`.

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/server/mcp.rs tests/integration/mcp_protocol.rs
git commit -m "feat(mcp): granular engine and lfo in the patch schema, descriptions and catalog"
```

---

### Task 6: Docs, spec status, final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `examples/api_reference.md`, `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md`

- [ ] **Step 1: README**

In "### Engines" add after wavetable:

```markdown
- **granular**: `source` `harmonics|noise|formant|inharmonic`, `grain_ms` 5-500, `density` 1-50 grains/s, `pitch_semitones` ±24, `randomness`, `stereo_width`, `env`. True stereo.
- **lfo** (not an engine, one per patch): `rate` 0.1-20 Hz, `depth` 0-1, `wave` `sine|triangle|saw|square|sample_hold`, `target` `cutoff|pitch|amplitude|morph|grain_density`.
```

Add the `cloud` example from the tool description under "Define your own", and update the built-in count (43) wherever it appears.

- [ ] **Step 2: CLAUDE.md**

Synthesis bullets: engines list gains granular and the LFO; `lfo.rs` bullet: "`lfo.rs` - five-shape LFO; `render_patch` runs one per patch and maps it onto `Modulation` (`engines/mod.rs`) each sample: cutoff, pitch, amplitude, wavetable morph, grain density."; patches count 43; pipeline step 2 mentions the LFO.

- [ ] **Step 3: examples/api_reference.md**

Add `granular` and `lfo` rows with the README one-liners.

- [ ] **Step 4: Spec**

`Status:` → `PRs 1-3 implemented; PR 4 pending`. §1 `lfo`: add "Amplitude depth `d` maps to `1 - d (1 - v) / 2`; morph adds `v d / 2` to the patch's morph and clamps; grain density multiplies by `2^(v d)`." §1 `granular`: add "Each voice builds one peak-normalised source cycle at note-on (`noise` is a fresh random cycle); up to 32 grains overlap, summed with 1/sqrt(active) normalisation."

- [ ] **Step 5: Verify and commit**

```bash
rg -n "39 built-in|four engines" README.md CLAUDE.md examples || echo "no stale counts"
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | grep -E "^test result"
git add README.md CLAUDE.md examples docs/superpowers/specs
git commit -m "docs: granular engine and lfo"
```

- [ ] **Step 6: Open the PR**

Use the `superpowers:finishing-a-development-branch` skill. Title: `feat: granular synth engine and per-patch lfo`. Body: spec and plan links; the four new patches and `warm_pad`'s new LFO; the depth mappings; follow-ups (PR 4 Time Fracture; issue #109 headroom).

---

## Self-review notes

- Spec coverage for PR 3: §1 `lfo` (Tasks 1, 2, 5), §1 `granular` (Tasks 3, 4, 5), §3 step 3 "read the LFO once per sample" (Task 2), §4 granular and LFO rows (Tasks 2, 3), §5 pad/texture/drone (Task 4), §7 "LFO on pitch: zero-crossing rate over successive windows" and "granular stereo_width" tests (Tasks 2, 3, 4), §8 PR 3 (this plan).
- Names used consistently: `LfoWave::{Sine, Triangle, Saw, Square, SampleHold}` + `ALL`/`as_str`; `LfoTarget::{Off, Cutoff, Pitch, Amplitude, Morph, GrainDensity}` + `ALL`/`as_str`; `LfoConfig { rate, depth, wave, target }` + `is_active`; `Lfo::new(rate, wave, sample_rate)` / `next()`; `Modulation { pitch_ratio, cutoff_ratio, amplitude, morph_offset, grain_density_ratio }` + `from_lfo`; `GrainSource::{Harmonics, Noise, Formant, Inharmonic}` + `ALL`/`as_str`; `Granular { level, source, grain_ms, density, pitch_semitones, randomness, stereo_width, env }`; `GranularVoice::new(&Granular, f32, f32)`; `source_cycle`, `MAX_GRAINS`, `SOURCE_SAMPLES`.
- Deferred by design: a second LFO, velocity modulation, LFO retrigger per note (the spec says free-running per patch), and the headroom work in issue #109.
