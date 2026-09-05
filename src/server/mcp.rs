use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::midi::{ExtendedSequence, MidiPlayer, SequencePattern, SimpleNote, SimpleSequence};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::time::Duration;

/// MCP protocol revision this server implements.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Per-process server state: the audio player (opened on first use so that
/// `tools/list` works without an audio device) and the session's patterns.
pub struct ServerState {
    player: Option<MidiPlayer>,
    patterns: HashMap<String, SequencePattern>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            player: None,
            patterns: HashMap::new(),
        }
    }

    fn player(&mut self) -> Result<&mut MidiPlayer, String> {
        if self.player.is_none() {
            self.player = Some(MidiPlayer::new()?);
            tracing::info!("Opened audio output stream");
        }
        Ok(self.player.as_mut().expect("player just initialised"))
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    /// `null` when the request id could not be read (JSON-RPC 2.0 §5).
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// A successful tool call carrying one text block.
    fn tool_text(id: Option<Value>, text: impl Into<String>) -> Self {
        Self::ok(
            id,
            json!({ "content": [{ "type": "text", "text": text.into() }] }),
        )
    }

    /// A tool call that ran but failed. Reported inside the result (with
    /// `isError`) so the model can see the message, per the MCP spec.
    fn tool_error(id: Option<Value>, text: impl Into<String>) -> Self {
        Self::ok(
            id,
            json!({ "content": [{ "type": "text", "text": text.into() }], "isError": true }),
        )
    }
}

/// JSON-RPC "Invalid params": the arguments could not be parsed or validated.
const INVALID_PARAMS: i32 = -32602;
/// JSON-RPC "Method not found".
const METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC "Parse error".
const PARSE_ERROR: i32 = -32700;

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    #[allow(dead_code)]
    protocol_version: String,
    #[allow(dead_code)]
    capabilities: Value,
    #[serde(rename = "clientInfo")]
    #[allow(dead_code)]
    client_info: Value,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    arguments: Value,
}

fn handle_initialize(_params: Option<Value>, id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling initialize request");

    let server_capabilities = json!({
        "tools": {
            "listChanged": false
        },
        "resources": {
            "subscribe": false,
            "listChanged": false
        },
        "prompts": {
            "listChanged": false
        }
    });

    let server_info = json!({
        "name": "mcp-muse",
        "version": env!("CARGO_PKG_VERSION")
    });

    JsonRpcResponse::ok(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": server_capabilities,
            "serverInfo": server_info
        }),
    )
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling tools/list request");

    let tools = json!([
        {
            "name": "define_sequence_pattern",
            "description": "Create reusable musical patterns (drum beats, bass lines, chord progressions, melodies) that can be referenced with play_sequence. Patterns can be transposed, use different instruments, and repeat with perfect bar-based timing.

Example: Define a 4-bar house beat once, then play it with variations throughout your composition.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "🏷️ Pattern name/identifier (e.g., 'house_beat', 'chord_prog_1', 'funky_bass')"
                    },
                    "description": {
                        "type": "string",
                        "description": "📝 Optional description of what this pattern represents"
                    },
                    "notes": {
                        "type": "array",
                        "description": "🎵 Array of notes that make up this pattern",
                        "items": {
                            "type": "object",
                            "properties": {
                                "note": {"type": "integer", "minimum": 0, "maximum": 127},
                                "velocity": {"type": "integer", "minimum": 0, "maximum": 127},
                                "start_time": {"type": "number", "description": "⚠️ DEPRECATED: Use musical_time for better sync"},
                                "duration": {"type": "number", "description": "⚠️ DEPRECATED: Use musical_duration for better sync"},
                                "musical_time": {
                                    "type": "object",
                                    "description": "🎼 Musical timing (bar.beat.tick) - RECOMMENDED for perfect sync!",
                                    "properties": {
                                        "bar": {"type": "integer", "minimum": 1, "description": "Bar number (1-based)"},
                                        "beat": {"type": "integer", "minimum": 1, "maximum": 4, "description": "Beat within bar (1-4)"},
                                        "tick": {"type": "integer", "minimum": 0, "maximum": 479, "description": "Tick within beat (0-479)"}
                                    },
                                    "required": ["bar", "beat", "tick"]
                                },
                                "musical_duration": {
                                    "type": "object",
                                    "description": "🎵 Musical duration - RECOMMENDED for perfect sync!",
                                    "oneOf": [
                                        {"type": "number", "description": "Duration in bars (e.g., 1.5 for one and a half bars)"},
                                        {"type": "string", "enum": ["whole", "half", "quarter", "eighth", "sixteenth", "triplet"], "description": "Note values"}
                                    ]
                                },
                                "channel": {"type": "integer", "minimum": 0, "maximum": 15, "default": 0},
                                "instrument": {"type": "integer", "minimum": 0, "maximum": 127},
                                "note_type": {"type": "string", "enum": ["midi", "r2d2"], "default": "midi"},
                                "r2d2_emotion": {"type": "string", "enum": ["Happy", "Sad", "Excited", "Worried", "Curious", "Affirmative", "Negative", "Surprised", "Thoughtful"]},
                                "r2d2_intensity": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                                "r2d2_complexity": {"type": "integer", "minimum": 1, "maximum": 5},
                                "synth_type": {"type": "string"},
                                "preset_name": {"type": "string"},
                                "preset_category": {"type": "string"}
                            },
                            "anyOf": [
                                {"required": ["start_time", "duration"]},
                                {"required": ["musical_time", "musical_duration"]}
                            ]
                        }
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Default tempo for this pattern (can be overridden when referenced)",
                        "minimum": 60,
                        "maximum": 200,
                        "default": 120
                    },
                    "pattern_bars": {
                        "type": "number",
                        "description": "🎼 Pattern length in bars (ensures perfect looping!) - RECOMMENDED for sync",
                        "minimum": 0.25,
                        "maximum": 16.0,
                        "default": 4.0
                    },
                    "beats_per_bar": {
                        "type": "integer",
                        "description": "🎶 Time signature - beats per bar (4 for 4/4 time)",
                        "minimum": 2,
                        "maximum": 8,
                        "default": 4
                    },
                    "quantize_grid": {
                        "type": "string",
                        "description": "📐 Snap timing to musical grid for perfect alignment",
                        "enum": ["off", "bar", "beat", "8th", "16th", "32nd", "triplet"],
                        "default": "off"
                    },
                    "category": {
                        "type": "string",
                        "description": "🏗️ Pattern category for organization (e.g., 'drums', 'bass', 'melody', 'chords')"
                    },
                    "tags": {
                        "type": "array",
                        "description": "🏷️ Tags for searching/filtering patterns",
                        "items": {"type": "string"}
                    }
                },
                "required": ["name", "notes"]
            }
        },
        {
            "name": "play_sequence",
            "description": "Play compositions using defined patterns or individual notes. Patterns can be transposed, repeated, and transformed. Use bar-based timing for professional sync.

For complex music: use patterns. For quick sounds: use play_notes instead.

Example: {\"patterns\": [{\"pattern_name\": \"drums\", \"start_bar\": 1, \"repeat_count\": 8}, {\"pattern_name\": \"bass\", \"start_bar\": 1, \"transpose\": 5}]}",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notes": {
                        "type": "array",
                        "description": "🎵 Individual notes (same format as play_notes tool)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "note": {"type": "integer", "minimum": 0, "maximum": 127},
                                "velocity": {"type": "integer", "minimum": 0, "maximum": 127},
                                "start_time": {"type": "number"},
                                "duration": {"type": "number"},
                                "channel": {"type": "integer", "minimum": 0, "maximum": 15, "default": 0},
                                "instrument": {"type": "integer", "minimum": 0, "maximum": 127},
                                "note_type": {"type": "string", "enum": ["midi", "r2d2"], "default": "midi"}
                            },
                            "required": ["start_time", "duration"]
                        }
                    },
                    "patterns": {
                        "type": "array",
                        "description": "🎼 Pattern references with transformations",
                        "items": {
                            "type": "object",
                            "properties": {
                                "pattern_name": {
                                    "type": "string",
                                    "description": "🏷️ Name of the pattern to reference"
                                },
                                "start_time_offset": {
                                    "type": "number",
                                    "description": "⏰ DEPRECATED: Use start_bar for perfect sync!",
                                    "default": 0
                                },
                                "start_bar": {
                                    "type": "integer",
                                    "description": "🎼 RECOMMENDED: Start at specific bar number (1-based) - ensures perfect alignment!",
                                    "minimum": 1,
                                    "maximum": 256
                                },
                                "start_beat": {
                                    "type": "integer",
                                    "description": "🎵 Start on specific beat within the bar (1-4 for 4/4 time)",
                                    "minimum": 1,
                                    "maximum": 8,
                                    "default": 1
                                },
                                "bars": {
                                    "type": "array",
                                    "description": "🎯 SMART ARRANGEMENT: Play pattern on specific bars only (e.g., [1, 5, 9, 13])",
                                    "items": {"type": "integer", "minimum": 1, "maximum": 256}
                                },
                                "transpose": {
                                    "type": "integer",
                                    "description": "🎵 Transpose by semitones (-12 to +12): -12=octave down, 0=original, +7=fifth up, +12=octave up",
                                    "minimum": -12,
                                    "maximum": 12,
                                    "default": 0
                                },
                                "instrument_override": {
                                    "type": "integer",
                                    "description": "🎹 Override instrument for all MIDI notes in pattern",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "velocity_scale": {
                                    "type": "number",
                                    "description": "🔊 Scale all velocities (0.1-2.0): 0.5=softer, 1.0=original, 1.5=louder",
                                    "minimum": 0.1,
                                    "maximum": 2.0,
                                    "default": 1.0
                                },
                                "duration_scale": {
                                    "type": "number",
                                    "description": "⏳ Scale all durations (0.1-4.0): 0.5=staccato, 1.0=original, 2.0=legato",
                                    "minimum": 0.1,
                                    "maximum": 4.0,
                                    "default": 1.0
                                },
                                "channel_override": {
                                    "type": "integer",
                                    "description": "📻 Override MIDI channel for all notes in pattern",
                                    "minimum": 0,
                                    "maximum": 15
                                },
                                "repeat_count": {
                                    "type": "integer",
                                    "description": "🔄 Number of times to repeat this pattern (ignored if 'bars' specified)",
                                    "minimum": 1,
                                    "maximum": 64,
                                    "default": 1
                                },
                                "repeat_spacing_bars": {
                                    "type": "number",
                                    "description": "🎼 RECOMMENDED: Spacing between repeats in bars (musical spacing)",
                                    "minimum": 0,
                                    "maximum": 16,
                                    "default": 0
                                },
                                "align_to_bars": {
                                    "type": "boolean",
                                    "description": "📐 Align pattern to bar boundaries for perfect sync",
                                    "default": true
                                }
                            },
                            "required": ["pattern_name"]
                        }
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Tempo in BPM for the entire sequence",
                        "minimum": 60,
                        "maximum": 200,
                        "default": 120
                    }
                },
                "anyOf": [
                    {"required": ["notes"]},
                    {"required": ["patterns"]}
                ]
            }
        },
        {
            "name": "list_patterns",
            "description": "List all defined sequence patterns with their names, categories, and note counts.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "stop_playback",
            "description": "Stop all sounds that are currently playing. Playback tools return immediately while audio continues in the background; call this to cut it short.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "play_notes",
            "description": "Play quick sounds, effects, and simple melodies. Supports MIDI (128 instruments), R2D2 expressions (9 emotions), and synthesis (19 types). For complex compositions with 3+ notes, use define_sequence_pattern + play_sequence instead.

Examples:
- Success chime: [{\"note\": 72, \"instrument\": 9, \"duration\": 0.5}]
- R2D2 happy: [{\"note_type\": \"r2d2\", \"r2d2_emotion\": \"Happy\", \"r2d2_intensity\": 0.8, \"r2d2_complexity\": 2, \"duration\": 1.0}]
- Kick drum: [{\"synth_type\": \"kick\", \"synth_frequency\": 60, \"duration\": 0.5}]",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notes": {
                        "type": "array",
                        "description": "Array of notes to play",
                        "items": {
                            "type": "object",
                            "properties": {
                                "note": {
                                    "type": "integer",
                                    "description": "🎵 MIDI note number: 60=C4(middle C), 64=E4, 67=G4. Range: C0(12) to G9(127). Use chromatic scales: C=0,2,4,5,7,9,11 pattern",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "velocity": {
                                    "type": "integer",
                                    "description": "🔊 Note attack velocity (intensity): 40=soft, 80=medium, 110=forte, 127=maximum. Affects both volume and timbre",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "start_time": {
                                    "type": "number",
                                    "description": "⏰ Start time in seconds. Use 0.0 for simultaneous notes (chords), incremental timing for melodies. DEPRECATED: Consider using musical_time for better sync."
                                },
                                "duration": {
                                    "type": "number",
                                    "description": "⏳ Note duration in seconds. Try: 0.25=16th, 0.5=8th, 1.0=quarter, 2.0=half, 4.0=whole note. DEPRECATED: Consider using musical_duration for better sync."
                                },
                                "musical_time": {
                                    "type": "object",
                                    "description": "🎼 Musical timing (bar.beat.tick) - Alternative to start_time for precise timing",
                                    "properties": {
                                        "bar": {"type": "integer", "minimum": 1, "description": "Bar number (1-based)"},
                                        "beat": {"type": "integer", "minimum": 1, "maximum": 4, "description": "Beat within bar (1-4)"},
                                        "tick": {"type": "integer", "minimum": 0, "maximum": 479, "description": "Tick within beat (0-479)"}
                                    },
                                    "required": ["bar", "beat", "tick"]
                                },
                                "musical_duration": {
                                    "type": "object",
                                    "description": "🎵 Musical duration - Alternative to duration for precise timing",
                                    "oneOf": [
                                        {"type": "number", "description": "Duration in bars (e.g., 1.5 for one and a half bars)"},
                                        {"type": "string", "enum": ["whole", "half", "quarter", "eighth", "sixteenth", "triplet"], "description": "Note values"}
                                    ]
                                },
                                "channel": {
                                    "type": "integer",
                                    "description": "📻 MIDI channel (0-15): Use different channels for different instruments in complex arrangements. Each channel can have unique instrument/effects",
                                    "minimum": 0,
                                    "maximum": 15
                                },
                                "instrument": {
                                    "type": "integer",
                                    "description": "🎹 GM Instrument: 0=Piano, 1=Bright Piano, 25=Steel Guitar, 40=Violin, 42=Cello, 56=Trumpet, 60=French Horn, 68=Oboe, 73=Flute, 80=Square Lead, 104=Sitar. Use variety for rich orchestration!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "reverb": {
                                    "type": "integer",
                                    "description": "🏛️ Reverb depth (0-127): Simulates acoustic spaces. Try 0=dry, 30=small room, 60=hall, 100=cathedral. Essential for realistic orchestral sound!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "chorus": {
                                    "type": "integer",
                                    "description": "✨ Chorus depth (0-127): Adds shimmer and richness. Try 0=off, 30=subtle, 60=lush, 100=ethereal. Great for strings, pads, and vocals!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "volume": {
                                    "type": "integer",
                                    "description": "🔊 Channel volume (0-127): Master volume per channel. Use for mixing balance - lead melody at 100-127, accompaniment at 60-90, bass at 80-100",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "pan": {
                                    "type": "integer",
                                    "description": "↔️ Pan position (0-127): For MONO instruments like trumpet, flute. 0=hard left, 64=center, 127=hard right. Create stereo width in arrangements!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "balance": {
                                    "type": "integer",
                                    "description": "⚖️ Balance control (0-127): For STEREO instruments like piano, strings. 0=left, 64=center, 127=right. Use this instead of pan for piano!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "expression": {
                                    "type": "integer",
                                    "description": "🎭 Expression control (0-127): Dynamic musical expression beyond velocity. 40=pianissimo, 80=normal, 110=forte, 127=fortissimo. Creates emotional phrasing!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "sustain": {
                                    "type": "integer",
                                    "description": "🎹 Sustain pedal (0-127): Piano-style sustain. 0=off (staccato), 127=on (legato). Use for flowing passages and rich harmonic resonance!",
                                    "minimum": 0,
                                    "maximum": 127
                                },
                                "note_type": {
                                    "type": "string",
                                    "description": "🎭 Note type: 'midi' for musical notes, 'r2d2' for robotic expressions. Defaults to 'midi'",
                                    "enum": ["midi", "r2d2"],
                                    "default": "midi"
                                },
                                "r2d2_emotion": {
                                    "type": "string",
                                    "description": "🤖 R2D2 emotion when note_type='r2d2': Choose from 9 distinct emotional expressions. **REQUIRED when note_type='r2d2'**",
                                    "enum": ["Happy", "Sad", "Excited", "Worried", "Curious", "Affirmative", "Negative", "Surprised", "Thoughtful"]
                                },
                                "r2d2_intensity": {
                                    "type": "number",
                                    "description": "🔥 R2D2 emotional intensity (0.0-1.0): 0.3=subtle, 0.6=moderate, 0.9=dramatic. **REQUIRED when note_type='r2d2'**",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "r2d2_complexity": {
                                    "type": "integer",
                                    "description": "🗣️ R2D2 phrase complexity (1-5 syllables): 1=simple beep, 3=conversational, 5=complex phrase. **REQUIRED when note_type='r2d2'**",
                                    "minimum": 1,
                                    "maximum": 5
                                },
                                "r2d2_pitch_range": {
                                    "type": "array",
                                    "description": "🎵 R2D2 frequency range [min_hz, max_hz]: [200,600]=low, [300,800]=normal, [400,1000]=high",
                                    "items": {
                                        "type": "number"
                                    },
                                    "minItems": 2,
                                    "maxItems": 2
                                },
                                "r2d2_context": {
                                    "type": "string",
                                    "description": "💭 R2D2 context: Optional conversation context for enhanced expression adaptation"
                                },
                                "synth_type": {
                                    "type": "string",
                                    "description": "🎛️ Synthesis type: 'sine', 'square', 'sawtooth', 'triangle', 'noise', 'fm', 'granular', 'wavetable', 'kick', 'snare', 'hihat', 'cymbal', 'swoosh', 'zap', 'chime', 'burst', 'pad', 'texture', 'drone' (optional)"
                                },
                                "synth_frequency": {
                                    "type": "number",
                                    "description": "🎵 Synthesis frequency in Hz (20-20000, optional, overrides MIDI note if present)",
                                    "minimum": 20,
                                    "maximum": 20000
                                },
                                "synth_amplitude": {
                                    "type": "number",
                                    "description": "🔊 Synthesis amplitude (0.0-1.0, optional, defaults to 0.7)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_attack": {
                                    "type": "number",
                                    "description": "⚡ Attack time in seconds (0.0-5.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 5.0
                                },
                                "synth_decay": {
                                    "type": "number",
                                    "description": "📉 Decay time in seconds (0.0-5.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 5.0
                                },
                                "synth_sustain": {
                                    "type": "number",
                                    "description": "🎹 Sustain level (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_release": {
                                    "type": "number",
                                    "description": "🌊 Release time in seconds (0.0-10.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 10.0
                                },
                                "synth_filter_type": {
                                    "type": "string",
                                    "description": "🎚️ Filter type: 'lowpass', 'highpass', 'bandpass' (optional)",
                                    "enum": ["lowpass", "highpass", "bandpass"]
                                },
                                "synth_filter_cutoff": {
                                    "type": "number",
                                    "description": "🔧 Filter cutoff frequency in Hz (20-20000, optional)",
                                    "minimum": 20,
                                    "maximum": 20000
                                },
                                "synth_filter_resonance": {
                                    "type": "number",
                                    "description": "✨ Filter resonance (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_reverb": {
                                    "type": "number",
                                    "description": "🏛️ Synthesis reverb intensity (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_chorus": {
                                    "type": "number",
                                    "description": "✨ Synthesis chorus intensity (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_delay": {
                                    "type": "number",
                                    "description": "🔄 Synthesis delay intensity (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "synth_delay_time": {
                                    "type": "number",
                                    "description": "⏰ Synthesis delay time in seconds (0.0-2.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 2.0
                                },
                                "synth_pulse_width": {
                                    "type": "number",
                                    "description": "📊 Pulse width for square wave (0.1-0.9, optional)",
                                    "minimum": 0.1,
                                    "maximum": 0.9
                                },
                                "synth_modulator_freq": {
                                    "type": "number",
                                    "description": "🌀 FM modulator frequency in Hz (0.1-1000.0, optional)",
                                    "minimum": 0.1,
                                    "maximum": 1000.0
                                },
                                "synth_modulation_index": {
                                    "type": "number",
                                    "description": "🎛️ FM modulation index (0.0-10.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 10.0
                                },
                                "synth_grain_size": {
                                    "type": "number",
                                    "description": "🌾 Granular grain size in seconds (0.01-0.5, optional)",
                                    "minimum": 0.01,
                                    "maximum": 0.5
                                },
                                "synth_texture_roughness": {
                                    "type": "number",
                                    "description": "🎨 Texture roughness (0.0-1.0, optional)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "preset_name": {
                                    "type": "string",
                                    "description": "🎹 Classic synthesizer preset name: Load specific authentic vintage preset (e.g., 'Minimoog Bass', 'TB-303 Acid', 'Jupiter Bass', 'Prophet Lead', 'DX7 E.Piano'). Use for instant access to iconic synthesizer sounds!"
                                },
                                "preset_category": {
                                    "type": "string",
                                    "description": "🎭 Preset category: Choose preset from category ('bass', 'pad', 'lead', 'keys', 'organ', 'arp', 'drums', 'effects'). Perfect for exploring different types of classic sounds!",
                                    "enum": ["bass", "pad", "lead", "keys", "organ", "arp", "drums", "effects"]
                                },
                                "preset_variation": {
                                    "type": "string",
                                    "description": "🎨 Preset variation: Apply subtle variation to base preset (e.g., 'bright', 'dark', 'squelchy'). Great for customizing classic sounds to fit your music!"
                                },
                                "preset_random": {
                                    "type": "boolean",
                                    "description": "🎲 Random preset selection: Set to true to randomly select a preset. Optionally combine with preset_category to limit random selection to specific category. Perfect for creative inspiration!"
                                },
                                "effects": {
                                    "type": "array",
                                    "description": "🎛️ PROFESSIONAL EFFECTS CHAIN: Apply high-quality audio effects to individual notes. Overrides preset signature effects when specified.",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "effect": {
                                                "type": "object",
                                                "description": "🎚️ Effect type configuration",
                                                "oneOf": [
                                                    {
                                                        "type": "object",
                                                        "description": "🏛️ REVERB: Schroeder reverb with comb filters + allpass diffusion for realistic spatial effects",
                                                        "properties": {
                                                            "type": {"const": "Reverb"},
                                                            "room_size": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Room size: 0.1=closet, 0.5=studio, 0.8=concert hall, 1.0=cathedral"},
                                                            "dampening": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "High-frequency dampening: 0.0=bright, 0.5=natural, 1.0=dark"},
                                                            "wet_level": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Reverb amount: 0.1=subtle, 0.3=moderate, 0.6=lush, 0.9=swimming"},
                                                            "pre_delay": {"type": "number", "minimum": 0.0, "maximum": 0.2, "description": "Pre-delay in seconds: 0.02=small room, 0.05=large hall, 0.1=stadium"}
                                                        }
                                                    },
                                                    {
                                                        "type": "object",
                                                        "description": "🔄 DELAY: Feedback delay with analog character and high-frequency damping",
                                                        "properties": {
                                                            "type": {"const": "Delay"},
                                                            "delay_time": {"type": "number", "minimum": 0.01, "maximum": 2.0, "description": "Delay time in seconds: 0.125=8th note @120bpm, 0.25=quarter note, 0.5=half note"},
                                                            "feedback": {"type": "number", "minimum": 0.0, "maximum": 0.95, "description": "Feedback amount: 0.2=single echo, 0.5=multiple repeats, 0.8=infinite sustain"},
                                                            "wet_level": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Delay mix: 0.2=subtle, 0.5=balanced, 0.8=delay-heavy"},
                                                            "sync_tempo": {"type": "boolean", "description": "Sync to tempo (future feature)"}
                                                        }
                                                    },
                                                    {
                                                        "type": "object",
                                                        "description": "🌊 CHORUS: Multi-tap modulated delays with LFO for lush, swirling effects",
                                                        "properties": {
                                                            "type": {"const": "Chorus"},
                                                            "rate": {"type": "number", "minimum": 0.1, "maximum": 8.0, "description": "LFO rate in Hz: 0.5=slow swirl, 1.5=moderate, 4.0=fast vibrato"},
                                                            "depth": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Modulation depth: 0.3=subtle, 0.6=lush, 0.9=dramatic"},
                                                            "feedback": {"type": "number", "minimum": 0.0, "maximum": 0.8, "description": "Chorus feedback: 0.2=clean, 0.4=rich, 0.7=resonant"},
                                                            "stereo_width": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Stereo width: 0.5=narrow, 0.8=wide, 1.0=maximum"}
                                                        }
                                                    },
                                                    {
                                                        "type": "object",
                                                        "description": "🎚️ FILTER: State variable filter with all filter types",
                                                        "properties": {
                                                            "type": {"const": "Filter"},
                                                            "filter_type": {"type": "string", "enum": ["LowPass", "HighPass", "BandPass", "Notch", "Peak", "LowShelf", "HighShelf"], "description": "Filter type"},
                                                            "cutoff": {"type": "number", "minimum": 20.0, "maximum": 20000.0, "description": "Cutoff frequency in Hz"},
                                                            "resonance": {"type": "number", "minimum": 0.1, "maximum": 20.0, "description": "Filter resonance/Q factor"},
                                                            "envelope_amount": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Envelope modulation (future feature)"}
                                                        }
                                                    },
                                                    {
                                                        "type": "object",
                                                        "description": "📊 COMPRESSOR: Smooth dynamics processing for punch and control",
                                                        "properties": {
                                                            "type": {"const": "Compressor"},
                                                            "threshold": {"type": "number", "minimum": -60.0, "maximum": 0.0, "description": "Threshold in dB: -20=gentle, -12=moderate, -6=aggressive"},
                                                            "ratio": {"type": "number", "minimum": 1.0, "maximum": 20.0, "description": "Compression ratio: 2=subtle, 4=moderate, 8=heavy, 20=limiter"},
                                                            "attack": {"type": "number", "minimum": 0.001, "maximum": 0.1, "description": "Attack time in seconds: 0.001=fast, 0.01=medium, 0.1=slow"},
                                                            "release": {"type": "number", "minimum": 0.01, "maximum": 2.0, "description": "Release time in seconds: 0.05=fast, 0.2=medium, 1.0=slow"}
                                                        }
                                                    },
                                                    {
                                                        "type": "object",
                                                        "description": "🔥 DISTORTION: Waveshaping with pre/post filtering for musical overdrive",
                                                        "properties": {
                                                            "type": {"const": "Distortion"},
                                                            "drive": {"type": "number", "minimum": 0.0, "maximum": 5.0, "description": "Drive amount: 1.0=warm, 2.5=crunch, 5.0=heavy"},
                                                            "tone": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Tone control: 0.0=dark, 0.5=neutral, 1.0=bright"},
                                                            "output_level": {"type": "number", "minimum": 0.1, "maximum": 2.0, "description": "Output compensation: 0.5=quiet, 1.0=unity, 1.5=boost"}
                                                        }
                                                    }
                                                ]
                                            },
                                            "intensity": {
                                                "type": "number",
                                                "minimum": 0.0,
                                                "maximum": 1.0,
                                                "description": "🔊 Effect intensity/wet-dry mix: 0.0=bypassed, 0.3=subtle, 0.6=moderate, 1.0=maximum effect"
                                            },
                                            "enabled": {
                                                "type": "boolean",
                                                "description": "🔛 Enable/disable this effect",
                                                "default": true
                                            }
                                        },
                                        "required": ["effect", "intensity"]
                                    }
                                },
                                "effects_preset": {
                                    "type": "string",
                                    "description": "🎭 EFFECTS PRESET: Apply curated effect combinations. Choose from professional presets: 'studio' (clean + subtle reverb), 'concert_hall' (spacious reverb), 'vintage' (analog warmth), 'ambient' (lush atmospheric), 'live_stage' (punchy compression), 'tight_mix' (controlled dynamics), 'dreamy' (soft ethereal), 'spacious' (wide reverb), 'analog_warmth' (tube character), 'retro_echo' (tape delay), 'psychedelic' (wild modulation), 'distorted' (aggressive), 'filtered' (prominent filtering), 'lush_chorus' (rich modulation). Effects presets provide instant professional sound character!",
                                    "enum": ["studio", "concert_hall", "vintage", "ambient", "live_stage", "tight_mix", "dreamy", "spacious", "analog_warmth", "retro_echo", "psychedelic", "distorted", "filtered", "lush_chorus"]
                                }
                            },
                            "anyOf": [
                                {"required": ["start_time", "duration"]},
                                {"required": ["musical_time", "musical_duration"]}
                            ],
                            "additionalProperties": false
                        }
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "Tempo in BPM (optional, defaults to 120)",
                        "minimum": 60,
                        "maximum": 200
                    }
                },
                "required": ["notes"]
            }
        }
    ]);

    JsonRpcResponse::ok(id, json!({ "tools": tools }))
}

fn handle_resources_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::ok(id, json!({ "resources": [] }))
}

fn handle_prompts_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::ok(id, json!({ "prompts": [] }))
}

fn handle_tool_call(
    state: &mut ServerState,
    params: Option<Value>,
    id: Option<Value>,
) -> JsonRpcResponse {
    let Some(params) = params else {
        return JsonRpcResponse::error(id, INVALID_PARAMS, "Invalid params");
    };
    let tool_params: ToolCallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Invalid tool call params: {}", e),
            );
        }
    };
    tracing::info!("tools/call {}", tool_params.name);

    match tool_params.name.as_str() {
        "play_notes" => handle_play_notes(state, tool_params.arguments, id),
        "define_sequence_pattern" => handle_define_pattern(state, tool_params.arguments, id),
        "play_sequence" => handle_play_sequence(state, tool_params.arguments, id),
        "list_patterns" => handle_list_patterns(state, id),
        "stop_playback" => handle_stop_playback(state, id),
        other => JsonRpcResponse::error(id, METHOD_NOT_FOUND, format!("Unknown tool: {}", other)),
    }
}

/// Parameter-level validation shared by every tool that accepts notes.
fn validate_notes(notes: &[SimpleNote]) -> Result<(), String> {
    for (i, note) in notes.iter().enumerate() {
        let checks = [
            ("R2D2", note.validate_r2d2()),
            ("synthesis", note.validate_synthesis()),
            ("preset", note.validate_preset()),
            ("effects", note.validate_effects()),
        ];
        for (what, result) in checks {
            if let Err(e) = result {
                return Err(format!(
                    "Invalid {} parameters in note {}: {}",
                    what,
                    i + 1,
                    e
                ));
            }
        }
    }
    Ok(())
}

/// Human-readable summary of what a sequence contains.
fn describe_sources(notes: &[SimpleNote]) -> String {
    let mut parts = Vec::new();
    if notes.iter().any(|n| n.note_type == "r2d2") {
        parts.push("R2D2 expressions");
    }
    if notes.iter().any(|n| n.is_preset()) {
        parts.push("classic synth presets");
    }
    if notes.iter().any(|n| n.is_synthesis() && !n.is_preset()) {
        parts.push("custom synthesis");
    }
    if notes
        .iter()
        .any(|n| n.note_type != "r2d2" && !n.is_synthesis() && !n.is_preset())
    {
        parts.push("MIDI instruments");
    }
    if parts.is_empty() {
        "audio".to_string()
    } else {
        parts.join(" + ")
    }
}

fn playback_started_text(summary: String, duration: Duration) -> String {
    format!(
        "🎵 Playback started ({}). It will finish in about {:.1} seconds including effect tails; call stop_playback to cut it short.",
        summary,
        duration.as_secs_f64()
    )
}

fn start_playback(
    state: &mut ServerState,
    sequence: SimpleSequence,
    id: Option<Value>,
    summary: String,
) -> JsonRpcResponse {
    let player = match state.player() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Audio output unavailable: {}", e);
            return JsonRpcResponse::tool_error(id, format!("Audio output unavailable: {}", e));
        }
    };
    match player.play_enhanced_mixed(sequence) {
        Ok(duration) => JsonRpcResponse::tool_text(id, playback_started_text(summary, duration)),
        Err(e) => {
            tracing::error!("Playback failed: {}", e);
            JsonRpcResponse::tool_error(id, format!("Playback failed: {}", e))
        }
    }
}

fn handle_play_notes(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let sequence: SimpleSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Failed to parse note sequence: {}", e),
            );
        }
    };
    if sequence.notes.is_empty() {
        return JsonRpcResponse::error(id, INVALID_PARAMS, "Note sequence cannot be empty");
    }
    if let Err(e) = validate_notes(&sequence.notes) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }

    let summary = format!(
        "{} notes: {}",
        sequence.notes.len(),
        describe_sources(&sequence.notes)
    );
    start_playback(state, sequence, id, summary)
}

fn handle_define_pattern(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let pattern: SequencePattern = match serde_json::from_value(arguments) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Failed to parse sequence pattern: {}", e),
            );
        }
    };
    if pattern.notes.is_empty() {
        return JsonRpcResponse::error(id, INVALID_PARAMS, "Pattern notes cannot be empty");
    }
    if let Err(e) = validate_notes(&pattern.notes) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }

    let mut details = format!(
        "🎼 Defined pattern '{}': {} notes, {:.2} s at {} BPM, {} bars of {}/4",
        pattern.name,
        pattern.notes.len(),
        pattern.get_pattern_duration(),
        pattern.tempo,
        pattern.pattern_bars,
        pattern.beats_per_bar
    );
    if let Some(category) = &pattern.category {
        details.push_str(&format!(" (category: {})", category));
    }
    if !pattern.tags.is_empty() {
        details.push_str(&format!(" [tags: {}]", pattern.tags.join(", ")));
    }
    if let Some(description) = &pattern.description {
        details.push_str(&format!("\n{}", description));
    }
    details.push_str(&format!(
        "\n\nUse it with play_sequence, e.g. {{\"patterns\": [{{\"pattern_name\": \"{}\", \"start_bar\": 1, \"repeat_count\": 2}}]}}",
        pattern.name
    ));

    tracing::info!("Stored pattern '{}'", pattern.name);
    state.patterns.insert(pattern.name.clone(), pattern);
    JsonRpcResponse::tool_text(id, details)
}

fn handle_play_sequence(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let extended: ExtendedSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Failed to parse sequence: {}", e),
            );
        }
    };
    if extended.notes.is_empty() && extended.patterns.is_empty() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            "Sequence must contain either notes or pattern references",
        );
    }
    if let Err(e) = validate_notes(&extended.notes) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }

    let resolved = match extended.resolve_patterns(&state.patterns) {
        Ok(seq) => seq,
        Err(e) => {
            let known: Vec<&String> = state.patterns.keys().collect();
            return JsonRpcResponse::tool_error(
                id,
                format!("{}. Defined patterns: {:?}", e, known),
            );
        }
    };
    if resolved.notes.is_empty() {
        return JsonRpcResponse::tool_error(id, "Resolved sequence contains no notes");
    }

    let summary = format!(
        "{} pattern references + {} individual notes → {} notes: {}",
        extended.patterns.len(),
        extended.notes.len(),
        resolved.notes.len(),
        describe_sources(&resolved.notes)
    );
    start_playback(state, resolved, id, summary)
}

fn handle_list_patterns(state: &ServerState, id: Option<Value>) -> JsonRpcResponse {
    if state.patterns.is_empty() {
        return JsonRpcResponse::tool_text(
            id,
            "📋 No patterns defined yet. Use define_sequence_pattern to create one, then reference it from play_sequence.",
        );
    }

    let mut by_category: std::collections::BTreeMap<&str, Vec<&SequencePattern>> =
        std::collections::BTreeMap::new();
    for pattern in state.patterns.values() {
        by_category
            .entry(pattern.category.as_deref().unwrap_or("uncategorized"))
            .or_default()
            .push(pattern);
    }

    let mut output = format!("📋 {} patterns defined\n", state.patterns.len());
    for (category, patterns) in by_category {
        output.push_str(&format!("\n## {} ({})\n", category, patterns.len()));
        let mut patterns = patterns;
        patterns.sort_by(|a, b| a.name.cmp(&b.name));
        for pattern in patterns {
            output.push_str(&format!(
                "- {}: {} notes, {:.1} s, {} bars",
                pattern.name,
                pattern.notes.len(),
                pattern.get_pattern_duration(),
                pattern.pattern_bars
            ));
            if let Some(desc) = &pattern.description {
                output.push_str(&format!(" — {}", desc));
            }
            if !pattern.tags.is_empty() {
                output.push_str(&format!(" [{}]", pattern.tags.join(", ")));
            }
            output.push('\n');
        }
    }
    JsonRpcResponse::tool_text(id, output)
}

fn handle_stop_playback(state: &mut ServerState, id: Option<Value>) -> JsonRpcResponse {
    let stopped = state.player.as_mut().map(|p| p.stop_all()).unwrap_or(0);
    JsonRpcResponse::tool_text(id, format!("⏹ Stopped {} active playback(s)", stopped))
}

fn write_response(stdout: &mut impl Write, response: &JsonRpcResponse) {
    match serde_json::to_string(response) {
        Ok(json) => {
            tracing::debug!("Sending: {}", json);
            let _ = writeln!(stdout, "{}", json);
            let _ = stdout.flush();
        }
        Err(e) => tracing::error!("Failed to serialize response: {}", e),
    }
}

pub fn run_stdio_server() {
    tracing::info!(
        "MCP server starting (mcp-muse {})",
        env!("CARGO_PKG_VERSION")
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = stdin.lock();
    let mut state = ServerState::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                tracing::error!("Error reading from stdin: {}", e);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        tracing::debug!("Received: {}", line);

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                tracing::error!("Failed to parse JSON-RPC request: {}", e);
                let mut response = JsonRpcResponse::error(None, PARSE_ERROR, "Parse error");
                if let Some(err) = response.error.as_mut() {
                    err.data = Some(json!(e.to_string()));
                }
                write_response(&mut stdout, &response);
                continue;
            }
        };

        // A request without an id is a notification: never answer it.
        let Some(id) = request.id else {
            tracing::info!("Notification: {}", request.method);
            continue;
        };
        let id = Some(id);

        let response = match request.method.as_str() {
            "initialize" => handle_initialize(request.params, id),
            "ping" => JsonRpcResponse::ok(id, json!({})),
            "tools/list" => handle_tools_list(id),
            "resources/list" => handle_resources_list(id),
            "prompts/list" => handle_prompts_list(id),
            "tools/call" => handle_tool_call(&mut state, request.params, id),
            _ => JsonRpcResponse::error(id, METHOD_NOT_FOUND, "Method not found"),
        };
        write_response(&mut stdout, &response);
    }

    tracing::info!("MCP server shutting down");
}
