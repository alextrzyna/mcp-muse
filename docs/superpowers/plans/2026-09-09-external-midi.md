# External MIDI output — implementation plan

Spec: docs/superpowers/specs/2026-09-09-external-midi-design.md
Branch: external-midi (from main at bafa700)

Each task is test-first; run `cargo test <filter>` after each, and
`cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
before every commit.

## Task 1 — `src/midi/external.rs`: pure pieces

- `PortId(u16)`; `encode(&EventKind) -> ([u8; 3], usize)`; `resolve_name(name, opened: &[String], available: &[String]) -> Result<Resolved, String>` with `Resolved::Opened(PortId) | Resolved::Available(index)`.
- Tests: note on/off/CC/PC bytes and lengths, channel masking; exact,
  case-insensitive, unique substring, ambiguous (error lists
  candidates), unknown (error lists everything).

## Task 2 — sender thread and `ExternalMidi`

- `ExternalMessage { Send { port, bytes, len }, AllNotesOff, Open { port, connection } }`.
- `ExternalSender(Sender<ExternalMessage>)`, cloneable; `send` swallows a
  disconnected channel (logs at debug).
- `ExternalMidi::new()` spawns the thread; creates the virtual port on
  unix through `midir::os::unix::VirtualOutput`; `virtual_port: Result<(), String>`.
- `outputs()` enumerates with a fresh `MidiOutput::new("mcp-muse catalog")`,
  filtering out ports named `mcp-muse` (our own source shows up as a
  destination). `resolve(&mut self, name)` opens on first use.
- Thread: on `Send` failure log warn and drop that connection; on
  `AllNotesOff` send CC 123 and CC 120 on 16 channels per port; on
  channel close send AllNotesOff then exit.
- Tests (unix, skipped when `MidiOutput::new` fails): after `new()`, a
  second client's destination list contains `mcp-muse`; `outputs()`
  excludes it and reports the virtual port as available.

## Task 3 — engine forwards external events

- `PlayCommand.external: Vec<(u64, PortId, EventKind)>` (Default empty).
- `Target { Synth, External(PortId) }` on `ScheduledEvent`; `push_event(at, kind, target)`.
- `MidiEngine::set_external(ExternalSender)`; `apply_event` sends
  `encode`d bytes for `External`; `silence` sends `AllNotesOff`.
- Tests with a bare mpsc channel wrapped in `ExternalSender::for_test`:
  event at frame N arrives after the chunk covering N and not before;
  replace and stop emit AllNotesOff; a layered command's external
  channel 0 stays 0 while its synth channel 0 is remapped.

## Task 4 — data model and validation

- `SimpleNote.midi_out: Option<String>`; `validate_effects` errors when
  `midi_out` is set with `effects`/`effects_preset`; `validate_synth`
  errors when `synth` and `midi_out` are both set. `is_external()`.
- Tests in `mod.rs`.

## Task 5 — translator groups by output

- `midi_events(notes, default_program: Option<u8>)`; internal callers
  pass `Some(0)`.
- `TranslatedParts.external: Vec<(PortId, Vec<MidiNote>)>`;
  `translate_parts(..., outputs: Option<&mut ExternalMidi>)`; `None`
  errors on a `midi_out` note ("Note N: midi_out cannot be exported").
- Resolve names via a closure `&mut dyn FnMut(&str) -> Result<PortId, String>`
  so translator tests need no midir.
- SoundFont check only for internal MIDI notes; external notes extend
  `note_end`.
- Tests: grouping by port, no default program, SoundFont not required,
  export path refuses, unknown name error text includes outputs.

## Task 6 — player, server state, tool surface

- `MidiPlayer::new(sender)`; `play(sequence, mode, patches, &mut ExternalMidi)`; demos updated; `test-midi-out [name]` demo added.
- `ServerState.external`; `handle_list_sounds` `midi_outputs` section;
  note schema `midi_out`; `describe_sources` names the ports used;
  `validate_notes` picks up the new checks; export passes `None`.
- Tests: schema has `midi_out`; list_sounds section renders (with a
  stubbed `Outputs`); play_notes with an unknown `midi_out` is a tool
  error listing outputs.

## Task 7 — docs

- README: "Playing instruments in Bitwig or another DAW" section and the
  note-field table; CLAUDE.md: tool list, pipeline step, module list.
- Memory note for the follow-up (plugin hosting spike).
