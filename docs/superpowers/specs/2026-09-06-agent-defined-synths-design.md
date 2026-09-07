# Agent-defined synths and effects

Date: 2026-09-06
Status: PRs 1-3 implemented; PR 4 pending

## Goal

Let the calling agent define a synthesizer **patch** as one structured JSON
object (engines, modulation, effects) and play notes through it, the way
TryX Synth (the `tryx-fx` repo) lets Gemini design sounds against a JSON
schema. This replaces the flat `synth_*` note fields, the `preset_*` fields
and the 29 Rust-coded presets. The quality gap between the two projects is
control surface, not oscillator math: mcp-muse already has PolyBLEP
oscillators, a TPT state-variable filter and a stateful effects chain, but
hardcodes most engine parameters in the translator and offers no filter
envelope, LFO, second oscillator or per-instrument effects bus.

## Decisions taken

- **Replace, don't coexist.** `synth_*` and `preset_*` fields are removed
  from `SimpleNote` in the first PR. Built-in patches take over the role of
  presets. R2D2 notes and MIDI notes are untouched. `effects_preset` stays
  because MIDI notes use it too.
- **Percussion is an engine**, not a separate note type. Kick, snare,
  hi-hat, cymbal, zap, swoosh, chime and burst are kinds of the
  `percussion` engine with their real parameters exposed.
- **Offline per-patch rendering.** All notes of a patch in one call are
  rendered on the tool thread into one stereo buffer, effects are applied
  once to that buffer, and the buffer is scheduled through the existing
  engine. No new real-time code on the audio thread.

## 1. Patch data model

Snake_case JSON, consistent with the existing `effects` entries. Every
field except `name` has a default. Engines that are absent or have
`level: 0` are silent; several engines may sound at once.

```json
{
  "name": "warm_pad",
  "description": "Slow detuned saw pad with a filter sweep",
  "level": 0.8,
  "subtractive": {
    "level": 1.0,
    "osc1": {"wave": "saw"},
    "osc2": {"wave": "saw", "mix": 0.4, "detune_cents": 12, "octave": 0},
    "filter": {
      "type": "low_pass", "cutoff": 800, "resonance": 0.3, "slope": 24,
      "env_amount": 0.6,
      "env": {"attack": 1.5, "decay": 2.0, "sustain": 0.3, "release": 3.0}
    },
    "env": {"attack": 0.8, "decay": 1.0, "sustain": 0.7, "release": 2.5}
  },
  "fm": {
    "level": 0.0,
    "algorithm": "stack",
    "feedback": 0.0,
    "operators": [
      {"ratio": 1.0, "level": 1.0, "detune_cents": 0,
       "env": {"attack": 0.01, "decay": 0.2, "sustain": 0.8, "release": 0.4}}
    ]
  },
  "wavetable": {"level": 0.0, "table": "basic", "morph": 0.0, "env": {}},
  "granular": {
    "level": 0.0, "source": "harmonics", "grain_ms": 50, "density": 10,
    "pitch_semitones": 0, "randomness": 0.2, "stereo_width": 0.5, "env": {}
  },
  "percussion": {"level": 0.0, "kind": "kick", "punch": 0.8, "body_freq": 60},
  "lfo": {"rate": 0.3, "depth": 0.2, "wave": "sine", "target": "cutoff"},
  "effects": [
    {"type": "chorus", "rate": 0.6, "depth": 0.4, "intensity": 0.3},
    {"type": "reverb", "room_size": 0.7, "dampening": 0.4, "intensity": 0.35}
  ]
}
```

### Field reference

Envelopes (`env`) are `attack`, `decay`, `release` in seconds (0.001 to 10)
and `sustain` 0 to 1. Defaults: 0.01, 0.1, 0.8, 0.3.

**subtractive**
- `osc1.wave`, `osc2.wave`: `sine | saw | square | triangle | noise`. Square
  takes `pulse_width` 0.1 to 0.9 (default 0.5). `noise` is white noise (for
  wind and breath layers) and ignores pitch.
- `osc2.mix` 0 to 1 (blend osc1 to osc2), `osc2.detune_cents` -100 to 100,
  `osc2.octave` -2 to 2.
- `filter.type`: `low_pass | high_pass | band_pass`; `cutoff` 20 to 20000 Hz;
  `resonance` 0 to 1 (maps to Q 0.5 to 10 as today); `slope` 12 or 24;
  `env_amount` -1 to 1, sweeping up to four octaves; `filter.env` an
  envelope. Filter omitted means no filter.
- `env`: amplitude envelope.

**fm**
- `algorithm`: `stack` (4 to 3 to 2 to 1), `pairs` (3 to 1, 4 to 2, carriers
  1 and 2), `fan_in` (2, 3, 4 all modulate 1), `parallel` (all four are
  carriers, additive). Operator 1 is the first array entry and always a
  carrier.
- `operators`: 1 to 4 entries of `ratio` 0.25 to 16, `level` 0 to 1,
  `detune_cents` -100 to 100, `env`. A modulator's `level` is its
  modulation depth; a carrier's `level` is its output gain.
- `feedback` 0 to 1 on the last operator.
- Missing operators (fewer than the algorithm routes) are silent.
  Modulation depth is 4 radians at level 1 (`FM_MOD_DEPTH`). The FM engine
  has no filter; use a `filter` entry in the patch's `effects`.

**wavetable**
- `table`: `basic | warm | bright | digital | vocal | pwm | organ | noise`
  (the eight procedural tables from tryx-fx).
- `morph` 0 to 1 blends toward the next table in that order.
- Band limiting: ten mip levels, one per octave from 27.5 Hz; a level
  keeps partials below 20 kHz for every fundamental in its octave. Every
  table is an integer-harmonic series, so a table cycle is periodic and
  the ratio filter is a true band limit.

**granular**
- `source`: `harmonics | noise | formant | inharmonic`.
- `grain_ms` 5 to 500, `density` 1 to 50 grains per second,
  `pitch_semitones` -24 to 24, `randomness` 0 to 1, `stereo_width` 0 to 1.
- Each voice builds one peak-normalised source cycle at note-on (`noise`
  is a fresh random cycle); up to 32 grains overlap, summed with
  1/sqrt(active) normalisation.

**percussion**
- `kind` plus `level` and that kind's parameters, using the names already
  on `SynthType`: kick (`punch`, `sustain`, `click_freq`), snare (`snap`,
  `buzz`, `noise_amount`), hihat (`metallic`, `decay`, `brightness`),
  cymbal (`size`, `metallic`, `strike_intensity`), zap (`energy`, `decay`,
  `harmonic_content`), swoosh (`direction`, `intensity`, `sweep`:
  `[start_hz, end_hz]`), chime (`harmonic_count`, `decay`,
  `inharmonicity`), burst (`bandwidth`, `intensity`, `shape`).
- One `frequency` field serves every kind (kick body, snare tone, hi-hat
  and cymbal base, zap start, chime fundamental, burst centre) with a
  per-kind default. Percussion ignores the note's pitch, so `note` may be
  omitted on a percussion-only patch. A parameter that does not belong to
  the chosen kind is a validation error.
- Percussion carries its own envelope; `env` is not accepted here.
- Percussion is a flat object rather than a tagged enum because serde
  cannot combine `flatten` with `deny_unknown_fields`.

**lfo**
- `rate` 0.1 to 20 Hz, `depth` 0 to 1, `wave`:
  `sine | triangle | saw | square | sample_hold`, `target`:
  `off | cutoff | pitch | amplitude | morph | grain_density`.
- One LFO per patch, free-running from the start of the rendered buffer.
  Depth scaling per target: cutoff up to 2 octaves, pitch up to 2
  semitones, amplitude 0 to 100% tremolo, morph the full 0 to 1 range,
  grain density up to 2x.
- Amplitude depth `d` maps to `1 - d (1 - v) / 2`; morph adds `v d / 2` to
  the patch's morph and clamps; grain density multiplies by `2^(v d)`.

**effects**
- The existing ordered `EffectConfig` list (`reverb`, `delay`, `chorus`,
  `filter`, `compressor`, `distortion`) with the same parameters.
- `delay` gains optional Time Fracture fields: `random_beats: [min, max]`
  (0 to 4 beats, replaces `delay_time` when present), `random_rate` Hz
  (0 = static), `pitch_intervals` (up to 12 semitone values, -12 to 12),
  `pitch_mode`: `random | up | down | up_down`. Beat values use the
  sequence tempo.

**Pitch and dynamics.** Pitch comes from the note's MIDI number. Velocity
scales amplitude linearly (velocity 127 = patch `level`). The note's
duration is the gate; the release runs past it.

## 2. Tools and note reference

- **`define_synth`** takes a patch object, validates it, stores it in
  `ServerState` next to the session's patterns, and returns a one-line
  confirmation with a `play_notes` example. Names are case-insensitive and
  unique per session; redefining a name replaces it. Built-in patch names
  can be shadowed for the session.
- **`SimpleNote.synth`**: a patch name (string) or an inline patch object.
  Inline patches are not stored. A note with `synth` set is a synthesis
  note. `synth` on a `note_type: "r2d2"` note is an error, since R2D2
  keeps its own voice. `effects` and `effects_preset` are likewise rejected
  on a synth note (implemented): a patch renders through its own `effects`
  chain, so a note-level chain would be silently dropped. Both fields stay
  available on MIDI and R2D2 notes.
- **Removed from `SimpleNote`**: `synth_type`, `synth_frequency`,
  `synth_amplitude`, `synth_attack`, `synth_decay`, `synth_sustain`,
  `synth_release`, `synth_filter_type`, `synth_filter_cutoff`,
  `synth_filter_resonance`, `synth_reverb`, `synth_chorus`, `synth_delay`,
  `synth_delay_time`, `synth_pulse_width`, `synth_modulator_freq`,
  `synth_modulation_index`, `synth_grain_size`, `synth_texture_roughness`,
  `preset_name`, `preset_category`, `preset_variation`, `preset_random`.
  `SimpleNote` and every patch struct get `#[serde(deny_unknown_fields)]`
  (today unknown note fields are silently ignored), so old prompts and
  misspelled patch fields fail as `-32602` naming the field.
- **`list_sounds`** gains a `synths` section listing built-in patches by
  category and session-defined patches, each with its one-line
  description. The `presets` and `synthesis` sections are removed. The
  `effects` section documents the new delay fields.
- The `play_notes` and `define_synth` tool descriptions each carry two
  complete example patches (a subtractive bass and an FM bell), because
  worked examples are what made the tryx-fx prompt effective.
- The tool count becomes seven: the six existing tools plus `define_synth`.

## 3. Rendering pipeline

`Translator::translate` collects synthesis notes, resolves each `synth`
reference (session patches first, then built-ins, then inline), and
groups notes by resolved patch (inline patches group by structural
equality). For each group it builds a `PatchRenderer` holding one instance
of each enabled engine, the LFO and the patch's `EffectsChain`, and
renders one **stereo** buffer:

1. Span: from the earliest note-on to the latest note-off plus the longest
   release across enabled engines plus `EFFECT_TAIL_SECONDS` when the
   chain is non-empty.
2. Each note spawns one voice per enabled engine with a gate of the note's
   duration. Envelopes are gate-driven: attack, decay and sustain while
   the gate is open, release once it closes. This replaces
   `EnvelopeParams::level_at(t, duration)`, which squeezes the release
   inside the note. A voice ends when its amplitude envelope reaches zero
   or, for percussion, when the rendered hit ends.
3. Per sample: read the LFO once, apply it to its target, sum voices,
   multiply by engine level, patch level and velocity, pass the stereo
   pair through the effects chain, then apply `SYNTH_BUS_GAIN`.
4. The buffer is scheduled at the earliest note's frame with the existing
   `PlayCommand.buffers` mechanism. The returned play duration accounts
   for release and effect tails.

Engine change: `PlayCommand.buffers` and `ScheduledBuffer` carry stereo
samples as `Vec<[f32; 2]>`. R2D2 buffers are duplicated to both channels
at translate time. `render_frames` adds left and right separately. Nothing
else on the audio thread changes.

Rendering is on the tool thread. If a call with several patches proves
slow, each patch group can render on its own thread; the design does not
depend on it. Tempo for beat-synced effects is the sequence tempo.

## 4. DSP work

| Engine | Kept from mcp-muse | Ported from tryx-fx or new |
|---|---|---|
| Subtractive | PolyBLEP saw/square, `Svf` | osc2 with detune and octave; 24 dB slope as two cascaded `Svf`; filter envelope with bipolar amount; gate envelopes |
| FM | DX7 modulation-graph renderer | 4 operators exposed as data, 4 named algorithms, per-operator ratio, level, detune, envelope; feedback |
| Wavetable | nothing | 8 procedural tables; morph between neighbours; band-limited per-octave tables so high notes alias less than tryx-fx |
| Granular | nothing | 4 sources, grain scheduler, Hann window, pitch shift, randomness, stereo width |
| Percussion | all of `percussion.rs`, plus chime and burst moved from `synth.rs` | parameters exposed instead of hardcoded in the translator |
| LFO | nothing | 5 shapes, 5 targets |
| Effects | whole `effects.rs` chain | Time Fracture fields on `Delay` |

Removed: `SynthType` variants `Pad`, `Texture`, `Drone`, `FM` (two-op),
`Granular`, `Wavetable`, and the 2-operator DX7 mapping in the translator.
Pad, texture and drone return as built-in patches. `SynthType` itself is
replaced by the engine structs; `percussion.rs` takes a `Percussion`
config struct instead.

New module layout under `src/expressive/`:
- `patch.rs`: serde types for the patch, validation, `PatchLibrary`.
- `engines/{subtractive,fm,wavetable,granular,percussion}.rs`: one
  `Voice` type each, `fn tick(&mut self, mods: &Modulation) -> (f32, f32)`.
- `envelope.rs`: gate-driven ADSR.
- `lfo.rs`.
- `render.rs`: `PatchRenderer` (voices, LFO, effects, buffer span).
- `patches/*.json`: built-in patches.

## 5. Built-in patches

The 29 presets, the drum kit (kick, snare, hihat, cymbal), the sound
effects (zap, swoosh, chime, burst) and pad, texture and drone become
JSON files under `src/expressive/patches/`, embedded with `include_str!`
and loaded into `PatchLibrary` at startup. They use the same schema as
`define_synth`, so each file doubles as a worked example. Preset
variations become separate patches only where they change the character
("minimoog_bass_bright"); the rest are dropped. Each file carries a
`category` (`bass | pad | lead | keys | drums | fx`) used by `list_sounds`.

A test asserts every built-in file parses, renders a middle C (or a hit,
for percussion) without NaN, and stays under the clipper knee at the bus.

## 6. Validation and errors

- Invalid arguments, following the server's existing convention: a
  malformed JSON shape (wrong type, unknown field) and a patch that fails
  validation (out-of-range value, empty `operators`, a percussion
  parameter of another kind) both return `-32602`, with the field path and
  the allowed range in the message so the agent learns the schema from the
  error. This applies equally to `define_synth` and to inline patches on
  notes.
- Failures while executing (unknown patch name, no audio device, render
  failure): result with `isError: true`.
- Values are clamped only where tryx-fx clamps them (`mix`, `feedback`,
  `intensity`); everything else is rejected.
- An unknown patch name lists the defined and built-in names, mirroring
  the pattern-not-found message. Deviation as implemented: an unknown patch
  name lists the session-defined names and points to `list_sounds` for the
  built-ins (not all 31 names), to keep the message readable.

## 7. Testing

DSP is verified by measurement with `test_util` (Goertzel power, RMS,
zero-crossing rate), never by ear:

- Filter envelope: power at a high harmonic of a saw is lower late in the
  note than early for a negative `env_amount`, and the reverse for positive.
- osc2 detune: the RMS envelope of two detuned saws modulates at the
  expected beat frequency.
- LFO on pitch: zero-crossing rate over successive windows follows the
  LFO rate.
- FM: each algorithm produces sidebands at carrier ± ratio multiples;
  `parallel` produces none.
- Granular: `stereo_width` 1 yields a non-zero L minus R signal;
  `stereo_width` 0 yields identical channels.
- Gate envelopes: a note is still audible after its gate closes and
  silent by gate plus release.
- Wavetable: at a high MIDI note the band-limited table has less energy
  above Nyquist/2 folding than a naive 2048-sample table.
- Time Fracture: with `random_rate` 0 the delay time is fixed; with
  `pitch_intervals: [12]` the repeat is one octave up (zero-crossing rate
  doubles).
- Serde: every example patch in the tool descriptions round-trips.
- Server integration: `define_synth` then `play_notes` with the name,
  `play_notes` with an inline patch, unknown name is `isError`, old
  `synth_type` field is `-32602`.
- Existing headroom tests in `player.rs` re-pointed at built-in patches.

## 8. Sequencing

Four PRs, each leaving `main` working, each with its own implementation
plan:

1. **Foundation.** Patch model and `PatchLibrary`, gate envelopes, stereo
   buffers, subtractive and percussion engines, `PatchRenderer`,
   `define_synth`, `synth` on notes, removal of the old fields and Rust
   presets, built-in patches for the subtractive basses, the lead, drums,
   sound effects and subtractive approximations of the pads, `list_sounds`
   changes, minimal demo rewrite, README section.
2. **FM and wavetable** engines and their built-in patches: the three FM
   presets (DX7 E.Piano, DX7 Slap Bass, TX81Z Lately), bells, organs.
3. **Granular and LFO**, then the full pad, texture and drone patches.
4. **Time Fracture** delay fields, `demos.rs` rewritten around patches,
   docs pass.

## Out of scope

- Real-time voice rendering on the audio thread.
- Per-channel effects for MIDI notes (still one OxiSynth bus).
- Velocity-to-filter or aftertouch modulation, a second LFO, modulation
  matrix. Add only when a built-in patch needs it.
- Persisting session patches to disk.
