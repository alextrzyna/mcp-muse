//! Offline export of a composition to WAV files: a stereo mixdown, wet stems
//! or dry tracks. Design: docs/superpowers/specs/2026-09-09-audio-export-design.md.
//!
//! Not yet wired into the `export_audio` MCP tool (a later task), so outside
//! of tests nothing in the binary calls this module yet.
#![allow(dead_code)]

use crate::expressive::Patch;
use crate::midi::SimpleSequence;
use crate::midi::engine::{
    CHUNK_FRAMES, EngineCommand, LEAD_FRAMES, MidiEngine, PlayCommand, PlayMode, SAMPLE_RATE,
    find_soundfont, load_synth, soft_clip,
};
use crate::midi::translate::{Effects, TranslatedParts, Translator};
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
    #[allow(dead_code)] // read by the split renderers in Task 3
    source: Source,
}

enum Source {
    Mixdown,
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
    let wet =
        translator.translate_parts(request.sequence.clone(), session_patches, Effects::Wet)?;
    if wet.midi.is_empty() && wet.patches.is_empty() && wet.r2d2.is_empty() {
        return Err("Nothing to export: the sequence has no notes".to_string());
    }
    let duration = wet.duration;
    let frames = (duration.as_secs_f64() * SAMPLE_RATE as f64).ceil() as usize;
    let parts = match request.split {
        Split::Tracks => {
            translator.translate_parts(request.sequence.clone(), session_patches, Effects::Dry)?
        }
        Split::Mixdown | Split::Stems => wet,
    };

    let targets = plan_targets(&request.dir, &name, request.split, &parts)?;
    check_collisions(&targets, request.overwrite)?;
    for target in &targets {
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
    }

    let synth = if parts.midi.is_empty() {
        None
    } else {
        Some(load_synth(&soundfont?)?)
    };
    let (mut engine, _handle) = MidiEngine::new(synth);

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
            return Err(format!(
                "split {} is not implemented yet",
                request.split.as_str()
            ));
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
    _parts: &TranslatedParts,
) -> Result<Vec<Target>, String> {
    match split {
        Split::Mixdown => Ok(vec![Target {
            path: dir.join(format!("{}.wav", name)),
            source: Source::Mixdown,
        }]),
        Split::Stems | Split::Tracks => {
            Err(format!("split {} is not implemented yet", split.as_str()))
        }
    }
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
}
