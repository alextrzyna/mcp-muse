# Agent-Defined Synths, PR 4 (Time Fracture Delay and Docs Pass) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the `delay` effect tryx-fx's Time Fracture behaviour (beat-synced random delay time and pitch-shifted repeats), make tempo reach the effects so beat values and `sync_tempo` work, size render tails to the delay, and close the documentation pass for the whole synth-patch project.

**Architecture:** The `EffectType::Delay` variant grows four optional fields with serde defaults; validation lives with the other effect checks in `src/midi/mod.rs`. `effects.rs`'s `Delay` node gains a tempo-aware constructor and a per-sample "fracture" path (cosine-interpolated random delay between two beat values, a pitch accumulator that drifts the read position for the shifted repeats), while the static path stays bit-identical. Tempo flows through `EffectsChain::with_tempo`; `render_patch` and the R2D2 chain get it from the sequence, and the MIDI bus gets it via a new `PlayCommand.tempo`. Effect tails become per-effect (`EffectConfig::tail_seconds`) so long delays are not cut.

**Tech Stack:** Rust 2024, serde/serde_json, rand 0.10 (`rand::rng()`, `RngExt::random`), existing `DelayLine::read_fractional`, `test_util` measurements.

**Spec:** `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md` (§1 `effects`, §3, §4 effects row, §7 Time Fracture tests, §8 PR 4).

## Global Constraints

- DSP is verified by measurement (`src/expressive/test_util.rs`: `goertzel_power`, `rms`, `zero_crossing_rate`, `db`), never by ear.
- Never allocate or log inside per-sample loops on the audio thread (`src/midi/engine.rs`). `set_bus_effects` builds chains once per play command; that stays the only allocation.
- Spec §1 `effects`: `delay` gains optional `random_beats: [min, max]` (0 to 4 beats, replaces `delay_time` when present), `random_rate` Hz (0 = static), `pitch_intervals` (up to 12 semitone values, -12 to 12), `pitch_mode`: `random | up | down | up_down`. Beat values use the sequence tempo.
- Spec §7: with `random_rate` 0 the delay time is fixed; with `pitch_intervals: [12]` the repeat is one octave up (zero-crossing rate doubles).
- Effect JSON stays flat snake_case; unknown effect fields are rejected as today; validation failures return `-32602` naming the field and range.
- The existing static delay (no Time Fracture fields, `sync_tempo` false) must produce bit-identical output to before.
- `cargo clippy --all-targets --all-features -- -D warnings` (CI's exact flags on the latest stable, `rustup update stable` first) and `cargo fmt --check` pass before every commit.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jxcj13gt2pyZbmDvvC1Pep
  ```
- Work in the worktree `.claude/worktrees/feat-synth-time-fracture` (branch `worktree-feat-synth-time-fracture`, from `main` f2fed6d).

---

## File map

| File | Responsibility |
|---|---|
| `src/midi/mod.rs` | `PitchMode`; new `Delay` fields; validation; `EffectConfig::tail_seconds` |
| `src/expressive/effects_presets.rs`, `src/midi/translate.rs` (tests) | delay literals gain the new fields |
| `src/expressive/effects.rs` | tempo-aware `Delay` with the fracture path; `EffectNode::from_config(sample_rate, tempo, cfg)`; `EffectsChain::with_tempo` |
| `src/midi/engine.rs` | `PlayCommand.tempo`; bus chain built with it |
| `src/midi/translate.rs` | passes the sequence tempo to every chain |
| `src/expressive/render.rs` | `render_patch(patch, notes, sample_rate, tempo)`; per-effect tails |
| `src/expressive/patches/shimmer_keys.json` | showcase patch |
| `src/demos.rs` | `test-effects` adds a Time Fracture pass |
| `src/server/mcp.rs` | effects schema and catalog text |
| `tests/integration/mcp_protocol.rs` | end-to-end |
| `README.md`, `CLAUDE.md`, `examples/api_reference.md`, spec | docs pass |

---

### Task 1: Delay data model, validation and tails

**Files:**
- Modify: `src/midi/mod.rs`, `src/expressive/effects_presets.rs`, `src/midi/translate.rs` (test literal only if it uses a struct literal; the existing one uses `json!`, so probably nothing), `src/expressive/effects.rs` (only the `from_config` pattern must name the new fields to compile; DSP comes in Task 2)

**Interfaces:**
- Produces:
  ```rust
  #[serde(rename_all = "snake_case")] pub enum PitchMode { Random, Up, Down, UpDown }   // Default = Random; ALL; as_str
  EffectType::Delay {
      delay_time: f32, feedback: f32, wet_level: f32, sync_tempo: bool,
      random_beats: Option<[f32; 2]>,     // #[serde(default)]
      random_rate: f32,                   // #[serde(default)] 0.0
      pitch_intervals: Vec<f32>,          // #[serde(default)]
      pitch_mode: PitchMode,              // #[serde(default)]
  }
  impl EffectConfig { pub fn tail_seconds(&self, tempo: u32) -> f32 }
  ```
  `tail_seconds`: disabled → 0; `Reverb` → 1.0; `Delay` → `4 × longest delay in seconds + 0.5` where the longest delay is `random_beats[1]` beats (or `delay_time` beats when `sync_tempo`, else `delay_time` seconds), min 1.0; every other effect → 0.5.

- [ ] **Step 1: Write the failing tests**

In the tests module of `src/midi/mod.rs` (next to `effect_config_accepts_flat_snake_case`):

```rust
    #[test]
    fn delay_accepts_time_fracture_fields_with_defaults() {
        let e: EffectConfig = serde_json::from_str(r#"{"type": "delay"}"#).unwrap();
        match &e.effect {
            EffectType::Delay {
                random_beats,
                random_rate,
                pitch_intervals,
                pitch_mode,
                ..
            } => {
                assert!(random_beats.is_none());
                assert_eq!(*random_rate, 0.0);
                assert!(pitch_intervals.is_empty());
                assert_eq!(*pitch_mode, PitchMode::Random);
            }
            other => panic!("not a delay: {other:?}"),
        }
        let e: EffectConfig = serde_json::from_str(
            r#"{"type": "delay", "random_beats": [0.25, 1.0], "random_rate": 2.0,
                "pitch_intervals": [7, 12], "pitch_mode": "up_down", "feedback": 0.5}"#,
        )
        .unwrap();
        assert!(e.validate_effect_config().is_ok());
        match &e.effect {
            EffectType::Delay {
                random_beats,
                pitch_intervals,
                pitch_mode,
                ..
            } => {
                assert_eq!(*random_beats, Some([0.25, 1.0]));
                assert_eq!(pitch_intervals, &vec![7.0, 12.0]);
                assert_eq!(*pitch_mode, PitchMode::UpDown);
            }
            other => panic!("not a delay: {other:?}"),
        }
        assert!(serde_json::from_str::<EffectConfig>(r#"{"type": "delay", "pitch_mode": "spiral"}"#).is_err());
        assert!(serde_json::from_str::<EffectConfig>(r#"{"type": "delay", "min_beats": 1}"#).is_err());
    }

    #[test]
    fn time_fracture_validation_names_the_field_and_range() {
        let cases = [
            (r#"{"type": "delay", "random_beats": [1.0, 5.0]}"#, "random_beats", "4"),
            (r#"{"type": "delay", "random_beats": [2.0, 1.0]}"#, "random_beats", "min"),
            (r#"{"type": "delay", "random_rate": 50}"#, "random_rate", "10"),
            (r#"{"type": "delay", "pitch_intervals": [30]}"#, "pitch_intervals", "12"),
            (r#"{"type": "delay", "pitch_intervals": [1,2,3,4,5,6,7,8,9,10,11,12,13]}"#, "pitch_intervals", "12"),
        ];
        for (json, field, needle) in cases {
            let e: EffectConfig = serde_json::from_str(json).unwrap();
            let err = e.validate_effect_config().unwrap_err();
            assert!(err.contains(field) && err.contains(needle), "{json}: {err}");
        }
    }

    #[test]
    fn effect_tail_seconds_follows_the_longest_delay() {
        let parse = |s: &str| serde_json::from_str::<EffectConfig>(s).unwrap();
        assert_eq!(parse(r#"{"type": "reverb"}"#).tail_seconds(120), 1.0);
        assert_eq!(parse(r#"{"type": "chorus"}"#).tail_seconds(120), 0.5);
        // 0.25 s static delay: 4 x 0.25 + 0.5 = 1.5
        assert!((parse(r#"{"type": "delay", "delay_time": 0.25}"#).tail_seconds(120) - 1.5).abs() < 1e-6);
        // 2 beats at 60 BPM = 2 s: 4 x 2 + 0.5 = 8.5
        assert!((parse(r#"{"type": "delay", "random_beats": [0.5, 2.0]}"#).tail_seconds(60) - 8.5).abs() < 1e-6);
        // sync_tempo: delay_time is in beats: 1 beat at 120 = 0.5 s -> 2.5
        assert!((parse(r#"{"type": "delay", "delay_time": 1.0, "sync_tempo": true}"#).tail_seconds(120) - 2.5).abs() < 1e-6);
        let mut off = parse(r#"{"type": "reverb"}"#);
        off.enabled = false;
        assert_eq!(off.tail_seconds(120), 0.0);
    }
```

`validate_effect_config` is whatever name the per-effect validator has in `mod.rs` today (`validate_single_effect` is a method on `SimpleNote`); if there is no standalone method on `EffectConfig`, add `pub fn validate_effect_config(&self) -> Result<(), String>` that wraps the existing per-effect checks and have `SimpleNote::validate_single_effect` call it, so both the note path and `Patch::validate` (which currently does not validate effects; see Step 3) share one implementation.

- [ ] **Step 2: Run to verify failure**

`cargo test delay_accepts time_fracture effect_tail 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement**

In `src/midi/mod.rs`:

```rust
/// Order in which Time Fracture picks the next pitch interval for a repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchMode {
    #[default]
    Random,
    Up,
    Down,
    UpDown,
}

impl PitchMode {
    pub const ALL: [PitchMode; 4] = [PitchMode::Random, PitchMode::Up, PitchMode::Down, PitchMode::UpDown];

    pub fn as_str(&self) -> &'static str {
        match self {
            PitchMode::Random => "random",
            PitchMode::Up => "up",
            PitchMode::Down => "down",
            PitchMode::UpDown => "up_down",
        }
    }
}
```

Extend the `Delay` variant (keep the existing four fields and their attributes):

```rust
        /// Time Fracture: random delay time between `[min, max]` beats of the
        /// sequence tempo; replaces `delay_time` when present.
        #[serde(default)]
        random_beats: Option<[f32; 2]>,
        /// How fast the random delay time moves, in Hz (0 = fixed at `min`).
        #[serde(default)]
        random_rate: f32,
        /// Semitone shifts applied to successive repeats (up to 12 values).
        #[serde(default)]
        pitch_intervals: Vec<f32>,
        /// Order the intervals are visited in.
        #[serde(default)]
        pitch_mode: PitchMode,
```

Validation (inside the existing `Delay` arm, after the `wet_level` check):

```rust
                if let Some([min, max]) = random_beats {
                    for (i, v) in [min, max].into_iter().enumerate() {
                        if !(0.0..=4.0).contains(v) {
                            return Err(format!("Delay random_beats[{i}] {v} is out of range (0-4 beats)"));
                        }
                    }
                    if min > max {
                        return Err(format!("Delay random_beats min {min} must not exceed max {max}"));
                    }
                }
                if !(0.0..=10.0).contains(random_rate) {
                    return Err(format!("Delay random_rate {random_rate} is out of range (0-10 Hz)"));
                }
                if pitch_intervals.len() > 12 {
                    return Err(format!("Delay pitch_intervals has {} values; at most 12", pitch_intervals.len()));
                }
                for (i, s) in pitch_intervals.iter().enumerate() {
                    if !(-12.0..=12.0).contains(s) {
                        return Err(format!("Delay pitch_intervals[{i}] {s} is out of range (-12 to 12 semitones)"));
                    }
                }
```

`tail_seconds`:

```rust
impl EffectConfig {
    /// Seconds of silence to render after the last note so this effect can ring out.
    pub fn tail_seconds(&self, tempo: u32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        match &self.effect {
            EffectType::Reverb { .. } => 1.0,
            EffectType::Delay {
                delay_time,
                sync_tempo,
                random_beats,
                ..
            } => {
                let longest = match random_beats {
                    Some([_, max]) => max * seconds_per_beat,
                    None if *sync_tempo => delay_time * seconds_per_beat,
                    None => *delay_time,
                };
                (4.0 * longest + 0.5).max(1.0)
            }
            _ => 0.5,
        }
    }
}
```

Update the four struct literals in `src/expressive/effects_presets.rs` (add `random_beats: None, random_rate: 0.0, pitch_intervals: Vec::new(), pitch_mode: PitchMode::Random`) and the two match patterns in `src/expressive/effects.rs` and `src/midi/mod.rs` that destructure `Delay` (`sync_tempo: _,` becomes `sync_tempo: _, random_beats: _, random_rate: _, pitch_intervals: _, pitch_mode: _,` in `effects.rs` for now; Task 2 uses them). Also make `Patch::validate` in `src/expressive/patch.rs` call `e.validate_effect_config()` for each entry in `patch.effects` with the field path `effects[i]` prefixed, so an invalid Time Fracture field on a patch is `-32602` too.

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

Expected: all pass (baseline 171 + 36, plus 3).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/midi/mod.rs src/expressive/effects_presets.rs src/expressive/effects.rs src/expressive/patch.rs
git commit -m "feat(effects): time fracture fields on delay with validation and per-effect tails"
```

---

### Task 2: Time Fracture DSP and tempo-aware chains

**Files:**
- Modify: `src/expressive/effects.rs`

**Interfaces:**
- Consumes: `PitchMode`, the new `Delay` fields (Task 1), `DelayLine::read_fractional`.
- Produces:
  ```rust
  pub const DEFAULT_TEMPO: u32 = 120;
  impl Delay { pub fn new(sample_rate, delay_time, feedback, wet_level, intensity) -> Self /* unchanged static */;
               pub fn time_fracture(sample_rate: f32, tempo: u32, min_beats: f32, max_beats: f32, random_rate: f32,
                                    pitch_intervals: &[f32], pitch_mode: PitchMode, feedback: f32, wet_level: f32, intensity: f32) -> Self; }
  impl EffectNode { pub fn from_config(sample_rate: f32, tempo: u32, config: &EffectConfig) -> Self }
  impl EffectsChain { pub fn new(sample_rate, configs) -> Self /* tempo DEFAULT_TEMPO */; pub fn with_tempo(sample_rate: f32, tempo: u32, configs: &[EffectConfig]) -> Self }
  ```
  `from_config` for `Delay`: if `random_beats` is `Some` or `pitch_intervals` is non-empty → `time_fracture` (with `random_beats` defaulting to `[t, t]` where `t` is `delay_time` in beats if `sync_tempo` else `delay_time / seconds_per_beat`); else if `sync_tempo` → static `Delay::new` with `delay_time * seconds_per_beat`; else static as today.

- [ ] **Step 1: Write the failing tests**

In `src/expressive/effects.rs` tests:

```rust
    fn impulse_response(d: &mut Delay, len: usize) -> Vec<f32> {
        (0..len).map(|i| d.process(if i == 0 { 1.0 } else { 0.0 })).collect()
    }

    fn first_echo_index(out: &[f32]) -> usize {
        out.iter().enumerate().skip(1).find(|(_, &x)| x.abs() > 0.05).map(|(i, _)| i).unwrap()
    }

    #[test]
    fn time_fracture_with_zero_rate_is_a_fixed_delay_at_min_beats() {
        // 0.5 beats at 120 BPM = 0.25 s = 11025 samples.
        let mut d = Delay::time_fracture(SR, 120, 0.5, 2.0, 0.0, &[], PitchMode::Random, 0.0, 1.0, 1.0);
        let out = impulse_response(&mut d, 30000);
        let echo = first_echo_index(&out);
        assert!((echo as i64 - 11025).abs() <= 2, "echo at {echo}");
        assert!(out[12000..].iter().all(|x| x.abs() < 1e-3), "no second echo without feedback");
    }

    #[test]
    fn sync_tempo_puts_delay_time_in_beats() {
        let cfg: EffectConfig = serde_json::from_str(
            r#"{"type": "delay", "delay_time": 1.0, "sync_tempo": true, "feedback": 0.0, "wet_level": 1.0, "intensity": 1.0}"#,
        )
        .unwrap();
        let mut chain = EffectsChain::with_tempo(SR, 60, std::slice::from_ref(&cfg));
        let out: Vec<f32> = (0..60000).map(|i| chain.process(if i == 0 { 1.0 } else { 0.0 })).collect();
        assert!((first_echo_index(&out) as i64 - 44100).abs() <= 2, "1 beat at 60 BPM is one second");
    }

    #[test]
    fn static_delay_output_is_unchanged_by_the_tempo_plumbing() {
        let cfg: EffectConfig =
            serde_json::from_str(r#"{"type": "delay", "delay_time": 0.1, "feedback": 0.3, "intensity": 0.6}"#).unwrap();
        let mut a = EffectsChain::new(SR, std::slice::from_ref(&cfg));
        let mut b = EffectsChain::with_tempo(SR, 97, std::slice::from_ref(&cfg));
        let mut reference = Delay::new(SR, 0.1, 0.3, 0.3, 0.6);
        for i in 0..10000 {
            let x = if i % 500 == 0 { 0.8 } else { 0.0 };
            let (ya, yb, yr) = (a.process(x), b.process(x), reference.process(x));
            assert_eq!(ya, yb);
            assert_eq!(ya, yr);
        }
    }

    #[test]
    fn random_rate_moves_the_delay_time_within_the_beat_range() {
        let mut d = Delay::time_fracture(SR, 120, 0.25, 1.0, 3.0, &[], PitchMode::Random, 0.0, 1.0, 1.0);
        let (min_s, max_s) = (0.125 * SR, 0.5 * SR);
        let mut seen_min = f32::MAX;
        let mut seen_max = 0.0f32;
        for _ in 0..(2.0 * SR) as usize {
            d.process(0.0);
            let cur = d.current_delay_samples();
            assert!(cur >= min_s - 1.0 && cur <= max_s + 1.0, "{cur} outside [{min_s}, {max_s}]");
            seen_min = seen_min.min(cur);
            seen_max = seen_max.max(cur);
        }
        assert!(seen_max - seen_min > 0.1 * (max_s - min_s), "delay time actually moves");
    }

    #[test]
    fn pitch_interval_of_twelve_repeats_one_octave_up() {
        // Wet only, no feedback: after the 0.3 s input burst ends, the output is
        // the pitch-shifted repeat alone.
        let mut d = Delay::time_fracture(SR, 120, 1.0, 1.0, 0.0, &[12.0], PitchMode::Up, 0.0, 1.0, 1.0);
        let burst = sine(220.0, 0.3, SR, 0.8);
        let total = (1.2 * SR) as usize;
        let out: Vec<f32> = (0..total)
            .map(|i| d.process(if i < burst.len() { burst[i] } else { 0.0 }))
            .collect();
        // Delay is 0.5 s (1 beat at 120); the shifted repeat plays the 0.3 s burst
        // at double speed, so it occupies roughly 0.5..0.65 s.
        let window = &out[(0.52 * SR) as usize..(0.62 * SR) as usize];
        assert!(rms(window) > 0.05, "repeat is audible");
        let zcr = zero_crossing_rate(window, SR);
        assert!((zcr - 880.0).abs() < 60.0, "an octave up: {zcr} crossings/s");
        let dry_window = &out[(0.05 * SR) as usize..(0.25 * SR) as usize];
        assert!(rms(dry_window) < 1e-3, "wet-only output is silent while the input plays");
    }

    #[test]
    fn pitch_modes_visit_intervals_in_order() {
        let mut up = Delay::time_fracture(SR, 120, 1.0, 1.0, 0.0, &[0.0, 12.0, -12.0], PitchMode::Up, 0.0, 1.0, 1.0);
        assert_eq!(up.next_pitch_semitones(), 0.0);
        assert_eq!(up.next_pitch_semitones(), 12.0);
        assert_eq!(up.next_pitch_semitones(), -12.0);
        assert_eq!(up.next_pitch_semitones(), 0.0);
        let mut down = Delay::time_fracture(SR, 120, 1.0, 1.0, 0.0, &[0.0, 12.0, -12.0], PitchMode::Down, 0.0, 1.0, 1.0);
        assert_eq!(down.next_pitch_semitones(), 0.0);
        assert_eq!(down.next_pitch_semitones(), -12.0);
        assert_eq!(down.next_pitch_semitones(), 12.0);
        let mut ud = Delay::time_fracture(SR, 120, 1.0, 1.0, 0.0, &[0.0, 5.0, 12.0], PitchMode::UpDown, 0.0, 1.0, 1.0);
        let seq: Vec<f32> = (0..6).map(|_| ud.next_pitch_semitones()).collect();
        assert_eq!(seq, vec![0.0, 5.0, 12.0, 5.0, 0.0, 5.0]);
        let mut random = Delay::time_fracture(SR, 120, 1.0, 1.0, 0.0, &[3.0, 7.0], PitchMode::Random, 0.0, 1.0, 1.0);
        for _ in 0..20 {
            let s = random.next_pitch_semitones();
            assert!(s == 3.0 || s == 7.0);
        }
    }

    #[test]
    fn time_fracture_with_feedback_stays_bounded() {
        let mut d = Delay::time_fracture(SR, 120, 0.25, 1.0, 4.0, &[7.0, 12.0], PitchMode::UpDown, 0.9, 1.0, 1.0);
        let noise = white_noise((3.0 * SR) as usize, 7);
        let mut peak = 0.0f32;
        for x in noise {
            let y = d.process(x * 0.5);
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        assert!(peak <= 1.0, "output is clamped: {peak}");
    }
```

Add `rms` and `zero_crossing_rate` to the test-module imports. `current_delay_samples()` and `next_pitch_semitones()` are `#[cfg(test)] pub(crate)` accessors on `Delay`.

- [ ] **Step 2: Run to verify failure**

`cargo test effects::tests 2>&1 | tail`. Expected: compile errors.

- [ ] **Step 3: Implement**

Replace the `Delay` struct and impl in `effects.rs`:

```rust
pub const DEFAULT_TEMPO: u32 = 120;

/// Longest delay line Time Fracture can address: 4 beats at 30 BPM.
const MAX_DELAY_SECONDS: f32 = 8.0;

/// Time Fracture state: a random delay time gliding between two bounds,
/// and a pitch accumulator that drifts the read position so each repeat
/// plays back transposed.
struct Fracture {
    min_samples: f32,
    max_samples: f32,
    /// Cycles per sample of the random-walk phase (0 = never moves).
    phase_inc: f32,
    phase: f32,
    last_random: f32,
    next_random: f32,
    intervals: Vec<f32>,
    mode: PitchMode,
    index: usize,
    going_up: bool,
    pitch_ratio: f32,
    pitch_accumulator: f32,
    /// xorshift32 state, seeded once from `rand::rng()`; keeps `Delay`
    /// `Clone + Debug` (a `ThreadRng` field would not) and allocation-free.
    rng: u32,
}

#[derive(Debug, Clone)]
pub struct Delay {
    line: DelayLine,
    feedback: f32,
    lp: f32,
    wet: f32,
    dry: f32,
    /// Fixed delay in samples for the static path.
    static_delay: f32,
    fracture: Option<Fracture>,
}
```

`Fracture` derives `Debug, Clone` like every other node (`EffectNode` and `EffectsChain` derive `Clone`, so a `ThreadRng` field is not an option; `Lfo` gets away without derives only because voices are never cloned). Its random source is a 32-bit xorshift:

```rust
impl Fracture {
    /// Uniform in [0, 1). xorshift32; the state is never zero because the
    /// seed is or-ed with 1.
    #[inline]
    fn next_unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}
```

The constructor seeds with `rand::rng().random::<u32>() | 1` (needs `use rand::RngExt;` as `lfo.rs` does); nothing else touches the `rand` crate.

```rust
impl Delay {
    /// Static delay: identical to the pre-Time-Fracture behaviour.
    pub fn new(sample_rate: f32, delay_time: f32, feedback: f32, wet_level: f32, intensity: f32) -> Self {
        let wet = (wet_level * intensity).clamp(0.0, 1.0);
        let samples = (delay_time.max(0.001) * sample_rate) as usize;
        Self {
            line: DelayLine::new(samples),
            feedback: (feedback * intensity).clamp(0.0, 0.95),
            lp: 0.0,
            wet,
            dry: 1.0 - wet * 0.5,
            static_delay: samples as f32,
            fracture: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn time_fracture(
        sample_rate: f32,
        tempo: u32,
        min_beats: f32,
        max_beats: f32,
        random_rate: f32,
        pitch_intervals: &[f32],
        pitch_mode: PitchMode,
        feedback: f32,
        wet_level: f32,
        intensity: f32,
    ) -> Self {
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        let to_samples = |beats: f32| (beats * seconds_per_beat * sample_rate).max(1.0);
        let (min_samples, max_samples) = (to_samples(min_beats), to_samples(max_beats).max(to_samples(min_beats)));
        let wet = (wet_level * intensity).clamp(0.0, 1.0);
        let mut seed_rng = rand::rng();
        // With no random motion the delay sits at `min`; with pitch shifting and
        // no random motion, a new repeat (and interval) starts every delay period.
        let phase_inc = if random_rate > 0.0 {
            random_rate / sample_rate
        } else if !pitch_intervals.is_empty() {
            1.0 / min_samples
        } else {
            0.0
        };
        let mut fracture = Fracture {
            min_samples,
            max_samples,
            phase_inc,
            phase: 0.0,
            last_random: 0.0,
            next_random: 0.0,
            intervals: pitch_intervals.iter().map(|s| s.round()).collect(),
            mode: pitch_mode,
            index: 0,
            going_up: true,
            pitch_ratio: 1.0,
            pitch_accumulator: 0.0,
            rng: seed_rng.random::<u32>() | 1,
        };
        if random_rate > 0.0 {
            fracture.last_random = fracture.next_unit();
            fracture.next_random = fracture.next_unit();
        }
        if !fracture.intervals.is_empty() {
            fracture.pitch_ratio = 2f32.powf(fracture.next_semitones() / 12.0);
        }
        Self {
            line: DelayLine::new((MAX_DELAY_SECONDS * sample_rate) as usize),
            feedback: (feedback * intensity).clamp(0.0, 0.95),
            lp: 0.0,
            wet,
            dry: 1.0 - wet * 0.5,
            static_delay: min_samples,
            fracture: Some(fracture),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let delayed = match &mut self.fracture {
            None => self.line.read(),
            Some(f) => {
                let delay = f.advance();
                self.line.read_fractional(delay)
            }
        };
        self.lp += 0.5 * (delayed - self.lp);
        self.line.write(x + self.lp * self.feedback);
        (x * self.dry + self.lp * self.wet).clamp(-1.0, 1.0)
    }

    #[cfg(test)]
    pub(crate) fn current_delay_samples(&self) -> f32 {
        match &self.fracture {
            Some(f) => f.current_delay(),
            None => self.static_delay,
        }
    }

    #[cfg(test)]
    pub(crate) fn next_pitch_semitones(&mut self) -> f32 {
        self.fracture.as_mut().map(|f| f.next_semitones()).unwrap_or(0.0)
    }
}

impl Fracture {
    fn current_delay(&self) -> f32 {
        let alpha = 0.5 - 0.5 * (std::f32::consts::PI * self.phase).cos();
        let r = self.last_random * (1.0 - alpha) + self.next_random * alpha;
        self.min_samples + (self.max_samples - self.min_samples) * r
    }

    /// One sample of motion; returns the effective read delay in samples.
    #[inline]
    fn advance(&mut self) -> f32 {
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.last_random = self.next_random;
            self.next_random = if self.phase_inc > 0.0 && self.max_samples > self.min_samples {
                self.next_unit()
            } else {
                0.0
            };
            if !self.intervals.is_empty() {
                self.pitch_ratio = 2f32.powf(self.next_semitones() / 12.0);
                self.pitch_accumulator = 0.0;
            }
        }
        if !self.intervals.is_empty() {
            self.pitch_accumulator += self.pitch_ratio - 1.0;
        }
        let mut delay = self.current_delay() - self.pitch_accumulator;
        // A transposed repeat that has run out of buffer starts over.
        if !self.intervals.is_empty() && (delay < 1.0 || delay >= self.max_samples * 2.0) {
            self.pitch_accumulator = 0.0;
            self.phase = 0.0;
            self.pitch_ratio = 2f32.powf(self.next_semitones() / 12.0);
            delay = self.current_delay();
        }
        delay.max(1.0)
    }

    fn next_semitones(&mut self) -> f32 {
        let n = self.intervals.len();
        if n == 0 {
            return 0.0;
        }
        let semis = match self.mode {
            PitchMode::Random => {
                let i = ((self.next_unit() * n as f32) as usize).min(n - 1);
                self.intervals[i]
            }
            PitchMode::Up => {
                let s = self.intervals[self.index];
                self.index = (self.index + 1) % n;
                s
            }
            PitchMode::Down => {
                let s = self.intervals[self.index];
                self.index = if self.index == 0 { n - 1 } else { self.index - 1 };
                s
            }
            PitchMode::UpDown => {
                let s = self.intervals[self.index];
                if self.going_up {
                    if self.index + 1 >= n {
                        self.index = n.saturating_sub(2);
                        self.going_up = false;
                    } else {
                        self.index += 1;
                    }
                } else if self.index == 0 {
                    self.index = 1.min(n - 1);
                    self.going_up = true;
                } else {
                    self.index -= 1;
                }
                s
            }
        };
        semis
    }
}
```

Note `read_fractional` clamps its argument to `[1, len-1]`; with an 8 s line that never truncates a 4-beat delay at 30 BPM or above, and the `pitch_accumulator` reset keeps the read inside the buffer. In the `random_rate == 0` case `last_random == next_random == 0`, so `current_delay` is exactly `min_samples` and the "fixed delay" test holds sample-exactly.

`from_config` and the chain:

```rust
impl EffectNode {
    pub fn from_config(sample_rate: f32, tempo: u32, config: &EffectConfig) -> Self {
        let intensity = config.intensity.clamp(0.0, 1.0);
        let seconds_per_beat = 60.0 / tempo.max(1) as f32;
        match &config.effect {
            // ... reverb unchanged ...
            EffectType::Delay {
                delay_time,
                feedback,
                wet_level,
                sync_tempo,
                random_beats,
                random_rate,
                pitch_intervals,
                pitch_mode,
            } => {
                if random_beats.is_some() || !pitch_intervals.is_empty() {
                    let [min, max] = random_beats.unwrap_or_else(|| {
                        let beats = if *sync_tempo { *delay_time } else { delay_time / seconds_per_beat };
                        [beats, beats]
                    });
                    EffectNode::Delay(Delay::time_fracture(
                        sample_rate, tempo, min, max, *random_rate, pitch_intervals, *pitch_mode,
                        *feedback, *wet_level, intensity,
                    ))
                } else {
                    let seconds = if *sync_tempo { delay_time * seconds_per_beat } else { *delay_time };
                    EffectNode::Delay(Delay::new(sample_rate, seconds, *feedback, *wet_level, intensity))
                }
            }
            // ... others unchanged ...
        }
    }
}

impl EffectsChain {
    pub fn new(sample_rate: f32, configs: &[EffectConfig]) -> Self {
        Self::with_tempo(sample_rate, DEFAULT_TEMPO, configs)
    }

    pub fn with_tempo(sample_rate: f32, tempo: u32, configs: &[EffectConfig]) -> Self {
        Self {
            nodes: configs
                .iter()
                .filter(|c| c.enabled)
                .map(|c| EffectNode::from_config(sample_rate, tempo, c))
                .collect(),
        }
    }
}
```

Import `PitchMode` from `crate::midi`.

- [ ] **Step 4: Run the tests**

```bash
cargo test effects:: 2>&1 | tail -30 && cargo test 2>&1 | grep -E "^test result"
```

Expected: all pass. If `pitch_interval_of_twelve_repeats_one_octave_up` measures a ZCR far from 880, check the sign of the accumulator (a ratio above 1 must *shorten* the delay each sample so the read position moves faster than real time).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/expressive/effects.rs
git commit -m "feat(effects): time fracture delay with random beat-synced time and pitch-shifted repeats"
```

---

### Task 3: Tempo plumbing and delay-aware render tails

**Files:**
- Modify: `src/expressive/render.rs`, `src/midi/translate.rs`, `src/midi/engine.rs`, `src/midi/player.rs` (tests only), `src/expressive/patch.rs` (library test call), `src/demos.rs` (no change expected; check it compiles)

**Interfaces:**
- Produces:
  ```rust
  pub fn render_length_seconds(patch: &Patch, notes: &[NoteEvent], tempo: u32) -> f32;
  pub fn render_patch(patch: &Patch, notes: &[NoteEvent], sample_rate: f32, tempo: u32) -> Vec<[f32; 2]>;
  pub struct PlayCommand { ..., pub tempo: u32 }   // Default DEFAULT_TEMPO
  ```
  `render_length_seconds` tail = the max of `effect.tail_seconds(tempo)` over the patch's effects (0 when none), replacing the flat `EFFECT_TAIL_SECONDS` (keep the constant for R2D2 until its branch is updated the same way, then delete it if unused).

- [ ] **Step 1: Write the failing tests**

`render.rs` tests (adjust every existing `render_patch(.., SR)` / `render_length_seconds(..)` call to pass `120`):

```rust
    #[test]
    fn a_long_delay_extends_the_tail_with_the_tempo() {
        let p = patch(json!({"name": "d", "subtractive": {"env": {"release": 0.1}},
            "effects": [{"type": "delay", "random_beats": [1.0, 2.0], "feedback": 0.5, "intensity": 0.6}]}));
        let n = [note(0.0, 0.5, 220.0)];
        // 2 beats at 120 = 1 s -> tail 4.5; at 60 = 2 s -> tail 8.5.
        assert!((render_length_seconds(&p, &n, 120) - (0.6 + 4.5)).abs() < 1e-4);
        assert!((render_length_seconds(&p, &n, 60) - (0.6 + 8.5)).abs() < 1e-4);
        let buf = left(&render_patch(&p, &n, SR, 60));
        assert!(rms(&buf[(3.0 * SR) as usize..(3.5 * SR) as usize]) > 1e-3, "repeats still audible at 3 s");
    }

    #[test]
    fn beat_synced_delay_follows_the_tempo() {
        let p = patch(json!({"name": "d", "subtractive": {"osc1": {"wave": "sine"},
            "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
            "effects": [{"type": "delay", "random_beats": [1.0, 1.0], "feedback": 0.0, "wet_level": 1.0, "intensity": 1.0}]}));
        let n = [note(0.0, 0.1, 220.0)];
        let fast = left(&render_patch(&p, &n, SR, 120)); // repeat at 0.5 s
        let slow = left(&render_patch(&p, &n, SR, 60));  // repeat at 1.0 s
        let energy = |s: &[f32], from: f32| rms(&s[(from * SR) as usize..((from + 0.1) * SR) as usize]);
        assert!(energy(&fast, 0.5) > 0.05 && energy(&fast, 1.0) < 1e-3);
        assert!(energy(&slow, 1.0) > 0.05 && energy(&slow, 0.5) < 1e-3);
    }
```

`translate.rs` tests:

```rust
    #[test]
    fn the_sequence_tempo_reaches_patch_effects_and_the_midi_bus() {
        let t = Translator::new(Err("no soundfont".into()));
        let mut s = seq(vec![patch_note(json!({"name": "d", "subtractive": {"env": {"release": 0.01}},
            "effects": [{"type": "delay", "random_beats": [1.0, 1.0], "intensity": 0.5}]}), 60, 0.0, 0.1)]);
        s.tempo = 60;
        let slow = t.translate(s.clone(), PlayMode::Replace, &no_session()).unwrap();
        s.tempo = 120;
        let fast = t.translate(s, PlayMode::Replace, &no_session()).unwrap();
        assert!(slow.duration > fast.duration, "longer beats, longer tail");
        assert_eq!(fast.command.tempo, 120);
        assert_eq!(slow.command.tempo, 60);
    }
```

`engine.rs` tests: the `play(...)` helper constructs `PlayCommand` with `..Default::default()` or explicit fields; add `tempo: 120` where needed, and one assertion that `PlayCommand::default().tempo == DEFAULT_TEMPO`.

- [ ] **Step 2: Run to verify failure**

`cargo test render:: translate engine 2>&1 | tail`. Expected: arity errors.

- [ ] **Step 3: Implement**

`render.rs`:

```rust
pub fn render_length_seconds(patch: &Patch, notes: &[NoteEvent], tempo: u32) -> f32 {
    // ... existing min_gate / last_gate ...
    let effect_tail = patch
        .effects
        .iter()
        .map(|e| e.tail_seconds(tempo))
        .fold(0.0f32, f32::max);
    (last_gate + patch.release_seconds() + effect_tail).min(MAX_RENDER_SECONDS)
}

pub fn render_patch(patch: &Patch, notes: &[NoteEvent], sample_rate: f32, tempo: u32) -> Vec<[f32; 2]> {
    let total = (render_length_seconds(patch, notes, tempo) * sample_rate) as usize;
    // ...
    let mut chain_l = EffectsChain::with_tempo(sample_rate, tempo, &patch.effects);
    let mut chain_r = EffectsChain::with_tempo(sample_rate, tempo, &patch.effects);
    // ...
}
```

Existing tests that asserted `EFFECT_TAIL_SECONDS` (1.0) for a reverb still hold (reverb tail = 1.0). Update the library test in `patch.rs` and every `render_patch` call site to pass a tempo (`120` in tests; `sequence.tempo` in `translate.rs`).

`translate.rs`: `render_patch(&patch, &events, SAMPLE_RATE as f32, sequence.tempo)`, `render_length_seconds(&patch, &events, sequence.tempo)`; the R2D2 chain becomes `EffectsChain::with_tempo(SAMPLE_RATE as f32, sequence.tempo, ...)` and its resize uses `effects.iter().map(|e| e.tail_seconds(sequence.tempo)).fold(0.0, f32::max)` seconds instead of a flat second; `PlayCommand { ..., tempo: sequence.tempo }`. Delete `EFFECT_TAIL_SECONDS` if nothing references it afterwards.

`engine.rs`: add `pub tempo: u32` to `PlayCommand` (with `impl Default` giving `DEFAULT_TEMPO` — replace the derived `Default` with a manual one), `set_bus_effects(&mut self, tempo: u32, effects: &[EffectConfig])` using `EffectsChain::with_tempo(SAMPLE_RATE as f32, tempo, effects)`, and pass `play.tempo` at both call sites.

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/expressive/render.rs src/midi src/expressive/patch.rs
git commit -m "feat: sequence tempo reaches every effects chain; render tails follow the delay"
```

---

### Task 4: Schema, catalog, showcase patch, demo, integration tests

**Files:**
- Modify: `src/server/mcp.rs`, `src/expressive/patch.rs` (`BUILTIN_PATCHES`), `src/demos.rs`, `tests/integration/mcp_protocol.rs`
- Create: `src/expressive/patches/shimmer_keys.json`

- [ ] **Step 1: Write the failing tests**

`mcp.rs` tests module:

```rust
    #[test]
    fn effects_schema_describes_time_fracture_and_it_validates_through_define_synth() {
        let props = &effects_schema()["items"]["properties"];
        assert_eq!(props["random_beats"]["minItems"], 2);
        assert_eq!(props["pitch_mode"]["enum"], json!(["random", "up", "down", "up_down"]));
        assert_eq!(props["pitch_intervals"]["maxItems"], 12);
        assert!(props["random_rate"].is_object());
        assert!(!props["sync_tempo"]["description"].as_str().unwrap().contains("reserved"));

        let mut state = ServerState::new();
        let r = call(&mut state, "define_synth", json!({"name": "shimmer", "subtractive": {},
            "effects": [{"type": "delay", "random_beats": [0.5, 1.0], "random_rate": 0.5,
                         "pitch_intervals": [7, 12], "pitch_mode": "up_down", "feedback": 0.5, "intensity": 0.6}]}));
        assert!(r.error.is_none(), "{:?}", r.error);
        let r = call(&mut state, "define_synth", json!({"name": "bad", "subtractive": {},
            "effects": [{"type": "delay", "random_beats": [0.5, 9.0]}]}));
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("random_beats"));
    }
```

`patch.rs` library test: `assert!(lib.get("shimmer_keys").is_some()); assert!(lib.count() >= 44);`.

`tests/integration/mcp_protocol.rs`:

```rust
#[test]
fn time_fracture_delay_plays_by_name_and_inline_and_is_validated() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 50, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"tempo": 90, "notes": [
            {"synth": "shimmer_keys", "note": 72, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 51, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"note": 60, "instrument": 0, "start_time": 0.0, "duration": 0.3,
             "effects": [{"type": "delay", "delay_time": 0.5, "sync_tempo": true, "feedback": 0.4, "intensity": 0.5}]}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 52, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "subtractive": {}, "effects": [{"type": "delay", "pitch_intervals": [40]}]},
             "note": 60, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("pitch_intervals"));
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 53, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "effects"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("random_beats") && text.contains("pitch_intervals"), "{text}");
}
```

- [ ] **Step 2: Run to verify failure**

`cargo test server::mcp patch:: 2>&1 | tail`.

- [ ] **Step 3: Implement**

`effects_schema()` in `mcp.rs`: change `sync_tempo`'s description to `"delay: when true, delay_time is in beats of the sequence tempo"`, `delay_time`'s to `"delay: seconds (or beats when sync_tempo); 0.25 = quarter note at 120 BPM (default 0.25)"`, and add:

```rust
                "random_beats": {"type": "array", "items": {"type": "number", "minimum": 0, "maximum": 4}, "minItems": 2, "maxItems": 2,
                    "description": "delay (Time Fracture): [min, max] delay in beats of the sequence tempo; replaces delay_time. Equal values = a fixed beat-synced delay"},
                "random_rate": {"type": "number", "minimum": 0, "maximum": 10, "default": 0,
                    "description": "delay (Time Fracture): how fast the delay time wanders between min and max, in Hz (0 = fixed at min)"},
                "pitch_intervals": {"type": "array", "items": {"type": "number", "minimum": -12, "maximum": 12}, "maxItems": 12,
                    "description": "delay (Time Fracture): semitone shifts for successive repeats, e.g. [7, 12] for a fifth-and-octave shimmer"},
                "pitch_mode": {"type": "string", "enum": ["random", "up", "down", "up_down"], "default": "random",
                    "description": "delay (Time Fracture): order the pitch_intervals are visited in"},
```

Update the schema's top-level description example to include a Time Fracture entry. In `handle_list_sounds`'s effects section replace the delay line with `- delay: delay_time, feedback, wet_level, sync_tempo; Time Fracture: random_beats [min, max], random_rate, pitch_intervals, pitch_mode (random/up/down/up_down)`. In the `define_synth` description add one example: `- Shimmer: {\"name\": \"shimmer\", \"category\": \"keys\", \"fm\": {\"algorithm\": \"stack\", \"operators\": [{\"ratio\": 1, \"env\": {\"decay\": 1.5, \"sustain\": 0.2, \"release\": 2}}, {\"ratio\": 3.5, \"level\": 0.5, \"env\": {\"decay\": 0.5, \"sustain\": 0}}]}, \"effects\": [{\"type\": \"delay\", \"random_beats\": [0.75, 0.75], \"pitch_intervals\": [7, 12], \"pitch_mode\": \"up\", \"feedback\": 0.5, \"intensity\": 0.6}, {\"type\": \"reverb\", \"room_size\": 0.8, \"intensity\": 0.4}]}`.

`src/expressive/patches/shimmer_keys.json`:

```json
{
  "name": "shimmer_keys",
  "description": "FM bell through a Time Fracture delay whose repeats climb a fifth then an octave, into a large reverb",
  "category": "keys",
  "level": 0.7,
  "fm": {
    "algorithm": "stack",
    "operators": [
      {"ratio": 1.0, "level": 1.0, "env": {"attack": 0.002, "decay": 1.5, "sustain": 0.2, "release": 2.0}},
      {"ratio": 3.5, "level": 0.5, "env": {"attack": 0.001, "decay": 0.5, "sustain": 0.0, "release": 0.5}}
    ]
  },
  "effects": [
    {"type": "delay", "random_beats": [0.75, 0.75], "random_rate": 0.0, "pitch_intervals": [7, 12], "pitch_mode": "up",
     "feedback": 0.5, "wet_level": 0.6, "intensity": 0.6},
    {"type": "reverb", "room_size": 0.8, "dampening": 0.3, "wet_level": 0.5, "intensity": 0.4}
  ]
}
```

Add it to `BUILTIN_PATCHES` under `// time fracture`. In `src/demos.rs` `test_effects`, add a fourth pass `("time fracture shimmer", Some(shimmer))` where `shimmer` parses `[{"type": "delay", "random_beats": [0.5, 1.0], "random_rate": 0.5, "pitch_intervals": [7, 12], "pitch_mode": "up_down", "feedback": 0.5, "intensity": 0.6}]`; the chord's `player.play` already receives `mode` and the empty session map.

- [ ] **Step 4: Run the tests**

```bash
cargo test 2>&1 | grep -E "^test result|panicked|FAILED"
```

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/server/mcp.rs src/expressive/patch.rs src/expressive/patches src/demos.rs tests
git commit -m "feat(mcp): time fracture delay in the effects schema, catalog, a shimmer_keys patch and the effects demo"
```

---

### Task 5: Docs pass and spec close-out

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `examples/api_reference.md`, `examples/basic_usage.md`, `docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md`

- [ ] **Step 1: README**

- Effects bullet under "### Engines": append "The `delay` also does Time Fracture: `random_beats: [min, max]` in beats of the sequence tempo, `random_rate` Hz, `pitch_intervals` in semitones with `pitch_mode` `random|up|down|up_down`; `sync_tempo: true` puts `delay_time` in beats."
- Add a short "Shimmer delay" example (the `shimmer` JSON from the tool description) after the granular example.
- Built-in count → 44; mention `shimmer_keys` in the keys list.
- Whole-file pass: remove or reword any sentence that still describes behaviour that no longer exists (search for "reserved", "19 types", "preset", "3 effects", "2x", "per note" in the effects bullets; the line "reverb, delay, chorus, filter, compressor and distortion rendered per note" becomes "rendered per patch (shared tails) and per MIDI bus"), and make sure every JSON example in the file parses (`python3 -c 'import json,sys; ...'` over each fenced block).

- [ ] **Step 2: CLAUDE.md**

- Effects bullet: "`effects.rs` - stateful effects: Schroeder reverb, damped feedback delay with Time Fracture (beat-synced random time, pitch-shifted repeats), 3-voice chorus, TPT state-variable filter, compressor, tanh distortion. `EffectsChain::with_tempo(sample_rate, tempo, &[EffectConfig])` (or `new` for 120 BPM) then `process` per sample or `process_buffer`."
- Pipeline: "`PlayCommand.tempo` carries the sequence tempo to the MIDI bus chain; `render_patch` and the R2D2 chain take it directly; render tails come from `EffectConfig::tail_seconds`."
- Patches count 44.

- [ ] **Step 3: examples**

`examples/api_reference.md`: effects table gains the four Time Fracture fields and the `sync_tempo` meaning. `examples/basic_usage.md`: one shimmer example. Run the `rg` check from PR 1 (`preset_name|synth_type|synth_attack|debug-dx7|test-volumes|test-pads|test-presets`) over `examples README.md CLAUDE.md`; it must print only the README migration table's "Before" column.

- [ ] **Step 4: Spec**

`Status:` → `All four PRs implemented (2026-09-07); follow-up: issue #109 headroom`. §1 `effects`: add "`random_rate` 0 fixes the delay at `min`; with pitch intervals and rate 0 a new repeat (and the next interval) starts every delay period. Intervals are rounded to whole semitones. Tails: `EffectConfig::tail_seconds` = 4 × the longest delay + 0.5 s (min 1 s), 1 s for reverb, 0.5 s otherwise." §3: replace "plus `EFFECT_TAIL_SECONDS`" with the per-effect tail. §8: mark all four PRs done and list the follow-up.

- [ ] **Step 5: Verify and commit**

```bash
rustup update stable >/dev/null 2>&1; cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test 2>&1 | grep -E "^test result"
git add README.md CLAUDE.md examples docs/superpowers/specs
git commit -m "docs: time fracture delay, tempo-aware effects, project close-out"
```

- [ ] **Step 6: Open the PR**

Use the `superpowers:finishing-a-development-branch` skill. Title: `feat: time fracture delay, tempo-aware effects`. Body: spec and plan links; the `shimmer_keys` patch; `sync_tempo` now meaningful; per-effect tails; follow-up #109.

---

## Self-review notes

- Spec coverage for PR 4: §1 `effects` Time Fracture fields (Tasks 1, 2, 4), "beat values use the sequence tempo" (Task 3), §3 tail (Task 3, improved), §4 effects row (Task 2), §7 Time Fracture tests (Task 2: `time_fracture_with_zero_rate_is_a_fixed_delay_at_min_beats`, `pitch_interval_of_twelve_repeats_one_octave_up`), §8 PR 4 (this plan; `demos.rs` was already rewritten in PR 1, so only the effects demo gains a pass).
- Names used consistently: `PitchMode::{Random, Up, Down, UpDown}` + `ALL`/`as_str`; `Delay` fields `random_beats`, `random_rate`, `pitch_intervals`, `pitch_mode`; `EffectConfig::tail_seconds(tempo)`; `Delay::time_fracture(...)`; `EffectNode::from_config(sample_rate, tempo, cfg)`; `EffectsChain::with_tempo`; `DEFAULT_TEMPO`; `render_patch(patch, notes, sample_rate, tempo)`; `render_length_seconds(patch, notes, tempo)`; `PlayCommand.tempo`.
- Out of scope: issue #109 (chord headroom, sustained-note guard, drum levels); microtonal intervals (rounded like tryx-fx without microtone mode); per-channel MIDI effects.
