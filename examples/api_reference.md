# mcp-muse API Reference

Technical documentation for the `mcp-muse` MCP server.

## Overview

`mcp-muse` implements the Model Context Protocol over stdio using JSON-RPC
2.0. It exposes six tools that let an AI agent play General MIDI music,
R2D2-style expressions, classic synthesizer presets and custom synthesis
through the local audio device.

## MCP Protocol Compliance

| Method | Description |
|--------|-------------|
| `initialize` | Handshake; returns capabilities and the crate version |
| `ping` | Returns `{}` |
| `tools/list` | Lists the six tools with JSON schemas |
| `tools/call` | Executes a tool |
| `resources/list` | Empty list |
| `prompts/list` | Empty list |
| `notifications/*` | Any request without an `id` is treated as a notification and never answered |

- **Protocol version**: `2024-11-05`
- **Transport**: stdio, one JSON object per line

### Error conventions

| Situation | Response |
|-----------|----------|
| Unparseable JSON | JSON-RPC error `-32700`, `"id": null` |
| Unknown method or tool | JSON-RPC error `-32601` |
| Arguments fail to parse or validate (empty notes, out-of-range value, unknown effects preset) | JSON-RPC error `-32602` |
| The tool ran but failed (unknown preset name, pattern not defined, no audio device) | Normal result with `"isError": true` and a text explanation |

## Tools

### play_notes

Play a sequence of notes immediately. Each note can be a MIDI instrument
note, an R2D2 expression, a custom synthesis sound or a classic preset;
they may be mixed freely in one array. Returns as soon as playback starts.

Top-level arguments:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `notes` | array | required | One object per note (see below) |
| `tempo` | integer | 120 | BPM used for `musical_time` / `musical_duration` |
| `beats_per_bar` | integer | 4 | Time signature numerator |

Note fields (all optional unless noted):

| Group | Fields |
|-------|--------|
| Timing | `start_time` + `duration` (seconds) **or** `musical_time` `{bar, beat, tick}` + `musical_duration` (number = bars, or `"whole"`, `"half"`, `"quarter"`, `"eighth"`, `"sixteenth"`, `"triplet"`) |
| MIDI | `note` (0-127), `velocity`, `channel` (9 = drums), `instrument` (GM program), `reverb`, `chorus`, `volume`, `pan`, `balance`, `expression`, `sustain` (all 0-127) |
| R2D2 | `note_type: "r2d2"`, `r2d2_emotion` (required), `r2d2_intensity` 0-1 (required), `r2d2_complexity` 1-5 (required), `r2d2_pitch_range` `[min_hz, max_hz]` |
| Synthesis | `synth_type` (see `list_sounds`), `synth_frequency`, `synth_amplitude`, `synth_attack/decay/sustain/release`, `synth_filter_type/cutoff/resonance`, `synth_reverb/chorus/delay/delay_time`, `synth_pulse_width`, `synth_modulator_freq`, `synth_modulation_index`, `synth_grain_size`, `synth_texture_roughness` |
| Presets | `preset_name`, `preset_category` (`bass`, `pad`, `lead`, `keys`, `drums`, `effects`), `preset_variation`, `preset_random` |
| Effects | `effects` (array of `{type, ...params, intensity, enabled}`), `effects_preset` (name) |

Success text includes the total playback time, effect tails included:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "🎵 Playback started (3 notes: MIDI instruments). It will finish in about 4.0 seconds including effect tails; call stop_playback to cut it short."
    }]
  }
}
```

### define_sequence_pattern

Store a named, reusable pattern for this server session. Arguments:
`name` (required), `notes` (required, same note format), `description`,
`tempo`, `pattern_bars` (default 4), `beats_per_bar` (default 4),
`quantize_grid` (`off`, `bar`, `beat`, `8th`, `16th`, `32nd`, `triplet`;
applied to notes that use `musical_time`), `category`, `tags`.

### play_sequence

Play pattern references and/or individual notes. Each pattern reference
accepts `pattern_name` (required), `start_bar`, `start_beat`, `bars` (explicit
bar list), `repeat_count`, `repeat_spacing_bars`, `transpose` (-12..12),
`instrument_override`, `velocity_scale`, `duration_scale`, `channel_override`.
Top-level `tempo` and `beats_per_bar` apply to the whole sequence. An
undefined pattern name is an `isError` result listing the defined patterns.

### list_patterns

Lists the patterns defined in this session grouped by category.

### list_sounds

Returns a text catalog. Optional `section`: `all` (default), `presets`,
`instruments`, `drums`, `synthesis`, `r2d2`, `effects`. Use it before
guessing a preset or instrument name.

### stop_playback

Stops every active playback and reports how many were stopped.

## Server Capabilities

```json
{
  "protocolVersion": "2024-11-05",
  "capabilities": {
    "tools": { "listChanged": false },
    "resources": { "subscribe": false, "listChanged": false },
    "prompts": { "listChanged": false }
  },
  "serverInfo": { "name": "mcp-muse", "version": "<Cargo.toml version>" }
}
```

## Configuration

### Command line

```
mcp-muse                 # start the MCP server (default)
mcp-muse server          # same, explicit
mcp-muse setup           # interactive setup: SoundFont download, Cursor config
mcp-muse test-presets    # listen-by-ear demos: test-drums, test-pads,
                         # test-volumes, test-effects, debug-dx7
```

### Setup

`mcp-muse setup` downloads FluidR3_GM.sf2 (about 142 MB, verified against a
pinned SHA-256), optionally records a custom SoundFont path, and writes
`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "mcp-muse": { "command": "/path/to/mcp-muse", "args": [] }
  }
}
```

For other hosts, point them at the binary with no arguments; it speaks MCP
over stdio.

### Logging

- Location: `~/Library/Application Support/mcp-muse/` (macOS),
  `~/.local/share/mcp-muse/` (Linux), `%APPDATA%\mcp-muse\` (Windows).
  Files rotate daily as `mcp-muse.log.YYYY-MM-DD`; files older than seven
  days are removed at startup.
- Level: set `MCP_MUSE_LOG` (EnvFilter syntax, e.g. `debug`,
  `mcp_muse::server=trace`). Default is `info`.
