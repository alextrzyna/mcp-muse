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
        if let Some(perc) = &self.percussion {
            perc.validate("percussion")?;
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
