//! Owns the audio output for the process and the one `MidiEngine` running on
//! it. Tool calls translate sequences into engine commands; playback state
//! lives on the audio thread.
use crate::expressive::Patch;
use crate::midi::SimpleSequence;
use crate::midi::engine::{
    EngineCommand, EngineHandle, EngineSource, LEAD_FRAMES, MidiEngine, PlayMode, find_soundfont,
    load_synth, seconds_to_frames,
};
use crate::midi::external::{ExternalMidi, ExternalSender};
use crate::midi::translate::{Translation, Translator};
use rodio::MixerDeviceSink;
use std::collections::HashMap;
use std::time::Duration;

pub struct MidiPlayer {
    /// Kept alive for the process; dropping it closes the device.
    _stream: MixerDeviceSink,
    engine: EngineHandle,
    translator: Translator,
    /// Engine frame at which each started playback ends (including tail).
    playback_ends: Vec<u64>,
}

impl MidiPlayer {
    /// Open the output device, load the SoundFont once, and attach the engine
    /// to the mixer. Without a SoundFont, synthesis and R2D2 still work.
    /// `external` lets the engine forward `midi_out` notes to ports on the
    /// machine; without it they are dropped.
    pub fn new(external: Option<ExternalSender>) -> Result<Self, String> {
        let stream = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("Failed to create audio output stream: {}", e))?;

        let (synth, midi_available) = match find_soundfont().and_then(|p| load_synth(&p)) {
            Ok(synth) => (Some(synth), Ok(())),
            Err(reason) => {
                tracing::warn!("MIDI unavailable: {}", reason);
                (None, Err(reason))
            }
        };
        let (mut engine, handle) = MidiEngine::new(synth);
        if let Some(sender) = external {
            engine.set_external(sender);
        }
        stream.mixer().add(EngineSource::new(engine));

        Ok(MidiPlayer {
            _stream: stream,
            engine: handle,
            translator: Translator::new(midi_available),
            playback_ends: Vec::new(),
        })
    }

    /// Schedule a sequence with no external outputs (demos and tests).
    pub fn play(
        &mut self,
        sequence: SimpleSequence,
        mode: PlayMode,
        session_patches: &HashMap<String, Patch>,
    ) -> Result<Duration, String> {
        self.play_with(sequence, mode, session_patches, None)
    }

    /// Schedule a sequence. Returns the time until it finishes, including
    /// effect tails. `Replace` cuts whatever is playing first. `external`
    /// resolves the notes' `midi_out` names; without it they are an error.
    pub fn play_with(
        &mut self,
        sequence: SimpleSequence,
        mode: PlayMode,
        session_patches: &HashMap<String, Patch>,
        external: Option<&mut ExternalMidi>,
    ) -> Result<Duration, String> {
        let now = self.engine.clock();
        self.playback_ends.retain(|&end| end > now);
        let Translation { command, duration } = match external {
            Some(external) => self.translator.translate_with(
                sequence,
                mode,
                session_patches,
                Some(&mut |name: &str| external.resolve(name)),
            )?,
            None => self.translator.translate(sequence, mode, session_patches)?,
        };
        if command.events.is_empty() && command.external.is_empty() && command.buffers.is_empty() {
            tracing::warn!("Nothing to play");
            if mode == PlayMode::Layer {
                return Ok(Duration::ZERO);
            }
            // A `Replace` with nothing in it still means "silence what is
            // playing": the engine stops on the empty command's mode.
            self.playback_ends.clear();
            self.engine.send(EngineCommand::Play(command))?;
            return Ok(Duration::ZERO);
        }
        if mode == PlayMode::Replace {
            self.playback_ends.clear();
        }
        // Re-read the clock after translation: translating can take long
        // enough (pre-rendering synth/R2D2 buffers) that the earlier read
        // would under-count the end frame and let the prune above drop this
        // playback too soon.
        let now = self.engine.clock();
        self.engine.send(EngineCommand::Play(command))?;
        self.playback_ends
            .push(now + LEAD_FRAMES + seconds_to_frames(duration));
        tracing::info!(
            "Playback started ({}) - duration: {:.2}s, active playbacks: {}",
            mode.as_str(),
            duration.as_secs_f64(),
            self.playback_ends.len()
        );
        Ok(duration)
    }

    /// Stop everything. Returns how many started playbacks had not finished.
    pub fn stop_all(&mut self) -> usize {
        let active = count_active(&self.playback_ends, self.engine.clock());
        self.playback_ends.clear();
        if let Err(e) = self.engine.send(EngineCommand::Stop) {
            tracing::warn!("Stop failed: {}", e);
        }
        tracing::info!("Stopped {} active playbacks", active);
        active
    }

    /// Number of started playbacks that have not reached their end frame.
    #[allow(dead_code)] // public API kept for callers outside the tool layer (the demos and future tools)
    pub fn active_playbacks(&mut self) -> usize {
        let now = self.engine.clock();
        self.playback_ends.retain(|&end| end > now);
        self.playback_ends.len()
    }
}

pub(crate) fn count_active(ends: &[u64], now: u64) -> usize {
    ends.iter().filter(|&&end| end > now).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_active_counts_playbacks_whose_end_is_in_the_future() {
        assert_eq!(count_active(&[], 10), 0);
        assert_eq!(count_active(&[5, 10, 11, 500], 10), 2);
    }
}

#[cfg(test)]
mod level_tests {
    //! Headroom checks: typical material must stay clear of the soft-clip knee.
    use super::*;
    use crate::midi::SimpleNote;

    /// Peak and fraction of samples above the clipper knee, before clipping.
    fn measure(seq: SimpleSequence) -> (f32, f32) {
        use crate::midi::engine::{CHUNK_FRAMES, EngineCommand, LEAD_FRAMES, PlayMode};
        use crate::midi::translate::Translator;
        let (mut engine, _handle) =
            crate::midi::engine::tests::engine_with_soundfont().expect("caller checked");
        let translation = Translator::new(Ok(()))
            .translate(seq, PlayMode::Replace, &HashMap::new())
            .unwrap();
        engine.apply(EngineCommand::Play(translation.command));

        // The engine's lead-in is silence; measuring it would dilute the
        // statistics, so only frames in [LEAD_FRAMES, LEAD_FRAMES + 1.5 s) count.
        let total = LEAD_FRAMES as usize + 66_150; // 1.5 s of material
        let (mut l, mut r) = (vec![0.0f32; CHUNK_FRAMES], vec![0.0f32; CHUNK_FRAMES]);
        let (mut peak, mut over, mut count) = (0.0f32, 0usize, 0usize);
        let mut rendered = 0;
        while rendered < total {
            engine.render_unclipped(&mut l, &mut r);
            for (i, (left, right)) in l.iter().zip(r.iter()).enumerate() {
                let frame = rendered + i;
                if frame < LEAD_FRAMES as usize || frame >= total {
                    continue;
                }
                for v in [left, right] {
                    peak = peak.max(v.abs());
                    if v.abs() > 0.8 {
                        over += 1;
                    }
                    count += 1;
                }
            }
            rendered += CHUNK_FRAMES;
        }
        (peak, over as f32 / count as f32)
    }

    fn midi(notes: &[(u8, u8, u8, Option<u8>)]) -> SimpleSequence {
        SimpleSequence {
            notes: notes
                .iter()
                .map(|&(note, vel, ch, inst)| SimpleNote {
                    note: Some(note),
                    velocity: Some(vel),
                    channel: ch,
                    instrument: inst,
                    ..Default::default()
                })
                .collect(),
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    fn patch(name: &str, notes: &[u8]) -> SimpleSequence {
        SimpleSequence {
            notes: notes
                .iter()
                .map(|&n| SimpleNote {
                    synth: Some(crate::expressive::SynthRef::Name(name.to_string())),
                    note: Some(n),
                    velocity: Some(100),
                    duration: Some(1.4),
                    ..Default::default()
                })
                .collect(),
            tempo: 120,
            beats_per_bar: 4,
        }
    }

    #[test]
    fn typical_material_stays_below_the_clipper_knee() {
        if find_soundfont().is_err() {
            eprintln!("skipping: SoundFont not installed");
            return;
        }
        let cases: Vec<(&str, SimpleSequence, f32)> = vec![
            ("flute", midi(&[(76, 100, 0, Some(73))]), 0.6),
            (
                "piano chord",
                midi(&[
                    (60, 90, 0, Some(0)),
                    (64, 90, 0, Some(0)),
                    (67, 90, 0, Some(0)),
                    (72, 90, 0, Some(0)),
                ]),
                0.8,
            ),
            (
                "strings chord",
                midi(&[
                    (60, 90, 0, Some(48)),
                    (64, 90, 0, Some(48)),
                    (67, 90, 0, Some(48)),
                    (72, 90, 0, Some(48)),
                ]),
                0.8,
            ),
            (
                "drums",
                midi(&[(36, 110, 9, None), (38, 110, 9, None), (42, 110, 9, None)]),
                1.0,
            ),
            ("minimoog bass", patch("minimoog_bass", &[36]), 0.6),
            (
                "jp8 strings chord",
                patch("jp_8_strings", &[60, 64, 67]),
                0.8,
            ),
            ("tr808 kick", patch("tr_808_kick", &[36]), 0.6),
        ];
        for (name, seq, max_peak) in cases {
            let (peak, over) = measure(seq);
            eprintln!(
                "LEVEL {name:20} peak={peak:.2} above_knee={:.1}%",
                over * 100.0
            );
            assert!(peak < max_peak, "{name}: peak {peak} exceeds {max_peak}");
            assert!(
                over < 0.01,
                "{name}: {:.1}% of samples in the clipper",
                over * 100.0
            );
        }
    }

    #[test]
    fn material_is_not_too_quiet_either() {
        if find_soundfont().is_err() {
            return;
        }
        let (flute, _) = measure(midi(&[(76, 100, 0, Some(73))]));
        let (bass, _) = measure(patch("minimoog_bass", &[36]));
        assert!(flute > 0.15, "flute peak {flute} is too quiet");
        assert!(bass > 0.15, "bass peak {bass} is too quiet");
    }
}
