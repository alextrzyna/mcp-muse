//! Offline export of a composition to WAV files: a stereo mixdown, wet stems
//! or dry tracks. Design: docs/superpowers/specs/2026-09-09-audio-export-design.md.

use crate::expressive::{EffectsPresetLibrary, Patch};
use crate::midi::engine::{
    CHUNK_FRAMES, EngineCommand, LEAD_FRAMES, MidiEngine, PlayCommand, PlayMode, SAMPLE_RATE,
    find_soundfont, load_synth, soft_clip,
};
use crate::midi::gm_names::GM_INSTRUMENTS;
use crate::midi::parser::MidiNote;
use crate::midi::translate::{Effects, TranslatedParts, Translator, midi_events};
use crate::midi::{EffectConfig, EffectType, SimpleSequence};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Split {
    /// One stereo file, soft-clipped like playback.
    #[default]
    Mixdown,
    /// One file per source with every effect chain applied; they sum to the mix.
    Stems,
    /// One file per source with the effect chains bypassed.
    Tracks,
}

impl Split {
    pub fn as_str(self) -> &'static str {
        match self {
            Split::Mixdown => "mixdown",
            Split::Stems => "stems",
            Split::Tracks => "tracks",
        }
    }
}

/// WAV sample format. Integer formats are clamped to ±1.0; float keeps the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BitDepth {
    Int16,
    #[default]
    Int24,
    Float32,
}

impl BitDepth {
    pub fn from_bits(bits: u32) -> Result<Self, String> {
        match bits {
            16 => Ok(BitDepth::Int16),
            24 => Ok(BitDepth::Int24),
            32 => Ok(BitDepth::Float32),
            other => Err(format!(
                "bit_depth must be 16, 24 or 32 (32 is IEEE float), got {}",
                other
            )),
        }
    }

    fn spec(self) -> hound::WavSpec {
        let (bits_per_sample, sample_format) = match self {
            BitDepth::Int16 => (16, hound::SampleFormat::Int),
            BitDepth::Int24 => (24, hound::SampleFormat::Int),
            BitDepth::Float32 => (32, hound::SampleFormat::Float),
        };
        hound::WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE,
            bits_per_sample,
            sample_format,
        }
    }
}

pub struct ExportRequest {
    pub sequence: SimpleSequence,
    /// Absolute directory the files go under.
    pub dir: PathBuf,
    /// Base name before sanitizing.
    pub name: String,
    pub split: Split,
    pub bit_depth: BitDepth,
    pub overwrite: bool,
}

#[derive(Debug)]
pub struct ExportedFile {
    pub path: PathBuf,
    /// Largest absolute sample before any clamping.
    pub peak: f32,
}

#[derive(Debug)]
pub struct ExportReport {
    pub files: Vec<ExportedFile>,
    /// The composition's length including effect tails; every file has it.
    pub duration: Duration,
    pub render_time: Duration,
}

/// Keep `[A-Za-z0-9_-]`; every other character becomes `_`.
pub fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Render and write the request. Uses the installed SoundFont for MIDI notes.
pub fn export(
    request: ExportRequest,
    session_patches: &HashMap<String, Patch>,
) -> Result<ExportReport, String> {
    export_with(request, session_patches, find_soundfont())
}

/// One target file and what goes into it.
struct Target {
    path: PathBuf,
    source: Source,
}

/// What goes into one target file.
enum Source {
    /// Everything, soft-clipped.
    Mixdown,
    /// One MIDI channel's events plus the bus chain (none for tracks).
    Channel(u8),
    /// Index into `TranslatedParts::patches`.
    Patch(usize),
    /// Every R2D2 note summed.
    R2d2,
}

/// `export` with the SoundFont lookup injected, so tests can run without one.
pub(crate) fn export_with(
    request: ExportRequest,
    session_patches: &HashMap<String, Patch>,
    soundfont: Result<PathBuf, String>,
) -> Result<ExportReport, String> {
    let started = Instant::now();
    let name = sanitize_name(&request.name);
    if name.is_empty() {
        return Err("name must contain at least one letter, digit, '_' or '-'".to_string());
    }

    let translator = Translator::new(soundfont.clone().map(|_| ()));
    let wet = translator.translate_parts(
        request.sequence.clone(),
        session_patches,
        Effects::Wet,
        None,
    )?;
    if wet.midi.is_empty() && wet.patches.is_empty() && wet.r2d2.is_empty() {
        return Err("Nothing to export: the sequence has no notes".to_string());
    }
    let duration = wet.duration;
    let frames = (duration.as_secs_f64() * SAMPLE_RATE as f64).ceil() as usize;
    let parts = match request.split {
        Split::Tracks => translator.translate_parts(
            request.sequence.clone(),
            session_patches,
            Effects::Dry,
            None,
        )?,
        Split::Mixdown | Split::Stems => wet,
    };

    let targets = plan_targets(&request.dir, &name, request.split, &parts)?;
    check_collisions(&targets, request.overwrite)?;
    if request.dir.is_file() {
        return Err(format!(
            "path {} exists and is not a directory",
            request.dir.display()
        ));
    }

    let synth = if parts.midi.is_empty() {
        None
    } else {
        Some(load_synth(&soundfont?)?)
    };
    let (mut engine, _handle) = MidiEngine::new(synth);

    for target in &targets {
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }

    let mut files = Vec::with_capacity(targets.len());
    match request.split {
        Split::Mixdown => {
            let target = targets.into_iter().next().expect("mixdown has one target");
            let mut mix =
                render_offline(&mut engine, parts.into_command(PlayMode::Replace), frames);
            for frame in &mut mix {
                frame[0] = soft_clip(frame[0]);
                frame[1] = soft_clip(frame[1]);
            }
            let peak = write_wav(&target.path, &mix, request.bit_depth)?;
            files.push(ExportedFile {
                path: target.path,
                peak,
            });
        }
        Split::Stems | Split::Tracks => {
            for target in targets {
                let samples = match target.source {
                    Source::Mixdown => unreachable!("splits never plan a mixdown target"),
                    Source::Channel(channel) => {
                        let notes: Vec<MidiNote> = parts
                            .midi
                            .iter()
                            .filter(|n| n.channel == channel)
                            .cloned()
                            .collect();
                        render_offline(
                            &mut engine,
                            PlayCommand {
                                events: midi_events(&notes, Some(0)),
                                external: Vec::new(),
                                buffers: Vec::new(),
                                midi_effects: parts.midi_effects.clone(),
                                mode: PlayMode::Replace,
                                tempo: parts.tempo,
                            },
                            frames,
                        )
                    }
                    Source::Patch(i) => {
                        let patch = &parts.patches[i];
                        place(&[(patch.start, patch.samples.as_slice())], frames)
                    }
                    Source::R2d2 => {
                        let buffers: Vec<(u64, &[[f32; 2]])> = parts
                            .r2d2
                            .iter()
                            .map(|(start, samples)| (*start, samples.as_slice()))
                            .collect();
                        place(&buffers, frames)
                    }
                };
                let peak = write_wav(&target.path, &samples, request.bit_depth)?;
                files.push(ExportedFile {
                    path: target.path,
                    peak,
                });
            }
        }
    }

    let report = ExportReport {
        files,
        duration,
        render_time: started.elapsed(),
    };
    tracing::info!(
        "Exported {} file(s) ({}) to {} in {:.2}s",
        report.files.len(),
        request.split.as_str(),
        request.dir.display(),
        report.render_time.as_secs_f64()
    );
    Ok(report)
}

/// Every file the request will write, derived before anything is rendered.
fn plan_targets(
    dir: &Path,
    name: &str,
    split: Split,
    parts: &TranslatedParts,
) -> Result<Vec<Target>, String> {
    Ok(match split {
        Split::Mixdown => vec![Target {
            path: dir.join(format!("{}.wav", name)),
            source: Source::Mixdown,
        }],
        Split::Stems | Split::Tracks => {
            let folder = dir.join(name);
            sources(parts)
                .into_iter()
                .map(|(source_name, source)| Target {
                    path: folder.join(format!("{}.wav", source_name)),
                    source,
                })
                .collect()
        }
    })
}

/// The split's sources in a stable order: MIDI channels ascending, then
/// patch groups in translation order, then R2D2. Names that repeat (two
/// inline patches with the same name) get `_2`, `_3`, ... appended.
fn sources(parts: &TranslatedParts) -> Vec<(String, Source)> {
    let mut out: Vec<(String, Source)> = Vec::new();
    let mut channels: Vec<u8> = parts.midi.iter().map(|n| n.channel).collect();
    channels.sort_unstable();
    channels.dedup();
    for channel in channels {
        let first = parts
            .midi
            .iter()
            .filter(|n| n.channel == channel)
            .min_by_key(|n| n.start_time)
            .expect("channel has notes");
        out.push((
            channel_name(channel, first.instrument.unwrap_or(0)),
            Source::Channel(channel),
        ));
    }
    for (i, patch) in parts.patches.iter().enumerate() {
        out.push((
            format!("synth_{}", sanitize_name(&patch.name)),
            Source::Patch(i),
        ));
    }
    if !parts.r2d2.is_empty() {
        out.push(("r2d2".to_string(), Source::R2d2));
    }
    dedupe_names(&mut out);
    out
}

/// Dedupe against the set of final names actually assigned, not against the
/// original names: for each source in order, if its name is already taken
/// (by an earlier source's original name or by an earlier suffix this
/// function chose), append `_2`, `_3`, ... until a free name is found. This
/// keeps the first occurrence of a name stable and guarantees every
/// returned name is distinct, even when a suffixed name collides with a
/// later source's own real name.
fn dedupe_names(sources: &mut [(String, Source)]) {
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, _) in sources.iter_mut() {
        if used.insert(name.clone()) {
            continue;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{}_{}", name, n);
            if used.insert(candidate.clone()) {
                *name = candidate;
                break;
            }
            n += 1;
        }
    }
}

/// True when the MIDI bus chain of this sequence has an effect that does
/// not commute with summing (compressor, distortion, or a Time Fracture
/// delay with randomised time or pitch), so per-channel stems cannot sum
/// back to the mix exactly. Mirrors `Translator::translate_parts`: the bus
/// chain is the first MIDI note's (not r2d2, not `is_synthesis()`) effects,
/// with its `effects_preset` (if any) appended.
pub fn bus_chain_is_nonlinear(sequence: &SimpleSequence) -> bool {
    let library = EffectsPresetLibrary::new();
    let chain = sequence
        .notes
        .iter()
        .filter(|n| !n.is_r2d2() && !n.is_synthesis())
        .find_map(|n| {
            let mut effects = n.effects.clone().unwrap_or_default();
            if let Some(preset) = &n.effects_preset
                && let Some(preset_effects) = library.get_preset(preset)
            {
                effects.extend(preset_effects.clone());
            }
            if effects.is_empty() {
                None
            } else {
                Some(effects)
            }
        });
    let Some(chain) = chain else {
        return false;
    };
    chain.iter().any(effect_is_nonlinear)
}

fn effect_is_nonlinear(config: &EffectConfig) -> bool {
    match &config.effect {
        EffectType::Compressor { .. } | EffectType::Distortion { .. } => true,
        EffectType::Delay {
            random_beats,
            pitch_intervals,
            ..
        } => random_beats.is_some() || !pitch_intervals.is_empty(),
        EffectType::Reverb { .. } | EffectType::Chorus { .. } | EffectType::Filter { .. } => false,
    }
}

/// `ch09_drums` for the drum channel, else `ch<NN>_<gm program name>`.
fn channel_name(channel: u8, program: u8) -> String {
    if channel == 9 {
        return "ch09_drums".to_string();
    }
    let gm = GM_INSTRUMENTS[(program as usize).min(GM_INSTRUMENTS.len() - 1)];
    format!("ch{:02}_{}", channel, sanitize_name(gm).to_lowercase())
}

fn check_collisions(targets: &[Target], overwrite: bool) -> Result<(), String> {
    if overwrite {
        return Ok(());
    }
    let existing: Vec<String> = targets
        .iter()
        .filter(|t| t.path.exists())
        .map(|t| t.path.display().to_string())
        .collect();
    if existing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Refusing to overwrite existing files (pass \"overwrite\": true to replace them): {}",
            existing.join(", ")
        ))
    }
}

/// Render `frames` frames of `command` from the engine's current (quiet)
/// state and drop the engine's lead-in. Unclipped; the caller decides.
fn render_offline(engine: &mut MidiEngine, command: PlayCommand, frames: usize) -> Vec<[f32; 2]> {
    engine.apply(EngineCommand::Play(PlayCommand {
        mode: PlayMode::Replace,
        ..command
    }));
    let total = LEAD_FRAMES as usize + frames;
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(total + CHUNK_FRAMES);
    let (mut left, mut right) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
    while out.len() < total {
        engine.render_unclipped(&mut left, &mut right);
        out.extend(left.iter().zip(&right).map(|(&l, &r)| [l, r]));
    }
    out.drain(..LEAD_FRAMES as usize);
    out.truncate(frames);
    out
}

/// Sum pre-rendered buffers into a zeroed buffer of `frames` frames at their
/// start offsets; anything past the end is dropped.
fn place(buffers: &[(u64, &[[f32; 2]])], frames: usize) -> Vec<[f32; 2]> {
    let mut out = vec![[0.0f32; 2]; frames];
    for (start, samples) in buffers {
        let start = *start as usize;
        for (slot, s) in out.iter_mut().skip(start).zip(samples.iter()) {
            slot[0] += s[0];
            slot[1] += s[1];
        }
    }
    out
}

/// Write stereo frames; returns the unclamped peak.
fn write_wav(path: &Path, frames: &[[f32; 2]], depth: BitDepth) -> Result<f32, String> {
    let peak = frames
        .iter()
        .flat_map(|f| f.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let mut writer = hound::WavWriter::create(path, depth.spec())
        .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;
    let mut samples = frames.iter().flat_map(|f| f.iter().copied());
    let result: Result<(), hound::Error> = match depth {
        BitDepth::Float32 => samples.try_for_each(|v| writer.write_sample(v)),
        BitDepth::Int16 => samples
            .try_for_each(|v| writer.write_sample((v.clamp(-1.0, 1.0) * 32_767.0).round() as i16)),
        BitDepth::Int24 => samples.try_for_each(|v| {
            writer.write_sample((v.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
        }),
    };
    result
        .and_then(|()| writer.finalize())
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(peak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::SimpleNote;
    use std::path::Path;

    /// A fresh, empty directory under the system temp dir, unique per test.
    pub(super) fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mcp-muse-export-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A sine patch note with a short release and no effects.
    pub(super) fn sine_note(start: f64, duration: f64, level: f32) -> SimpleNote {
        SimpleNote {
            note: Some(69),
            velocity: Some(100),
            start_time: Some(start),
            duration: Some(duration),
            synth: Some(
                serde_json::from_value(serde_json::json!({
                    "name": format!("sine_{}", (level * 100.0) as u32),
                    "level": level,
                    "subtractive": {"osc1": {"wave": "sine"},
                        "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}
                }))
                .unwrap(),
            ),
            ..Default::default()
        }
    }

    pub(super) fn midi_note(channel: u8, key: u8, instrument: Option<u8>) -> SimpleNote {
        SimpleNote {
            note: Some(key),
            velocity: Some(100),
            duration: Some(1.0),
            channel,
            instrument,
            ..Default::default()
        }
    }

    pub(super) fn request(
        notes: Vec<SimpleNote>,
        dir: &Path,
        name: &str,
        split: Split,
        bit_depth: BitDepth,
    ) -> ExportRequest {
        ExportRequest {
            sequence: SimpleSequence {
                notes,
                tempo: 120,
                beats_per_bar: 4,
            },
            dir: dir.to_path_buf(),
            name: name.to_string(),
            split,
            bit_depth,
            overwrite: false,
        }
    }

    /// Read a WAV back as normalised stereo frames plus its spec.
    pub(super) fn read_wav(path: &Path) -> (hound::WavSpec, Vec<[f32; 2]>) {
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        let mono: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => {
                let full = ((1u64 << (spec.bits_per_sample - 1)) - 1) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / full)
                    .collect()
            }
        };
        (spec, mono.chunks(2).map(|c| [c[0], c[1]]).collect())
    }

    pub(super) fn rms_left(frames: &[[f32; 2]], from_s: f64, to_s: f64) -> f32 {
        let a = (from_s * SAMPLE_RATE as f64) as usize;
        let b = ((to_s * SAMPLE_RATE as f64) as usize).min(frames.len());
        let left: Vec<f32> = frames[a..b].iter().map(|f| f[0]).collect();
        crate::expressive::test_util::rms(&left)
    }

    #[test]
    fn sanitize_name_keeps_letters_digits_underscore_and_dash() {
        assert_eq!(sanitize_name(" My Mix/01! "), "My_Mix_01_");
        assert_eq!(sanitize_name("take-2_final"), "take-2_final");
        assert_eq!(sanitize_name("   "), "");
    }

    #[test]
    fn bit_depth_parses_16_24_and_32_only() {
        assert_eq!(BitDepth::from_bits(16).unwrap(), BitDepth::Int16);
        assert_eq!(BitDepth::from_bits(24).unwrap(), BitDepth::Int24);
        assert_eq!(BitDepth::from_bits(32).unwrap(), BitDepth::Float32);
        assert!(BitDepth::from_bits(8).unwrap_err().contains("bit_depth"));
    }

    #[test]
    fn a_mixdown_is_silent_before_the_note_and_sounds_after_it() {
        let dir = tmp_dir("mixdown").join("nested");
        let report = export_with(
            request(
                vec![sine_note(0.5, 0.5, 0.8)],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("no soundfont".into()),
        )
        .unwrap();

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, dir.join("mix.wav"));
        let (spec, frames) = read_wav(&report.files[0].path);
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, SAMPLE_RATE);
        assert_eq!(spec.bits_per_sample, 24);
        let expected = (report.duration.as_secs_f64() * SAMPLE_RATE as f64).ceil() as usize;
        assert_eq!(
            frames.len(),
            expected,
            "file length is the reported duration"
        );
        assert_eq!(rms_left(&frames, 0.0, 0.45), 0.0, "silence before the note");
        assert!(
            rms_left(&frames, 0.55, 0.95) > 0.05,
            "energy during the note"
        );
        assert!(report.files[0].peak > 0.1);
    }

    #[test]
    fn bit_depths_set_the_wav_spec() {
        let dir = tmp_dir("depths");
        for (depth, bits, format) in [
            (BitDepth::Int16, 16, hound::SampleFormat::Int),
            (BitDepth::Int24, 24, hound::SampleFormat::Int),
            (BitDepth::Float32, 32, hound::SampleFormat::Float),
        ] {
            let report = export_with(
                request(
                    vec![sine_note(0.0, 0.1, 0.5)],
                    &dir,
                    &format!("d{}", bits),
                    Split::Mixdown,
                    depth,
                ),
                &HashMap::new(),
                Err("no soundfont".into()),
            )
            .unwrap();
            let (spec, frames) = read_wav(&report.files[0].path);
            assert_eq!(spec.bits_per_sample, bits);
            assert_eq!(spec.sample_format, format);
            assert!(rms_left(&frames, 0.0, 0.1) > 0.02);
        }
    }

    #[test]
    fn existing_files_are_not_overwritten_unless_asked() {
        let dir = tmp_dir("overwrite");
        let req = || {
            request(
                vec![sine_note(0.0, 0.1, 0.5)],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int16,
            )
        };
        export_with(req(), &HashMap::new(), Err("x".into())).unwrap();
        let err = export_with(req(), &HashMap::new(), Err("x".into())).unwrap_err();
        assert!(err.contains("overwrite"), "{}", err);
        assert!(err.contains("mix.wav"), "{}", err);
        let mut again = req();
        again.overwrite = true;
        export_with(again, &HashMap::new(), Err("x".into())).unwrap();
    }

    #[test]
    fn a_dir_that_is_already_a_regular_file_is_reported_not_a_directory() {
        let dir = tmp_dir("notadir");
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::write(&dir, b"not a directory").unwrap();
        let err = export_with(
            request(
                vec![sine_note(0.0, 0.1, 0.5)],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap_err();
        assert!(err.contains("not a directory"), "{}", err);
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn midi_notes_need_a_soundfont_and_nothing_is_written_without_one() {
        let dir = tmp_dir("nosf");
        let err = export_with(
            request(
                vec![midi_note(0, 60, Some(0))],
                &dir,
                "mix",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("SoundFont not found".into()),
        )
        .unwrap_err();
        assert!(err.contains("SoundFont"), "{}", err);
        assert!(!dir.exists(), "no directory is created on failure");
    }

    #[test]
    fn a_name_that_is_empty_after_trimming_is_rejected() {
        // Only whitespace sanitizes to nothing; "!!!" becomes "___" and is allowed.
        let dir = tmp_dir("noname");
        let err = export_with(
            request(
                vec![sine_note(0.0, 0.1, 0.5)],
                &dir,
                "   ",
                Split::Mixdown,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap_err();
        assert!(err.contains("name"), "{}", err);
    }

    fn parts_with(
        midi: Vec<crate::midi::parser::MidiNote>,
        patch_names: &[&str],
        r2d2: bool,
    ) -> TranslatedParts {
        TranslatedParts {
            midi,
            external: Vec::new(),
            midi_effects: None,
            patches: patch_names
                .iter()
                .map(|n| crate::midi::translate::PatchRender {
                    name: n.to_string(),
                    start: 0,
                    samples: vec![[0.1, 0.1]; 10],
                })
                .collect(),
            r2d2: if r2d2 {
                vec![(0, vec![[0.2, 0.2]; 10])]
            } else {
                Vec::new()
            },
            tempo: 120,
            duration: Duration::from_secs(1),
        }
    }

    fn parsed_midi(
        channel: u8,
        instrument: Option<u8>,
        start: f64,
    ) -> crate::midi::parser::MidiNote {
        crate::midi::parser::MidiNote {
            note: 60,
            velocity: 100,
            channel,
            start_time: Duration::from_secs_f64(start),
            duration: Duration::from_secs(1),
            instrument,
            reverb: None,
            chorus: None,
            volume: None,
            pan: None,
            balance: None,
            expression: None,
            sustain: None,
        }
    }

    #[test]
    fn sources_are_named_after_channel_program_patch_and_r2d2() {
        let parts = parts_with(
            vec![
                parsed_midi(3, Some(73), 1.0),
                parsed_midi(3, Some(0), 2.0),
                parsed_midi(0, None, 0.0),
                parsed_midi(9, None, 0.0),
            ],
            &["blip", "blip", "tr_808_kick"],
            true,
        );
        let names: Vec<String> = sources(&parts).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec![
                "ch00_acoustic_grand_piano",
                "ch03_flute",
                "ch09_drums",
                "synth_blip",
                "synth_blip_2",
                "synth_tr_808_kick",
                "r2d2",
            ]
        );
    }

    #[test]
    fn stems_sum_to_the_mixdown_and_share_its_length() {
        let dir = tmp_dir("stems");
        let mut a = sine_note(0.0, 0.4, 0.3);
        let b = sine_note(0.2, 0.4, 0.3);
        // Two different patches so they become two stems.
        a.synth = Some(
            serde_json::from_value(serde_json::json!({
                "name": "square_quiet", "level": 0.3,
                "subtractive": {"osc1": {"wave": "square"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}}
            }))
            .unwrap(),
        );
        let notes = || vec![a.clone(), b.clone()];
        let mix = export_with(
            request(notes(), &dir, "mix", Split::Mixdown, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let stems = export_with(
            request(notes(), &dir, "stems", Split::Stems, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();

        assert_eq!(stems.files.len(), 2);
        assert_eq!(stems.duration, mix.duration);
        let names: Vec<String> = stems
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["synth_square_quiet.wav", "synth_sine_30.wav"]);
        assert!(stems.files[0].path.starts_with(dir.join("stems")));

        let (_, mixed) = read_wav(&mix.files[0].path);
        let (_, s0) = read_wav(&stems.files[0].path);
        let (_, s1) = read_wav(&stems.files[1].path);
        assert_eq!(s0.len(), mixed.len());
        assert_eq!(s1.len(), mixed.len());
        // Peaks stay below the soft-clip knee, so the mixdown is the plain sum.
        assert!(mix.files[0].peak < 0.8);
        let worst = mixed
            .iter()
            .zip(&s0)
            .zip(&s1)
            .map(|((m, x), y)| {
                (m[0] - (x[0] + y[0]))
                    .abs()
                    .max((m[1] - (x[1] + y[1])).abs())
            })
            .fold(0.0f32, f32::max);
        assert!(
            worst < 1e-5,
            "stems must sum to the mixdown, worst diff {}",
            worst
        );
    }

    #[test]
    fn r2d2_notes_become_one_stem() {
        let dir = tmp_dir("r2d2");
        let r2d2 = SimpleNote {
            note_type: "r2d2".to_string(),
            r2d2_emotion: Some("Happy".to_string()),
            r2d2_intensity: Some(0.7),
            r2d2_complexity: Some(2),
            duration: Some(0.5),
            ..Default::default()
        };
        let report = export_with(
            request(
                vec![r2d2.clone(), r2d2],
                &dir,
                "beeps",
                Split::Stems,
                BitDepth::Int24,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, dir.join("beeps").join("r2d2.wav"));
        let (_, frames) = read_wav(&report.files[0].path);
        assert!(rms_left(&frames, 0.0, 0.4) > 0.01);
    }

    #[test]
    fn tracks_bypass_the_patch_effect_chain_but_keep_the_stem_length() {
        let dir = tmp_dir("tracks");
        let mut note = sine_note(0.0, 0.2, 0.8);
        note.synth = Some(
            serde_json::from_value(serde_json::json!({
                "name": "echo", "level": 0.8,
                "subtractive": {"osc1": {"wave": "sine"},
                    "env": {"attack": 0.001, "decay": 0.001, "sustain": 1.0, "release": 0.01}},
                "effects": [{"type": "delay", "delay_time": 0.5, "feedback": 0.5, "intensity": 0.8}]
            }))
            .unwrap(),
        );
        let wet = export_with(
            request(
                vec![note.clone()],
                &dir,
                "wet",
                Split::Stems,
                BitDepth::Float32,
            ),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let dry = export_with(
            request(vec![note], &dir, "dry", Split::Tracks, BitDepth::Float32),
            &HashMap::new(),
            Err("x".into()),
        )
        .unwrap();
        let (_, wet_frames) = read_wav(&wet.files[0].path);
        let (_, dry_frames) = read_wav(&dry.files[0].path);
        assert_eq!(
            dry_frames.len(),
            wet_frames.len(),
            "tracks are padded to the wet length"
        );
        // The first echo lands at 0.5 s; a dry track has nothing there.
        assert!(
            rms_left(&wet_frames, 1.0, 1.2) > 1e-3,
            "the stem carries the delay repeats"
        );
        assert!(rms_left(&dry_frames, 1.0, 1.2) < 1e-5, "the track does not");
        assert!(
            rms_left(&dry_frames, 0.02, 0.18) > 0.05,
            "the track still has the note"
        );
    }

    #[test]
    fn midi_channels_become_separate_stems() {
        let Ok(soundfont) = find_soundfont() else {
            eprintln!("skipping: SoundFont not installed (run `mcp-muse setup`)");
            return;
        };
        let dir = tmp_dir("channels");
        let report = export_with(
            request(
                vec![
                    midi_note(0, 76, Some(73)), // flute E5, 659.26 Hz
                    midi_note(1, 67, Some(73)), // flute G4, 392.00 Hz
                    midi_note(9, 38, None),     // snare on the drum channel
                ],
                &dir,
                "band",
                Split::Stems,
                BitDepth::Float32,
            ),
            &HashMap::new(),
            Ok(soundfont),
        )
        .unwrap();
        let names: Vec<String> = report
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["ch00_flute.wav", "ch01_flute.wav", "ch09_drums.wav"]
        );

        use crate::expressive::test_util::goertzel_power;
        let window = |frames: &[[f32; 2]]| -> Vec<f32> {
            let a = (0.1 * SAMPLE_RATE as f64) as usize;
            let b = (0.9 * SAMPLE_RATE as f64) as usize;
            frames[a..b].iter().map(|f| f[0]).collect()
        };
        let (_, ch0) = read_wav(&report.files[0].path);
        let (_, ch1) = read_wav(&report.files[1].path);
        let (e5, g4) = (659.26, 392.0);
        let sr = SAMPLE_RATE as f32;
        let ch0 = window(&ch0);
        let ch1 = window(&ch1);
        assert!(
            goertzel_power(&ch0, e5, sr) > 10.0 * goertzel_power(&ch0, g4, sr),
            "channel 0 carries only its own pitch"
        );
        assert!(
            goertzel_power(&ch1, g4, sr) > 10.0 * goertzel_power(&ch1, e5, sr),
            "channel 1 carries only its own pitch"
        );
        let (_, drums) = read_wav(&report.files[2].path);
        assert!(
            rms_left(&drums, 0.0, 0.2) > 0.01,
            "the snare hit is on the drum stem"
        );
    }

    #[test]
    fn dedupe_suffixes_never_collide_with_a_real_name() {
        let mut sources: Vec<(String, Source)> = vec![
            ("synth_blip".to_string(), Source::Patch(0)),
            ("synth_blip".to_string(), Source::Patch(1)),
            ("synth_blip_2".to_string(), Source::Patch(2)),
        ];
        dedupe_names(&mut sources);
        let names: Vec<&str> = sources.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["synth_blip", "synth_blip_2", "synth_blip_2_2"]);
        let distinct: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(distinct.len(), names.len(), "all names must be distinct");
    }

    fn seq(notes: Vec<SimpleNote>) -> SimpleSequence {
        SimpleSequence {
            notes,
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    fn with_effects(mut note: SimpleNote, effects: Vec<serde_json::Value>) -> SimpleNote {
        note.effects = Some(
            effects
                .into_iter()
                .map(|v| serde_json::from_value(v).unwrap())
                .collect(),
        );
        note
    }

    #[test]
    fn bus_chain_is_nonlinear_is_false_with_no_notes_or_a_linear_chain() {
        assert!(
            !bus_chain_is_nonlinear(&seq(Vec::new())),
            "no MIDI notes at all"
        );
        assert!(
            !bus_chain_is_nonlinear(&seq(vec![midi_note(0, 60, Some(0))])),
            "no effects"
        );
        let reverb = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "reverb"})],
        );
        assert!(
            !bus_chain_is_nonlinear(&seq(vec![reverb])),
            "reverb commutes with summing"
        );
        let plain_delay = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "delay", "delay_time": 0.3})],
        );
        assert!(
            !bus_chain_is_nonlinear(&seq(vec![plain_delay])),
            "a plain delay (no Time Fracture) commutes with summing"
        );
    }

    #[test]
    fn bus_chain_is_nonlinear_is_true_for_compressor_distortion_and_fracture_delay() {
        let compressor = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "compressor"})],
        );
        assert!(bus_chain_is_nonlinear(&seq(vec![compressor])));

        let distortion = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "distortion"})],
        );
        assert!(bus_chain_is_nonlinear(&seq(vec![distortion])));

        let random_beats_delay = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "delay", "random_beats": [0.5, 1.0]})],
        );
        assert!(bus_chain_is_nonlinear(&seq(vec![random_beats_delay])));

        let pitch_fracture_delay = with_effects(
            midi_note(0, 60, Some(0)),
            vec![serde_json::json!({"type": "delay", "pitch_intervals": [3.0, 7.0]})],
        );
        assert!(bus_chain_is_nonlinear(&seq(vec![pitch_fracture_delay])));
    }

    #[test]
    fn bus_chain_is_nonlinear_expands_effects_preset() {
        // "studio" (src/expressive/effects_presets.rs) opens with a compressor.
        let mut note = midi_note(0, 60, Some(0));
        note.effects_preset = Some("studio".to_string());
        assert!(bus_chain_is_nonlinear(&seq(vec![note])));
    }
}
