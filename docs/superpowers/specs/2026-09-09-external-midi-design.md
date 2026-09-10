# Playing the machine's own instruments over MIDI

Date: 2026-09-09
Status: Implemented on branch external-midi (2026-09-09) under the assumptions below; user review pending

## Goal

Let the calling agent play instruments that live outside this server:
the devices inside a DAW such as Bitwig Studio, a software synth, or a
hardware synth on a MIDI interface. The agent should be able to discover
what is reachable on this machine and address it from the same note array
it already uses for General MIDI, synth patches and R2D2.

## The constraint that shapes the design

Bitwig's instruments (Polymer, Phase-4, the Grid and the rest) are not
plugins. No process other than Bitwig can load them. The same is true of
every DAW's built-in devices. The only way to play them from outside is
to send MIDI into the DAW and let it produce the audio.

Third-party plugins in `/Library/Audio/Plug-Ins` (VST3, AU, CLAP) could
in principle be hosted inside this process. That is a separate and much
larger project: one FFI host per format, plugin crashes taking the server
down with them, and it would still not reach the DAW's own devices. It
is left as a follow-up (see "Not in scope").

So this feature is **external MIDI output**: the server publishes a
virtual MIDI port that any DAW can select as an input, can also send to
any MIDI destination already on the machine, and lets a note say which
one it wants.

## Decisions taken

- **A `midi_out` field on the note.** `"midi_out": "<port name>"` sends
  that note as MIDI to the named output instead of the internal
  SoundFont synth. It is per note, like `synth` and `instrument`, so one
  call can mix internal drums with a lead played by Bitwig. Absent means
  the internal synth, as today. (Alternative rejected: a per-sequence
  field, which would force one call per destination.)
- **The server publishes a virtual port named `mcp-muse`.** On macOS and
  Linux it exists from server start until exit, so it is already there
  when the user opens the DAW's MIDI settings. Windows has no virtual
  ports without a driver; there the catalog says so and points at
  loopMIDI. Existing destinations (an IAC bus, a hardware interface,
  a DAW's own virtual input) are opened on first use and kept open for
  the session. (Alternative rejected: relying on the IAC driver, which
  the user has to enable by hand and which does not exist on Linux.)
- **Discovery through `list_sounds`.** A new `midi_outputs` section
  lists the virtual port and every destination CoreMIDI/ALSA reports,
  with the one-time DAW setup steps. No plugin folder scanning: listing
  plugins the server cannot load would only mislead the agent.
- **External events ride the engine's sample clock.** They are scheduled
  in the same event heap as internal MIDI events and applied at their
  exact frame; the audio thread forwards each one over a channel to a
  small sender thread that owns the `midir` connections. Internal and
  external notes therefore stay aligned to within the audio device's
  output latency, and replace, layer, and `stop_playback` keep their
  meaning without a second scheduler. (Alternative rejected: a wall-clock
  scheduler thread, which would be a second clock to keep in step and a
  second set of replace/stop semantics.)
- **Replace and stop silence the outside too.** Every reset sends CC 123
  (all notes off) and CC 120 (all sound off) on all sixteen channels of
  every open port, so a DAW never keeps a note hanging. The sender
  thread does the same when the server exits.
- **External notes are never remapped.** The layer-mode channel
  allocation (issue #98) exists because OxiSynth shares sixteen channels
  across playbacks. A DAW routes by channel, so an external note's
  `channel` is what reaches the port, always.
- **No default program change.** For the internal synth, the first note
  on a channel sends program 0 when none is given, so an unspecified
  instrument is still piano. Sent to a hardware synth, that would switch
  its preset. External notes send a program change only when
  `instrument` is set.
- **No effects on external notes.** The audio is made elsewhere, so
  `effects` and `effects_preset` on a `midi_out` note are a parameter
  error (like on a synth note), not a silent drop. `reverb`, `chorus`,
  `volume`, `pan`, `balance`, `expression` and `sustain` are plain MIDI
  controllers and go out as such.
- **Export refuses external notes.** `export_audio` renders offline and
  has no way to capture the DAW; a `midi_out` note in an export is a
  tool error naming the note, so the agent knows to leave it out.

## 1. Tool surface

Note schema gains:

```json
"midi_out": {
  "type": "string",
  "description": "Send this note as MIDI to an output on this machine instead of the built-in synth: \"mcp-muse\" (this server's virtual port; select it as a MIDI input in Bitwig or another DAW) or a destination name from list_sounds section \"midi_outputs\". The note's channel selects the DAW track. instrument is sent as a program change only when given; effects are not available."
}
```

`list_sounds` gains a section `midi_outputs`:

```
# MIDI outputs — use "midi_out": "<name>" on a note
- mcp-muse — this server's virtual port (always available while the server runs)
- IAC Driver Bus 1

Bitwig: Settings > Controllers > Add Controller > Generic > "MIDI Keyboard",
MIDI input mcp-muse. Notes then play on the selected/armed instrument track;
to address several tracks, set each track's input chooser to mcp-muse and one
channel, and arm them all.
```

Playback responses mention the outputs used: `MIDI instruments + external
MIDI (mcp-muse)`.

A `midi_out` naming nothing on the machine is a runtime error (`isError`)
listing what exists. A name matches case-insensitively: exact first, then
a unique substring (so `"iac"` finds `IAC Driver Bus 1`); an ambiguous
substring is an error listing the candidates.

## 2. Components

### `src/midi/external.rs` (new)

- `pub struct ExternalMidi` — lives on the tool thread, owned by
  `ServerState`. Holds the `Sender<ExternalMessage>` for the sender
  thread and the list of opened port names (`Vec<String>`, index =
  `PortId`). `new()` spawns the thread and creates the virtual port on
  unix; a failure to create it is logged and reported by the catalog,
  never fatal.
- `pub fn outputs(&self) -> Outputs` — enumerates destinations now
  (excluding the server's own virtual port, which CoreMIDI also lists as
  a destination) plus the virtual port's status.
- `pub fn resolve(&mut self, name: &str) -> Result<PortId, String>` —
  `resolve_name` (pure, tested) over opened names then available
  destinations; opens a destination on first use and hands the
  connection to the sender thread.
- `pub fn sender(&self) -> ExternalSender` — a cloneable handle the
  engine keeps.
- `pub fn encode(kind: &EventKind) -> ([u8; 3], usize)` — MIDI bytes for
  note on/off, control change, program change (pure, tested).
- Sender thread: `loop { match rx.recv() { Send{port, bytes} => conn.send, AllNotesOff => for every port, 16 channels × CC 123, CC 120, Open(id, conn) => push } }`.
  It exits when every sender is dropped, after sending all notes off.

### `src/midi/engine.rs`

- `PlayCommand.external: Vec<(u64, PortId, EventKind)>`.
- The heap entry gains `target: Target` (`Synth` or `External(PortId)`).
  `apply_event` matches on it: `Synth` goes to OxiSynth as now,
  `External` encodes and sends through the `ExternalSender` if one is
  set. `MidiEngine::with_external(sender)` installs it; tests use a
  bare channel and read what arrives.
- `schedule` pushes external events untouched by `allocate_channels`.
- `silence` sends `AllNotesOff` when a sender is set.

### `src/midi/translate.rs`

- `TranslatedParts.external: Vec<(PortId, Vec<MidiNote>)>`, grouped by
  resolved port; `into_command` runs `midi_events` per group with
  `default_program: None`.
- `translate_parts` takes an `Option<&mut ExternalMidi>`. Playback passes
  the server's; export passes `None` and gets an error for any
  `midi_out` note.
- The SoundFont check applies only to notes bound for the internal synth.
- Duration: external notes count toward `note_end` like internal ones,
  so the reported duration and `playback_ends` cover them.

### `src/midi/player.rs` and `src/server/mcp.rs`

- `MidiPlayer::new(sender: ExternalSender)`; `play(..., &mut ExternalMidi)`.
- `ServerState` gains `external: ExternalMidi`, created in `new()`.
- `handle_list_sounds` renders the section from `outputs()`.
- `validate_notes` adds the effects-on-`midi_out` check.

### `src/midi/mod.rs`

- `SimpleNote.midi_out: Option<String>`; `validate_effects` rejects
  effects when `midi_out` is set; `validate_synth` rejects `synth`
  together with `midi_out`.

## 3. Data flow

```
play_notes {midi_out: "mcp-muse", channel: 2, note: 60}
  → validate (no effects/synth on a midi_out note)
  → Translator: resolve "mcp-muse" → PortId 0; MidiNote into external[0]
  → PlayCommand.external = midi_events(group, default_program: None)
  → engine.schedule: heap entries with Target::External(0), no remap
  → audio thread reaches the frame: encode → ExternalSender.send
  → sender thread: MidiOutputConnection::send([0x92, 60, vel])
  → CoreMIDI → Bitwig track listening to mcp-muse channel 3 (1-based)
```

## 4. Error handling

- No virtual port (Windows, or CoreMIDI refused): catalog says so; a
  note naming `mcp-muse` gets the runtime error with the reason.
- Unknown or ambiguous `midi_out`: runtime error listing the outputs.
- Port disappears mid-session (interface unplugged): `send` fails, the
  sender thread logs at warn once per port and drops that connection;
  the next `resolve` reopens it if it is back.
- Sender channel gone (thread panicked): the engine drops external
  events; playback of everything else continues.

## 5. Testing

Pure, no device: name resolution, byte encoding, translator grouping
(no default program change, no SoundFont needed, effects rejected,
export refuses), engine forwarding at the exact frame through a plain
channel, replace and stop sending all-notes-off, layer leaving external
channels alone, schema and catalog contents.

With a MIDI backend (unix, skipped if the backend fails to initialise):
create the virtual port and see it in the destination list of a second
`MidiOutput` client.

Listen-by-ear: a demo `cargo run -- test-midi-out [name]` plays a scale to
the named output (default `mcp-muse`) for checking a DAW routing.

## Not in scope

- Hosting VST3/AU/CLAP plugins inside the server. Worth a spike with
  the `clack` (CLAP) crate or AudioToolbox on macOS if in-process audio
  from third-party instruments is wanted; it would not reach DAW-native
  devices.
- Recording the DAW's audio back into `export_audio`.
- MIDI input, MIDI clock or transport control of the DAW.
- A configurable latency offset between internal and external notes.
