//! Agent-defined synth patches: the JSON data model, validation and the
//! library of built-in patches. See docs/superpowers/specs/2026-09-06-agent-defined-synths-design.md.

use crate::expressive::{Adsr, Wave};
use crate::midi::EffectConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        assert!(
            err.contains("subtractive.filter.cutoff") && err.contains("20"),
            "{err}"
        );

        let p = parse(json!({"name": "x", "subtractive": {"osc2": {"octave": 3}}})).unwrap();
        assert!(
            p.validate()
                .unwrap_err()
                .contains("subtractive.osc2.octave")
        );

        let p = parse(json!({"name": "x", "subtractive": {"filter": {"slope": 18}}})).unwrap();
        assert!(p.validate().unwrap_err().contains("slope"));

        let p = parse(json!({"name": "x", "subtractive": {"env": {"sustain": 2}}})).unwrap();
        assert!(
            p.validate()
                .unwrap_err()
                .contains("subtractive.env.sustain")
        );

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
        let p =
            parse(json!({"name": "k", "percussion": {"kind": "swoosh", "sweep": [200.0, 2000.0]}}))
                .unwrap();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn synth_ref_is_a_name_or_an_inline_patch() {
        let r: SynthRef = serde_json::from_value(json!("minimoog_bass")).unwrap();
        assert!(matches!(r, SynthRef::Name(n) if n == "minimoog_bass"));
        let r: SynthRef = serde_json::from_value(json!({"name": "x", "subtractive": {}})).unwrap();
        assert!(matches!(r, SynthRef::Inline(_)));

        // An object is always read as an inline patch, so a typo inside it
        // names the offending field instead of "did not match any variant".
        let err = serde_json::from_value::<SynthRef>(
            json!({"name": "x", "subtractive": {"cutoff": 500}}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("cutoff"), "{err}");

        let err = serde_json::from_value::<SynthRef>(json!(42))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("patch name") && err.contains("inline patch object"),
            "{err}"
        );
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

    #[test]
    fn every_builtin_patch_parses_validates_and_renders_cleanly() {
        use crate::expressive::render::{NoteEvent, render_patch};
        let lib = PatchLibrary::new();
        assert!(
            lib.count() >= 28,
            "expected the migrated presets, got {}",
            lib.count()
        );
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
            assert!(
                peak * 0.5 <= 0.8,
                "{name} peaks at {peak}, too hot for the bus"
            );
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
            assert!(
                patches.windows(2).all(|w| w[0].name <= w[1].name),
                "sorted by name"
            );
        }
        assert!(lib.get("dx7_e_piano").is_some());
        assert!(cats.contains(&PatchCategory::Keys), "keys category is back");
        assert!(lib.get("wt_organ").is_some());
        assert!(lib.get("grain_cloud").is_some() && lib.get("drone").is_some());
        assert!(lib.count() >= 43);
    }

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
        assert!(
            p.validate().unwrap_err().contains("fm.operators"),
            "empty operators"
        );
        let five: Vec<serde_json::Value> = (0..5).map(|_| json!({})).collect();
        let p = parse(json!({"name": "x", "fm": {"operators": five}})).unwrap();
        assert!(
            p.validate().unwrap_err().contains("fm.operators"),
            "too many operators"
        );
        let p = parse(json!({"name": "x", "fm": {"operators": [{"ratio": 20}]}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(
            err.contains("fm.operators[0].ratio") && err.contains("16"),
            "{err}"
        );
        let p =
            parse(json!({"name": "x", "fm": {"operators": [{}, {"detune_cents": 150}]}})).unwrap();
        assert!(
            p.validate()
                .unwrap_err()
                .contains("fm.operators[1].detune_cents")
        );
        let p = parse(json!({"name": "x", "fm": {"feedback": 2}})).unwrap();
        assert!(p.validate().unwrap_err().contains("fm.feedback"));
        let p =
            parse(json!({"name": "x", "fm": {"operators": [{"env": {"sustain": 3}}]}})).unwrap();
        assert!(
            p.validate()
                .unwrap_err()
                .contains("fm.operators[0].env.sustain")
        );
        assert!(
            parse(json!({"name": "x", "fm": {"algorithm": "serial"}})).is_err(),
            "unknown algorithm"
        );
        assert!(
            parse(json!({"name": "x", "fm": {"mod_index": 2}}))
                .unwrap_err()
                .contains("mod_index")
        );
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
    fn wavetable_patch_parses_validates_and_names_fields() {
        let p = parse(json!({"name": "w", "wavetable": {}})).unwrap();
        let wt = p.wavetable.as_ref().unwrap();
        assert_eq!(wt.table, TableName::Basic);
        assert_eq!(wt.morph, 0.0);
        assert!(p.validate().is_ok() && p.has_pitched_engine());
        let p = parse(json!({"name": "w", "wavetable": {"table": "organ", "morph": 0.4, "env": {"release": 1.0}}})).unwrap();
        assert_eq!(p.release_seconds(), 1.0);
        assert_eq!(
            serde_json::to_value(&p).unwrap()["wavetable"]["table"],
            "organ"
        );
        let p = parse(json!({"name": "w", "wavetable": {"morph": 1.5}})).unwrap();
        assert!(p.validate().unwrap_err().contains("wavetable.morph"));
        assert!(parse(json!({"name": "w", "wavetable": {"table": "sawtooth"}})).is_err());
        assert!(
            parse(json!({"name": "w", "wavetable": {"position": 0.2}}))
                .unwrap_err()
                .contains("position")
        );
        assert_eq!(TableName::Noise.next(), TableName::Basic, "morph wraps");
        assert_eq!(TableName::Pwm.index(), 5);
    }

    #[test]
    fn release_seconds_is_the_longest_across_engines() {
        let p = parse(json!({"name": "both",
            "subtractive": {"env": {"release": 0.5}},
            "fm": {"operators": [{"env": {"release": 1.5}}, {"env": {"release": 9.0}}]}}))
        .unwrap();
        // operator 2 is a modulator in `stack`, so its 9 s release does not count.
        assert_eq!(p.release_seconds(), 1.5);
    }

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
            "lfo": {"rate": 0.3, "depth": 0.2, "wave": "sample_hold", "target": "cutoff"}}))
        .unwrap();
        assert!(p.lfo.as_ref().unwrap().is_active());
        assert_eq!(serde_json::to_value(&p).unwrap()["lfo"]["target"], "cutoff");

        let p = parse(json!({"name": "x", "subtractive": {}, "lfo": {"rate": 50}})).unwrap();
        let err = p.validate().unwrap_err();
        assert!(err.contains("lfo.rate") && err.contains("20"), "{err}");
        let p = parse(json!({"name": "x", "subtractive": {}, "lfo": {"depth": 2}})).unwrap();
        assert!(p.validate().unwrap_err().contains("lfo.depth"));
        assert!(
            parse(json!({"name": "x", "subtractive": {}, "lfo": {"target": "filter"}})).is_err()
        );
        assert!(
            parse(json!({"name": "x", "subtractive": {}, "lfo": {"speed": 1}}))
                .unwrap_err()
                .contains("speed")
        );
        assert_eq!(LfoTarget::ALL.len(), 6);
        assert_eq!(LfoTarget::GrainDensity.as_str(), "grain_density");
        assert_eq!(LfoWave::SampleHold.as_str(), "sample_hold");
    }

    #[test]
    fn granular_patch_parses_validates_and_names_fields() {
        let p = parse(json!({"name": "g", "granular": {}})).unwrap();
        let g = p.granular.as_ref().unwrap();
        assert_eq!(g.source, GrainSource::Harmonics);
        assert_eq!(
            (
                g.grain_ms,
                g.density,
                g.pitch_semitones,
                g.randomness,
                g.stereo_width
            ),
            (50.0, 10.0, 0.0, 0.2, 0.5)
        );
        assert!(p.validate().is_ok() && p.has_pitched_engine());
        let p = parse(
            json!({"name": "g", "granular": {"source": "formant", "grain_ms": 120,
            "density": 15, "pitch_semitones": 7, "randomness": 0.7, "stereo_width": 0.9,
            "env": {"release": 3.0}}}),
        )
        .unwrap();
        assert!(p.validate().is_ok());
        assert_eq!(p.release_seconds(), 3.0);
        assert_eq!(
            serde_json::to_value(&p).unwrap()["granular"]["source"],
            "formant"
        );
        for (field, value) in [
            ("grain_ms", 1000.0),
            ("density", 0.5),
            ("pitch_semitones", 30.0),
            ("randomness", 1.5),
            ("stereo_width", -0.1),
            ("level", 2.0),
        ] {
            let p = parse(json!({"name": "g", "granular": {field: value}})).unwrap();
            let err = p.validate().unwrap_err();
            assert!(err.contains(&format!("granular.{field}")), "{field}: {err}");
        }
        assert!(parse(json!({"name": "g", "granular": {"source": "sample"}})).is_err());
        assert!(
            parse(json!({"name": "g", "granular": {"grain_size": 0.1}}))
                .unwrap_err()
                .contains("grain_size")
        );
        let p = parse(json!({"name": "silent"})).unwrap();
        assert!(
            p.validate().unwrap_err().contains("granular"),
            "no-engines message lists granular"
        );
    }

    #[test]
    fn invalid_effect_on_a_patch_is_reported_with_its_index() {
        let p = parse(json!({
            "name": "fractured",
            "subtractive": {},
            "effects": [{"type": "delay", "random_rate": 50}],
        }))
        .unwrap();
        let err = p.validate().unwrap_err();
        assert!(
            err.contains("effects[0]") && err.contains("random_rate"),
            "{err}"
        );
    }
}

fn one() -> f32 {
    1.0
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
    pub fm: Option<Fm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wavetable: Option<Wavetable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granular: Option<Granular>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percussion: Option<Percussion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lfo: Option<LfoConfig>,
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
#[allow(clippy::enum_variant_names)]
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
    pub const ALL: [FmAlgorithm; 4] = [
        FmAlgorithm::Stack,
        FmAlgorithm::Pairs,
        FmAlgorithm::FanIn,
        FmAlgorithm::Parallel,
    ];

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
        Self::ALL
            .iter()
            .position(|t| t == self)
            .expect("every table is in ALL")
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
        check_range(
            &format!("{path}.pitch_semitones"),
            self.pitch_semitones,
            -24.0,
            24.0,
        )?;
        check_range(&format!("{path}.randomness"), self.randomness, 0.0, 1.0)?;
        check_range(&format!("{path}.stereo_width"), self.stereo_width, 0.0, 1.0)?;
        self.env.validate(&format!("{path}.env"))
    }
}

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

/// A patch reference on a note: a stored name or a one-off inline patch.
///
/// Serialized untagged (a bare string or a patch object). Deserialization is
/// hand-written rather than `#[serde(untagged)]`: untagged swallows the inner
/// error and reports only "data did not match any variant", so a typo in an
/// inline patch gave the model nothing to fix. Dispatching on the JSON shape
/// first lets the `Patch` error through with its field name.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum SynthRef {
    Name(String),
    Inline(Patch),
}

impl<'de> Deserialize<'de> for SynthRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(name) => Ok(SynthRef::Name(name)),
            v @ serde_json::Value::Object(_) => serde_json::from_value::<Patch>(v)
                .map(SynthRef::Inline)
                .map_err(D::Error::custom),
            _ => Err(D::Error::custom(
                "synth must be a patch name (string) or an inline patch object",
            )),
        }
    }
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
        let wavetable = self
            .wavetable
            .as_ref()
            .filter(|w| w.level > 0.0)
            .map(|w| w.env.release)
            .unwrap_or(0.0);
        let granular = self
            .granular
            .as_ref()
            .filter(|g| g.level > 0.0)
            .map(|g| g.env.release)
            .unwrap_or(0.0);
        sub.max(fm).max(wavetable).max(granular)
    }

    /// True when at least one enabled engine takes its pitch from the note.
    pub fn has_pitched_engine(&self) -> bool {
        self.subtractive.as_ref().is_some_and(|s| s.level > 0.0)
            || self.fm.as_ref().is_some_and(|f| f.level > 0.0)
            || self.wavetable.as_ref().is_some_and(|w| w.level > 0.0)
            || self.granular.as_ref().is_some_and(|g| g.level > 0.0)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        check_range("level", self.level, 0.0, 1.0)?;
        if self.subtractive.is_none()
            && self.percussion.is_none()
            && self.fm.is_none()
            && self.wavetable.is_none()
            && self.granular.is_none()
        {
            return Err(format!(
                "patch '{}' has no engines: add \"subtractive\", \"fm\", \"wavetable\", \"granular\" or \"percussion\"",
                self.name
            ));
        }
        if let Some(sub) = &self.subtractive {
            check_range("subtractive.level", sub.level, 0.0, 1.0)?;
            check_range(
                "subtractive.osc1.pulse_width",
                sub.osc1.pulse_width,
                0.1,
                0.9,
            )?;
            if let Some(o2) = &sub.osc2 {
                check_range("subtractive.osc2.pulse_width", o2.pulse_width, 0.1, 0.9)?;
                check_range("subtractive.osc2.mix", o2.mix, 0.0, 1.0)?;
                check_range(
                    "subtractive.osc2.detune_cents",
                    o2.detune_cents,
                    -100.0,
                    100.0,
                )?;
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
        if let Some(fm) = &self.fm {
            fm.validate("fm")?;
        }
        if let Some(wt) = &self.wavetable {
            wt.validate("wavetable")?;
        }
        if let Some(g) = &self.granular {
            g.validate("granular")?;
        }
        if let Some(perc) = &self.percussion {
            perc.validate("percussion")?;
        }
        if let Some(lfo) = &self.lfo {
            lfo.validate("lfo")?;
        }
        for (i, e) in self.effects.iter().enumerate() {
            e.validate_effect_config()
                .map_err(|err| format!("effects[{i}]: {err}"))?;
        }
        Ok(())
    }
}

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
    // fm
    include_str!("patches/dx7_e_piano.json"),
    include_str!("patches/dx7_slap_bass.json"),
    include_str!("patches/tx81z_lately.json"),
    include_str!("patches/fm_bell.json"),
    // wavetable
    include_str!("patches/wt_organ.json"),
    include_str!("patches/wt_vocal_pad.json"),
    include_str!("patches/wt_pwm_lead.json"),
    include_str!("patches/wt_glass_keys.json"),
    // granular / lfo
    include_str!("patches/grain_cloud.json"),
    include_str!("patches/formant_texture.json"),
    include_str!("patches/noise_texture.json"),
    include_str!("patches/drone.json"),
];

/// Every built-in patch, parsed and validated once at construction.
pub struct PatchLibrary {
    patches: HashMap<String, Patch>,
}

impl Default for PatchLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchLibrary {
    /// Parses every embedded patch file; panics on a bad file.
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

    /// Case-insensitive, trimmed lookup by name.
    pub fn get(&self, name: &str) -> Option<&Patch> {
        self.patches.get(&name.trim().to_lowercase())
    }

    /// Every patch name, sorted. (Only the tests enumerate the library today;
    /// `catalog` is what `list_sounds` uses.)
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.patches.values().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names
    }

    /// Total number of built-in patches.
    pub fn count(&self) -> usize {
        self.patches.len()
    }

    /// Every non-empty category, in `PatchCategory::ALL` order, with its
    /// patches sorted by name.
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
