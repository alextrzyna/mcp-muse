use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::expressive::{
    FmAlgorithm, GrainSource, LfoTarget, LfoWave, Patch, PatchLibrary, PercussionKind, TableName,
};
use crate::midi::export::{
    BitDepth, ExportReport, ExportRequest, Split, bus_chain_is_nonlinear, export, sanitize_name,
};
use crate::midi::external::{ExternalMidi, Outputs, VIRTUAL_PORT_NAME};
use crate::midi::{
    ExtendedSequence, MidiPlayer, PitchMode, PlayMode, SequencePattern, SimpleNote, SimpleSequence,
    validate_tempo,
};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::time::Duration;

/// MCP protocol revision this server implements.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Per-process server state: the audio player (opened on first use so that
/// `tools/list` works without an audio device), the MIDI outputs on this
/// machine (the virtual port is published from the start so a DAW can
/// select it before anything plays), the session's patterns and the
/// session's `define_synth` patches (keyed by `Patch::key()`).
pub struct ServerState {
    player: Option<MidiPlayer>,
    external: ExternalMidi,
    patterns: HashMap<String, SequencePattern>,
    synths: HashMap<String, Patch>,
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
            external: ExternalMidi::new(),
            patterns: HashMap::new(),
            synths: HashMap::new(),
        }
    }

    /// The player and the external outputs, opening the audio device on
    /// first use. Both are borrowed at once because a play call resolves
    /// `midi_out` names while it translates.
    fn player(&mut self) -> Result<(&mut MidiPlayer, &mut ExternalMidi), String> {
        if self.player.is_none() {
            self.player = Some(MidiPlayer::new(Some(self.external.sender()))?);
            tracing::info!("Opened audio output stream");
        }
        let player = self.player.as_mut().expect("player just initialised");
        Ok((player, &mut self.external))
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

/// The accepted `pitch_mode` values, taken from the enum itself so the schema
/// and the `list_sounds` catalog cannot drift from what serde will parse.
fn pitch_modes() -> Vec<&'static str> {
    PitchMode::ALL.iter().map(PitchMode::as_str).collect()
}

/// JSON schema for the `effects` chain, shared by `note_schema` and
/// `patch_schema` so the two cannot drift apart.
fn effects_schema() -> Value {
    json!({
        "type": "array",
        "description": "🎛️ Effects chain applied in order. Each entry is a flat object: {\"type\": \"reverb\"|\"delay\"|\"chorus\"|\"filter\"|\"compressor\"|\"distortion\", ...parameters, \"intensity\": 0-1}. Example: [{\"type\": \"reverb\", \"room_size\": 0.7, \"wet_level\": 0.4, \"intensity\": 0.6}, {\"type\": \"delay\", \"delay_time\": 0.25, \"feedback\": 0.3, \"intensity\": 0.5}]. Time Fracture (a pitch-shifting, tempo-wandering delay): [{\"type\": \"delay\", \"random_beats\": [0.5, 1.0], \"random_rate\": 0.5, \"pitch_intervals\": [7, 12], \"pitch_mode\": \"up_down\", \"feedback\": 0.5, \"intensity\": 0.6}]. The parameters below are listed together for readability, but only the ones belonging to the entry's own \"type\" are accepted: any other key is rejected with a -32602 naming it.",
        "items": {
            "type": "object",
            "properties": {
                "type": {"type": "string", "enum": ["reverb", "delay", "chorus", "filter", "compressor", "distortion"], "description": "Effect type (required)"},
                "intensity": {"type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.5, "description": "Wet/dry mix: 0.3=subtle, 0.6=moderate, 1.0=maximum"},
                "enabled": {"type": "boolean", "default": true},
                "room_size": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "reverb: 0.1=closet, 0.5=studio, 0.8=hall, 1.0=cathedral (default 0.5)"},
                "dampening": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "reverb: high-frequency damping 0=bright, 1=dark (default 0.3)"},
                "wet_level": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "reverb/delay: wet amount (default 0.3)"},
                "pre_delay": {"type": "number", "minimum": 0.0, "maximum": 0.2, "description": "reverb: seconds before the reverb starts (default 0.02)"},
                "delay_time": {"type": "number", "minimum": 0.01, "maximum": 2.0, "description": "delay: seconds, or beats of the sequence tempo when sync_tempo is true; 0.25 = quarter note at 120 BPM (default 0.25). The resulting delay is capped at 8 s"},
                "feedback": {"type": "number", "minimum": 0.0, "maximum": 0.95, "description": "delay/chorus: repeat amount (delay default 0.4, chorus default 0.2)"},
                "sync_tempo": {"type": "boolean", "description": "delay: when true, delay_time is in beats of the sequence tempo"},
                "random_beats": {"type": "array", "items": {"type": "number", "minimum": 0, "maximum": 4}, "minItems": 2, "maxItems": 2,
                    "description": "delay (Time Fracture): [min, max] delay in beats of the sequence tempo; replaces delay_time. Equal values = a fixed beat-synced delay"},
                "random_rate": {"type": "number", "minimum": 0, "maximum": 10, "default": 0,
                    "description": "delay (Time Fracture): how fast the delay time wanders between min and max, in Hz (0 = fixed at min)"},
                "pitch_intervals": {"type": "array", "items": {"type": "number", "minimum": -12, "maximum": 12}, "maxItems": 12,
                    "description": "delay (Time Fracture): semitone shifts for successive repeats, e.g. [7, 12] for a fifth-and-octave shimmer; rounded to whole semitones"},
                "pitch_mode": {"type": "string", "enum": pitch_modes(), "default": "random",
                    "description": "delay (Time Fracture): order the pitch_intervals are visited in"},
                "rate": {"type": "number", "minimum": 0.1, "maximum": 8.0, "description": "chorus: LFO Hz (default 1.5)"},
                "depth": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "chorus: modulation depth (default 0.3)"},
                "stereo_width": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "chorus: reserved"},
                "filter_type": {"type": "string", "enum": ["low_pass", "high_pass", "band_pass", "notch", "peak", "low_shelf", "high_shelf"], "description": "filter: response (default low_pass)"},
                "cutoff": {"type": "number", "minimum": 20.0, "maximum": 20000.0, "description": "filter: Hz (default 1000)"},
                "resonance": {"type": "number", "minimum": 0.1, "maximum": 20.0, "description": "filter: Q (default 1.0)"},
                "envelope_amount": {"type": "number", "minimum": -1.0, "maximum": 1.0, "description": "filter: reserved"},
                "threshold": {"type": "number", "minimum": -60.0, "maximum": 0.0, "description": "compressor: dB (default -12)"},
                "ratio": {"type": "number", "minimum": 1.0, "maximum": 20.0, "description": "compressor: 2=subtle, 4=moderate, 8=heavy (default 4)"},
                "attack": {"type": "number", "minimum": 0.001, "maximum": 0.1, "description": "compressor: seconds (default 0.01)"},
                "release": {"type": "number", "minimum": 0.01, "maximum": 2.0, "description": "compressor: seconds (default 0.1)"},
                "drive": {"type": "number", "minimum": 0.0, "maximum": 5.0, "description": "distortion: 1=warm, 2.5=crunch, 5=heavy (default 2)"},
                "tone": {"type": "number", "minimum": 0.0, "maximum": 1.0, "description": "distortion: 0=dark, 1=bright (default 0.5)"},
                "output_level": {"type": "number", "minimum": 0.1, "maximum": 2.0, "description": "distortion: output gain (default 1.0)"}
            },
            "required": ["type"]
        }
    })
}

/// JSON schema for a synth patch, shared by define_synth and inline `synth` on notes.
fn patch_schema() -> Value {
    let env = |what: &str| {
        json!({
            "type": "object",
            "description": format!("{what} envelope in seconds (0-10) and sustain 0-1. Defaults: attack 0.01, decay 0.1, sustain 0.8, release 0.3"),
            "properties": {
                "attack": {"type": "number", "minimum": 0, "maximum": 10},
                "decay": {"type": "number", "minimum": 0, "maximum": 10},
                "sustain": {"type": "number", "minimum": 0, "maximum": 1},
                "release": {"type": "number", "minimum": 0, "maximum": 10}
            },
            "additionalProperties": false
        })
    };
    let wave = json!({"type": "string", "enum": ["sine", "saw", "square", "triangle", "noise"], "default": "saw"});
    json!({
        "type": "object",
        "description": "A synth patch. Include at least one engine (subtractive, fm, wavetable, granular or percussion); several may layer. An optional lfo modulates one target.",
        "properties": {
            "name": {"type": "string", "description": "Patch name; notes reference it with \"synth\": \"<name>\""},
            "description": {"type": "string"},
            "category": {"type": "string", "enum": ["bass", "pad", "lead", "keys", "drums", "fx"]},
            "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1, "description": "Patch output level; velocity 127 plays at this level"},
            "subtractive": {
                "type": "object",
                "description": "Two oscillators, optional state-variable filter with its own envelope, amplitude envelope",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "osc1": {"type": "object", "properties": {"wave": wave, "pulse_width": {"type": "number", "minimum": 0.1, "maximum": 0.9, "default": 0.5}}, "additionalProperties": false},
                    "osc2": {"type": "object", "description": "Second oscillator blended with osc1",
                        "properties": {"wave": wave, "pulse_width": {"type": "number", "minimum": 0.1, "maximum": 0.9, "default": 0.5},
                            "mix": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "0 = only osc1, 1 = only osc2"},
                            "detune_cents": {"type": "number", "minimum": -100, "maximum": 100, "default": 0, "description": "5-15 thickens, 50+ beats audibly"},
                            "octave": {"type": "integer", "minimum": -2, "maximum": 2, "default": 0}},
                        "additionalProperties": false},
                    "filter": {"type": "object",
                        "properties": {"type": {"type": "string", "enum": ["low_pass", "high_pass", "band_pass"], "default": "low_pass"},
                            "cutoff": {"type": "number", "minimum": 20, "maximum": 20000, "default": 1000},
                            "resonance": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.2},
                            "slope": {"type": "integer", "enum": [12, 24], "default": 12, "description": "dB per octave"},
                            "env_amount": {"type": "number", "minimum": -1, "maximum": 1, "default": 0, "description": "Filter envelope depth; 1 sweeps up four octaves, -1 down"},
                            "env": env("Filter")},
                        "additionalProperties": false},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
            "fm": {
                "type": "object",
                "description": "Four-operator FM. Operator 1 (first entry) is always a carrier; a carrier's level is its volume, a modulator's level is its modulation depth (1 = about 4 radians). Bells: high non-integer ratios with fast-decaying modulators; basses: ratio 1-2 modulators with a short decay.",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "algorithm": {"type": "string", "enum": ["stack", "pairs", "fan_in", "parallel"], "default": "stack",
                        "description": "stack: 4->3->2->1, one carrier. pairs: 3->1 and 4->2, carriers 1 and 2. fan_in: 2, 3, 4 all modulate 1. parallel: all carriers (additive)"},
                    "feedback": {"type": "number", "minimum": 0, "maximum": 1, "default": 0, "description": "Self-modulation of the last operator in radians (0 to 1); adds grit. A modulator's level 1 is 4 radians, so feedback is a gentler effect"},
                    "operators": {
                        "type": "array", "minItems": 1, "maxItems": 4,
                        "description": "1 to 4 operators in order; missing operators are silent",
                        "items": {"type": "object", "properties": {
                            "ratio": {"type": "number", "minimum": 0.25, "maximum": 16, "default": 1, "description": "Frequency relative to the note (2 = one octave up, 3.5 = bell-like)"},
                            "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                            "detune_cents": {"type": "number", "minimum": -100, "maximum": 100, "default": 0},
                            "env": env("Operator")
                        }, "additionalProperties": false}
                    }
                },
                "additionalProperties": false
            },
            "wavetable": {
                "type": "object",
                "description": "One of eight band-limited tables, optionally morphed toward the next table in the list, through an amplitude envelope",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "table": {"type": "string", "enum": ["basic", "warm", "bright", "digital", "vocal", "pwm", "organ", "noise"], "default": "basic",
                        "description": "basic: soft sine+; warm: odd harmonics; bright: saw-like; digital: inharmonic/metallic; vocal: formant peaks; pwm: stacked pulse widths; organ: drawbars; noise: dense harmonics with scattered phases"},
                    "morph": {"type": "number", "minimum": 0, "maximum": 1, "default": 0, "description": "0 = the chosen table, 1 = the next table in the list (noise wraps to basic)"},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
            "granular": {
                "type": "object",
                "description": "A cloud of short Hann-windowed grains read from a one-cycle source at the note's pitch; the first engine with true stereo. Long grains + low density = smooth; short grains + high density = dense texture.",
                "properties": {
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "source": {"type": "string", "enum": ["harmonics", "noise", "formant", "inharmonic"], "default": "harmonics",
                        "description": "harmonics: warm; noise: pitched buzz; formant: vowel-like; inharmonic: bell-like"},
                    "grain_ms": {"type": "number", "minimum": 5, "maximum": 500, "default": 50, "description": "Grain length in ms"},
                    "density": {"type": "number", "minimum": 1, "maximum": 50, "default": 10, "description": "Grains started per second"},
                    "pitch_semitones": {"type": "number", "minimum": -24, "maximum": 24, "default": 0},
                    "randomness": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.2, "description": "Random grain start position; higher = more chaotic"},
                    "stereo_width": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "Random stereo placement per grain"},
                    "env": env("Amplitude")
                },
                "additionalProperties": false
            },
            "percussion": {
                "type": "object",
                "description": "One-shot hit with its own envelope; ignores the note's pitch. Only the parameters of the chosen kind are allowed.",
                "properties": {
                    "kind": {"type": "string", "enum": ["kick", "snare", "hihat", "cymbal", "zap", "swoosh", "chime", "burst"]},
                    "level": {"type": "number", "minimum": 0, "maximum": 1, "default": 1},
                    "frequency": {"type": "number", "minimum": 20, "maximum": 20000, "description": "Body (kick 60), tone (snare 200), base (hihat 8000, cymbal 4000), start (zap 800), fundamental (chime 880) or centre (burst 1000)"},
                    "punch": {"type": "number", "minimum": 0, "maximum": 1, "description": "kick"},
                    "sustain": {"type": "number", "minimum": 0, "maximum": 1, "description": "kick"},
                    "click_freq": {"type": "number", "minimum": 20, "maximum": 20000, "description": "kick"},
                    "snap": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "buzz": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "noise_amount": {"type": "number", "minimum": 0, "maximum": 1, "description": "snare"},
                    "metallic": {"type": "number", "minimum": 0, "maximum": 1, "description": "hihat, cymbal"},
                    "decay": {"type": "number", "minimum": 0.01, "maximum": 10, "description": "hihat, zap, chime: decay time in seconds (larger = longer ring)"},
                    "brightness": {"type": "number", "minimum": 0, "maximum": 1, "description": "hihat"},
                    "size": {"type": "number", "minimum": 0, "maximum": 1, "description": "cymbal"},
                    "strike_intensity": {"type": "number", "minimum": 0, "maximum": 1, "description": "cymbal"},
                    "energy": {"type": "number", "minimum": 0, "maximum": 1, "description": "zap"},
                    "harmonic_content": {"type": "number", "minimum": 0, "maximum": 1, "description": "zap"},
                    "direction": {"type": "number", "minimum": -1, "maximum": 1, "description": "swoosh"},
                    "intensity": {"type": "number", "minimum": 0, "maximum": 1, "description": "swoosh, burst"},
                    "sweep": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "swoosh [start_hz, end_hz]"},
                    "harmonic_count": {"type": "integer", "minimum": 1, "maximum": 16, "description": "chime"},
                    "inharmonicity": {"type": "number", "minimum": 0, "maximum": 1, "description": "chime"},
                    "bandwidth": {"type": "number", "minimum": 1, "maximum": 20000, "description": "burst"},
                    "shape": {"type": "number", "minimum": 0, "maximum": 1, "description": "burst: 0 sharp, 1 smooth"}
                },
                "required": ["kind"],
                "additionalProperties": false
            },
            "lfo": {
                "type": "object",
                "description": "One free-running LFO per patch routed to a single target. Depth 1 = cutoff ±2 octaves, pitch ±2 semitones, amplitude down to silence, morph ±0.5, grain density 0.5x-2x.",
                "properties": {
                    "rate": {"type": "number", "minimum": 0.1, "maximum": 20, "default": 1, "description": "Hz"},
                    "depth": {"type": "number", "minimum": 0, "maximum": 1, "default": 0},
                    "wave": {"type": "string", "enum": ["sine", "triangle", "saw", "square", "sample_hold"], "default": "sine"},
                    "target": {"type": "string", "enum": ["off", "cutoff", "pitch", "amplitude", "morph", "grain_density"], "default": "off",
                        "description": "cutoff: subtractive filter; pitch: every pitched engine (vibrato); amplitude: tremolo; morph: wavetable; grain_density: granular"}
                },
                "additionalProperties": false
            },
            "effects": effects_schema()
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

/// JSON schema for one note, shared by play_notes, define_sequence_pattern
/// and play_sequence so the three tools cannot drift apart.
fn note_schema() -> Value {
    // Effects on a note drive the MIDI bus chain or the R2D2 buffer; a synth
    // note renders through its patch's own chain, so both fields are rejected
    // there rather than silently dropped.
    let note_effects = {
        let mut schema = effects_schema();
        let base = schema["description"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        schema["description"] = json!(format!(
            "{base} Applies to MIDI and R2D2 notes only; a synth note takes its effects from its patch's \"effects\" chain (define_synth or an inline patch)."
        ));
        schema
    };
    json!({
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
                "description": "⏰ Start time in seconds (at most 300). Use 0.0 for simultaneous notes (chords), incremental timing for melodies. DEPRECATED: Consider using musical_time for better sync.",
                "maximum": 300
            },
            "duration": {
                "type": "number",
                "description": "⏳ Note duration in seconds (at most 300). Try: 0.25=16th, 0.5=8th, 1.0=quarter, 2.0=half, 4.0=whole note. DEPRECATED: Consider using musical_duration for better sync.",
                "maximum": 300
            },
            "musical_time": {
                "type": "object",
                "description": "🎼 Musical timing (bar.beat.tick) - Alternative to start_time for precise timing",
                "properties": {
                    "bar": {"type": "integer", "minimum": 1, "description": "Bar number (1-based)"},
                    "beat": {"type": "integer", "minimum": 1, "maximum": 8, "description": "Beat within bar (1-based, up to beats_per_bar)"},
                    "tick": {"type": "integer", "minimum": 0, "maximum": 479, "description": "Tick within beat (0-479)"}
                },
                "required": ["bar", "beat", "tick"]
            },
            "musical_duration": {
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
            "effects": note_effects,
            "synth": {
                "description": "🎛️ Synth patch for this note: the name of a built-in or define_synth patch, or an inline patch object. Pitch comes from `note`; percussion patches ignore it.",
                "oneOf": [
                    {"type": "string"},
                    patch_schema()
                ]
            },
            "midi_out": {
                "type": "string",
                "description": "🔌 Send this note as MIDI to an instrument on this machine instead of the built-in synth: \"mcp-muse\" is this server's virtual port (select it as a MIDI input in Bitwig or another DAW), or name a destination from list_sounds section \"midi_outputs\". The note's channel picks the DAW track. instrument is sent as a program change only when given; effects and effects_preset are not available (the receiving instrument makes the sound)."
            },
            "effects_preset": {
                "type": "string",
                "description": "🎭 EFFECTS PRESET: Apply curated effect combinations to MIDI and R2D2 notes (a synth note takes its effects from its patch's \"effects\" chain instead). Choose from professional presets: 'studio' (clean + subtle reverb), 'concert_hall' (spacious reverb), 'vintage' (analog warmth), 'ambient' (lush atmospheric), 'live_stage' (punchy compression), 'tight_mix' (controlled dynamics), 'dreamy' (soft ethereal), 'spacious' (wide reverb), 'analog_warmth' (tube character), 'retro_echo' (tape delay), 'psychedelic' (wild modulation), 'distorted' (aggressive), 'filtered' (prominent filtering), 'lush_chorus' (rich modulation). Effects presets provide instant professional sound character!",
                "enum": ["studio", "concert_hall", "vintage", "ambient", "live_stage", "tight_mix", "dreamy", "spacious", "analog_warmth", "retro_echo", "psychedelic", "distorted", "filtered", "lush_chorus"]
            }
        },
        "anyOf": [
            {"required": ["start_time", "duration"]},
            {"required": ["musical_time", "musical_duration"]}
        ],
        "additionalProperties": false
    })
}

/// Schema of one entry in a `patterns` array (shared by play_sequence and export_audio).
fn pattern_reference_schema() -> Value {
    json!({
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
    })
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling tools/list request");

    let tools = json!([
        {
            "name": "define_synth",
            "description": "Define a reusable synth patch for this session, then play it with \"synth\": \"<name>\" on notes in play_notes, define_sequence_pattern or play_sequence. Engines: subtractive (two oscillators, filter with envelope), fm (four operators, four algorithms), wavetable (eight band-limited tables with morph), granular (grain cloud with stereo width) and percussion (kick/snare/hihat/cymbal/zap/swoosh/chime/burst). An optional lfo modulates one target (cutoff/pitch/amplitude/morph/grain_density). An ordered effects chain applies once to all notes of the patch, so reverb and delay tails are shared.

Examples:
- Bass: {\"name\": \"rubber_bass\", \"category\": \"bass\", \"subtractive\": {\"osc1\": {\"wave\": \"saw\"}, \"osc2\": {\"wave\": \"square\", \"mix\": 0.3, \"detune_cents\": 6}, \"filter\": {\"type\": \"low_pass\", \"cutoff\": 500, \"resonance\": 0.4, \"slope\": 24, \"env_amount\": 0.7, \"env\": {\"attack\": 0.005, \"decay\": 0.25, \"sustain\": 0.1, \"release\": 0.2}}, \"env\": {\"attack\": 0.005, \"decay\": 0.3, \"sustain\": 0.6, \"release\": 0.15}}, \"effects\": [{\"type\": \"distortion\", \"drive\": 3, \"intensity\": 0.4}, {\"type\": \"compressor\", \"threshold\": -18, \"ratio\": 4, \"intensity\": 1}]}
- Pad: {\"name\": \"glass_pad\", \"category\": \"pad\", \"level\": 0.7, \"subtractive\": {\"osc1\": {\"wave\": \"triangle\"}, \"osc2\": {\"wave\": \"saw\", \"mix\": 0.35, \"detune_cents\": 9}, \"filter\": {\"cutoff\": 900, \"env_amount\": 0.5, \"env\": {\"attack\": 1.2, \"decay\": 2, \"sustain\": 0.4, \"release\": 3}}, \"env\": {\"attack\": 0.9, \"decay\": 1, \"sustain\": 0.8, \"release\": 2.5}}, \"effects\": [{\"type\": \"chorus\", \"rate\": 0.5, \"depth\": 0.4, \"intensity\": 0.3}, {\"type\": \"reverb\", \"room_size\": 0.8, \"intensity\": 0.4}]}
- Drum: {\"name\": \"tight_kick\", \"category\": \"drums\", \"percussion\": {\"kind\": \"kick\", \"frequency\": 50, \"punch\": 0.9, \"sustain\": 0.2}}
- Bell: {\"name\": \"glass_bell\", \"category\": \"keys\", \"fm\": {\"algorithm\": \"stack\", \"operators\": [{\"ratio\": 1, \"env\": {\"attack\": 0.002, \"decay\": 1.5, \"sustain\": 0.2, \"release\": 2.5}}, {\"ratio\": 3.5, \"level\": 0.55, \"env\": {\"decay\": 0.6, \"sustain\": 0}}]}, \"effects\": [{\"type\": \"reverb\", \"room_size\": 0.7, \"intensity\": 0.35}]}
- Organ: {\"name\": \"organ\", \"category\": \"keys\", \"wavetable\": {\"table\": \"organ\", \"env\": {\"attack\": 0.005, \"sustain\": 1, \"release\": 0.15}}}
- Texture: {\"name\": \"cloud\", \"category\": \"pad\", \"level\": 0.6, \"granular\": {\"source\": \"formant\", \"grain_ms\": 120, \"density\": 15, \"pitch_semitones\": 7, \"randomness\": 0.7, \"stereo_width\": 0.9, \"env\": {\"attack\": 1.5, \"release\": 3}}, \"lfo\": {\"rate\": 0.2, \"depth\": 0.4, \"target\": \"grain_density\"}, \"effects\": [{\"type\": \"reverb\", \"room_size\": 0.9, \"intensity\": 0.5}]}
- Shimmer: {\"name\": \"shimmer\", \"category\": \"keys\", \"fm\": {\"algorithm\": \"stack\", \"operators\": [{\"ratio\": 1, \"env\": {\"decay\": 1.5, \"sustain\": 0.2, \"release\": 2}}, {\"ratio\": 3.5, \"level\": 0.5, \"env\": {\"decay\": 0.5, \"sustain\": 0}}]}, \"effects\": [{\"type\": \"delay\", \"random_beats\": [0.75, 0.75], \"pitch_intervals\": [7, 12], \"pitch_mode\": \"up\", \"feedback\": 0.5, \"intensity\": 0.6}, {\"type\": \"reverb\", \"room_size\": 0.8, \"intensity\": 0.4}]}

Call list_sounds with section \"synths\" to see the built-in patches, which double as worked examples.",
            "inputSchema": patch_schema()
        },
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
                        "items": note_schema()
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Default tempo for this pattern (can be overridden when referenced)",
                        "minimum": 20,
                        "maximum": 300,
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
                        "items": note_schema()
                    },
                    "patterns": {
                        "type": "array",
                        "description": "🎼 Pattern references with transformations",
                        "items": pattern_reference_schema()
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Tempo in BPM for the entire sequence",
                        "minimum": 20,
                        "maximum": 300,
                        "default": 120
                    },
                    "beats_per_bar": {
                        "type": "integer",
                        "description": "🎶 Time signature numerator used to convert musical_time and musical_duration (4 for 4/4, 3 for 3/4)",
                        "minimum": 2,
                        "maximum": 8,
                        "default": 4
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["replace", "layer"],
                        "default": "replace",
                        "description": "replace (default) stops whatever is playing before this starts; layer mixes this on top of the current playback. A layered call gets its own MIDI channels (its programs, pan, volume and sustain do not touch what is already playing; drums always share channel 9), and a layered call that specifies effects swaps the MIDI bus chain immediately."
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
            "name": "list_sounds",
            "description": "Catalog of every sound this server can make or reach: synth patches (built-in and this session's define_synth patches), the 128 General MIDI instruments, drum keys for channel 9, R2D2 emotions, effect types and effects presets, and the MIDI outputs on this machine (the server's virtual port for a DAW such as Bitwig, plus any other destination) for a note's midi_out. Call this before guessing an instrument, synth or output name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "Limit the catalog to one section",
                        "enum": ["all", "synths", "instruments", "drums", "r2d2", "effects", "midi_outputs"],
                        "default": "all"
                    }
                },
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
            "description": "Play quick sounds, effects, and simple melodies. Supports MIDI (128 instruments), R2D2 expressions (9 emotions) and synth patches (built-in or define_synth). For complex compositions with 3+ notes, use define_sequence_pattern + play_sequence instead.

Examples:
- Success chime: [{\"note\": 72, \"instrument\": 9, \"duration\": 0.5}]
- R2D2 happy: [{\"note_type\": \"r2d2\", \"r2d2_emotion\": \"Happy\", \"r2d2_intensity\": 0.8, \"r2d2_complexity\": 2, \"duration\": 1.0}]
- Drum kick: [{\"synth\": \"tr_808_kick\", \"duration\": 0.5}]
- Inline synth: [{\"synth\": {\"name\": \"blip\", \"subtractive\": {\"osc1\": {\"wave\": \"square\"}, \"env\": {\"release\": 0.05}}}, \"note\": 84, \"duration\": 0.1}]
- Inline FM bell: [{\"synth\": {\"name\": \"glass_bell\", \"fm\": {\"algorithm\": \"stack\", \"operators\": [{\"ratio\": 1, \"env\": {\"attack\": 0.002, \"decay\": 1.5, \"sustain\": 0.2, \"release\": 2.5}}, {\"ratio\": 3.5, \"level\": 0.55, \"env\": {\"decay\": 0.6, \"sustain\": 0}}]}, \"effects\": [{\"type\": \"reverb\", \"room_size\": 0.7, \"intensity\": 0.35}]}, \"note\": 76, \"duration\": 0.3}]

Pass \"mode\": \"layer\" to play over what is already sounding; the default replaces it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notes": {
                        "type": "array",
                        "description": "Array of notes to play",
                        "items": note_schema()
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "Tempo in BPM (optional, defaults to 120)",
                        "minimum": 20,
                        "maximum": 300
                    },
                    "beats_per_bar": {
                        "type": "integer",
                        "description": "🎶 Time signature numerator used to convert musical_time and musical_duration (4 for 4/4, 3 for 3/4)",
                        "minimum": 2,
                        "maximum": 8,
                        "default": 4
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["replace", "layer"],
                        "default": "replace",
                        "description": "replace (default) stops whatever is playing before this starts; layer mixes this on top of the current playback. A layered call gets its own MIDI channels (its programs, pan, volume and sustain do not touch what is already playing; drums always share channel 9), and a layered call that specifies effects swaps the MIDI bus chain immediately."
                    }
                },
                "required": ["notes"]
            }
        },
        {
            "name": "export_audio",
            "description": "Save a composition to disk as WAV instead of playing it. Takes the same notes, patterns, tempo and beats_per_bar as play_sequence and renders offline (nothing is heard, live playback is untouched). split selects what is written: \"mixdown\" (default) is one stereo file; \"stems\" is one file per sound source (each MIDI channel, each synth patch, all R2D2 together) with every effect baked in so they sum back to the mix; \"tracks\" is the same split with the MIDI bus, patch and R2D2 effect chains bypassed, for mixing elsewhere. Every file in one export has the same length so they line up at zero in a DAW. MIDI channel stems each carry their own copy of the MIDI bus chain, so a bus with a compressor, distortion or Time Fracture delay does not sum back exactly; synth and R2D2 stems always do.

Example: {\"patterns\": [{\"pattern_name\": \"drums\", \"start_bar\": 1, \"repeat_count\": 4}], \"path\": \"/Users/me/Music/demo\", \"name\": \"take1\", \"split\": \"stems\"} writes /Users/me/Music/demo/take1/ch09_drums.wav and friends.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notes": {
                        "type": "array",
                        "description": "🎵 Individual notes (same format as play_notes)",
                        "items": note_schema()
                    },
                    "patterns": {
                        "type": "array",
                        "description": "🎼 Pattern references with transformations (same format as play_sequence)",
                        "items": pattern_reference_schema()
                    },
                    "tempo": {
                        "type": "integer",
                        "description": "🎵 Tempo in BPM for the entire sequence",
                        "minimum": 20,
                        "maximum": 300,
                        "default": 120
                    },
                    "beats_per_bar": {
                        "type": "integer",
                        "description": "🎶 Time signature numerator used to convert musical_time and musical_duration (4 for 4/4, 3 for 3/4)",
                        "minimum": 2,
                        "maximum": 8,
                        "default": 4
                    },
                    "path": {
                        "type": "string",
                        "description": "📁 Absolute directory to write into; created if missing. A mixdown is written as <path>/<name>.wav, stems and tracks as <path>/<name>/<source>.wav."
                    },
                    "name": {
                        "type": "string",
                        "description": "🏷️ Base file or folder name. Letters, digits, '_' and '-' are kept; anything else becomes '_'.",
                        "default": "mix"
                    },
                    "split": {
                        "type": "string",
                        "enum": ["mixdown", "stems", "tracks"],
                        "default": "mixdown",
                        "description": "mixdown: one soft-clipped stereo file. stems: one file per source with effects, not clipped. tracks: one file per source with the effect chains bypassed, not clipped."
                    },
                    "bit_depth": {
                        "type": "integer",
                        "enum": [16, 24, 32],
                        "default": 24,
                        "description": "WAV sample format: 16 or 24-bit integer (peaks above 0 dBFS are clamped and reported) or 32 for IEEE float (never clamps)."
                    },
                    "overwrite": {
                        "type": "boolean",
                        "default": false,
                        "description": "Replace files that already exist. Without it an existing target is an error listing the collisions."
                    }
                },
                "required": ["path"]
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

    // A bug inside a render must not kill the server; the model gets an
    // isError result and the next call still works.
    let name = tool_params.name.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch_tool(state, tool_params, id.clone())
    }));
    outcome.unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".to_string());
        tracing::error!("Tool {} panicked: {}", name, message);
        JsonRpcResponse::tool_error(
            id,
            format!("Internal error while running {}: {}", name, message),
        )
    })
}

fn dispatch_tool(
    state: &mut ServerState,
    tool_params: ToolCallParams,
    id: Option<Value>,
) -> JsonRpcResponse {
    match tool_params.name.as_str() {
        "define_synth" => handle_define_synth(state, tool_params.arguments, id),
        "play_notes" => handle_play_notes(state, tool_params.arguments, id),
        "define_sequence_pattern" => handle_define_pattern(state, tool_params.arguments, id),
        "play_sequence" => handle_play_sequence(state, tool_params.arguments, id),
        "list_patterns" => handle_list_patterns(state, id),
        "list_sounds" => handle_list_sounds(state, tool_params.arguments, id),
        "stop_playback" => handle_stop_playback(state, id),
        "export_audio" => handle_export_audio(state, tool_params.arguments, id),
        other => JsonRpcResponse::error(id, METHOD_NOT_FOUND, format!("Unknown tool: {}", other)),
    }
}

fn handle_define_synth(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let patch: Patch = match serde_json::from_value(arguments) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Failed to parse synth patch: {}", e),
            );
        }
    };
    if let Err(e) = patch.validate() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            format!("Invalid synth patch '{}': {}", patch.name, e),
        );
    }
    let mut engines = Vec::new();
    if patch.subtractive.as_ref().is_some_and(|s| s.level > 0.0) {
        engines.push("subtractive");
    }
    if patch.fm.as_ref().is_some_and(|f| f.level > 0.0) {
        engines.push("fm");
    }
    if patch.wavetable.as_ref().is_some_and(|w| w.level > 0.0) {
        engines.push("wavetable");
    }
    if patch.granular.as_ref().is_some_and(|g| g.level > 0.0) {
        engines.push("granular");
    }
    if let Some(p) = patch.percussion.as_ref().filter(|p| p.level > 0.0) {
        engines.push(p.kind.as_str());
    }
    let shadowed = PatchLibrary::new().get(&patch.name).is_some();
    let mut details = format!(
        "🎛️ Defined synth '{}': {} engine(s) [{}], {} effect(s)",
        patch.name,
        engines.len(),
        engines.join(", "),
        patch.effects.iter().filter(|e| e.enabled).count(),
    );
    if let Some(lfo) = patch.lfo.as_ref().filter(|l| l.is_active()) {
        details.push_str(&format!(" + lfo → {}", lfo.target.as_str()));
    }
    if shadowed {
        details.push_str(" (shadows the built-in patch of the same name for this session)");
    }
    if !patch.description.is_empty() {
        details.push_str(&format!("\n{}", patch.description));
    }
    details.push_str(&format!(
        "\n\nPlay it with play_notes, e.g. {{\"notes\": [{{\"synth\": \"{}\", \"note\": 48, \"duration\": 1}}]}}",
        patch.name
    ));
    tracing::info!("Stored synth patch '{}'", patch.name);
    state.synths.insert(patch.key(), patch);
    JsonRpcResponse::tool_text(id, details)
}

/// Parameter-level validation shared by every tool that accepts notes.
fn validate_notes(notes: &[SimpleNote]) -> Result<(), String> {
    for (i, note) in notes.iter().enumerate() {
        let checks = [
            ("timing", note.validate_timing()),
            ("synth", note.validate_synth()),
            ("midi_out", note.validate_midi_out()),
            ("R2D2", note.validate_r2d2()),
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
    let mut parts: Vec<String> = Vec::new();
    if notes.iter().any(|n| n.note_type == "r2d2") {
        parts.push("R2D2 expressions".into());
    }
    if notes.iter().any(|n| n.is_synthesis()) {
        parts.push("synth patches".into());
    }
    if notes
        .iter()
        .any(|n| n.note_type != "r2d2" && !n.is_synthesis() && !n.is_external())
    {
        parts.push("MIDI instruments".into());
    }
    let mut outputs: Vec<&str> = notes
        .iter()
        .filter_map(|n| n.midi_out.as_deref())
        .map(str::trim)
        .collect();
    outputs.sort_unstable();
    outputs.dedup();
    if !outputs.is_empty() {
        parts.push(format!("external MIDI ({})", outputs.join(", ")));
    }
    if parts.is_empty() {
        "audio".to_string()
    } else {
        parts.join(" + ")
    }
}

/// `mode` is read separately from the sequence structs so patterns and the
/// demos keep their plain data shapes; unknown values are a parameter error.
fn parse_mode(arguments: &Value) -> Result<PlayMode, String> {
    match arguments.get("mode") {
        None | Some(Value::Null) => Ok(PlayMode::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| format!("Invalid mode {}: expected \"replace\" or \"layer\"", value)),
    }
}

fn playback_started_text(summary: String, duration: Duration, mode: PlayMode) -> String {
    let how = match mode {
        PlayMode::Replace => "replaced what was playing",
        PlayMode::Layer => "layered over the current playback",
    };
    format!(
        "🎵 Playback started ({}; {}). It will finish in about {:.1} seconds including effect tails; call stop_playback to cut it short.",
        summary,
        how,
        duration.as_secs_f64()
    )
}

fn start_playback(
    state: &mut ServerState,
    sequence: SimpleSequence,
    mode: PlayMode,
    id: Option<Value>,
    summary: String,
) -> JsonRpcResponse {
    let synths = state.synths.clone();
    let (player, external) = match state.player() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Audio output unavailable: {}", e);
            return JsonRpcResponse::tool_error(id, format!("Audio output unavailable: {}", e));
        }
    };
    match player.play_with(sequence, mode, &synths, Some(external)) {
        Ok(duration) => {
            JsonRpcResponse::tool_text(id, playback_started_text(summary, duration, mode))
        }
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
    let mode = match parse_mode(&arguments) {
        Ok(mode) => mode,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
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
    if let Err(e) = validate_tempo(sequence.tempo) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }
    if let Err(e) = validate_notes(&sequence.notes) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }

    let summary = format!(
        "{} notes: {}",
        sequence.notes.len(),
        describe_sources(&sequence.notes)
    );
    start_playback(state, sequence, mode, id, summary)
}

fn handle_define_pattern(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let mut pattern: SequencePattern = match serde_json::from_value(arguments) {
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
    if let Err(e) = validate_tempo(pattern.tempo) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }
    if let Err(e) = validate_notes(&pattern.notes) {
        return JsonRpcResponse::error(id, INVALID_PARAMS, e);
    }
    pattern.quantize_notes();

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

/// The shared front half of play_sequence and export_audio: parse the
/// notes/patterns arguments, validate, resolve patterns against the session
/// and describe the result. `Err` carries the response to send back.
// The whole response is the error payload here, not a hot path; boxing it
// would only push the allocation onto every call site.
#[allow(clippy::result_large_err)]
fn resolve_sequence_arguments(
    state: &ServerState,
    arguments: Value,
    id: &Option<Value>,
) -> Result<(SimpleSequence, String), JsonRpcResponse> {
    let extended: ExtendedSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            return Err(JsonRpcResponse::error(
                id.clone(),
                INVALID_PARAMS,
                format!("Failed to parse sequence: {}", e),
            ));
        }
    };
    if extended.notes.is_empty() && extended.patterns.is_empty() {
        return Err(JsonRpcResponse::error(
            id.clone(),
            INVALID_PARAMS,
            "Sequence must contain either notes or pattern references",
        ));
    }
    if let Err(e) = validate_tempo(extended.tempo) {
        return Err(JsonRpcResponse::error(id.clone(), INVALID_PARAMS, e));
    }
    if let Err(e) = validate_notes(&extended.notes) {
        return Err(JsonRpcResponse::error(id.clone(), INVALID_PARAMS, e));
    }

    let resolved = match extended.resolve_patterns(&state.patterns) {
        Ok(seq) => seq,
        Err(e) => {
            let known: Vec<&String> = state.patterns.keys().collect();
            return Err(JsonRpcResponse::tool_error(
                id.clone(),
                format!("{}. Defined patterns: {:?}", e, known),
            ));
        }
    };
    if resolved.notes.is_empty() {
        return Err(JsonRpcResponse::tool_error(
            id.clone(),
            "Resolved sequence contains no notes",
        ));
    }

    let summary = format!(
        "{} pattern references + {} individual notes → {} notes: {}",
        extended.patterns.len(),
        extended.notes.len(),
        resolved.notes.len(),
        describe_sources(&resolved.notes)
    );
    Ok((resolved, summary))
}

fn handle_play_sequence(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let mode = match parse_mode(&arguments) {
        Ok(mode) => mode,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
    let (resolved, summary) = match resolve_sequence_arguments(state, arguments, &id) {
        Ok(r) => r,
        Err(response) => return response,
    };
    start_playback(state, resolved, mode, id, summary)
}

/// Output options of export_audio. The sequence fields in the same object
/// are parsed separately by `resolve_sequence_arguments`.
#[derive(Debug, Deserialize)]
struct ExportOptions {
    path: String,
    #[serde(default = "default_export_name")]
    name: String,
    #[serde(default)]
    split: Split,
    #[serde(default = "default_bit_depth")]
    bit_depth: u32,
    #[serde(default)]
    overwrite: bool,
}

fn default_export_name() -> String {
    "mix".to_string()
}

fn default_bit_depth() -> u32 {
    24
}

fn handle_export_audio(
    state: &mut ServerState,
    arguments: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let options: ExportOptions = match serde_json::from_value(arguments.clone()) {
        Ok(o) => o,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!(
                    "Invalid export options: {} (split must be mixdown|stems|tracks, path is required)",
                    e
                ),
            );
        }
    };
    let dir = std::path::PathBuf::from(&options.path);
    if !dir.is_absolute() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            format!("path must be an absolute directory, got {:?}", options.path),
        );
    }
    if sanitize_name(&options.name).is_empty() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            "name must contain at least one letter, digit, '_' or '-'",
        );
    }
    let bit_depth = match BitDepth::from_bits(options.bit_depth) {
        Ok(b) => b,
        Err(e) => return JsonRpcResponse::error(id, INVALID_PARAMS, e),
    };
    let (sequence, summary) = match resolve_sequence_arguments(state, arguments, &id) {
        Ok(r) => r,
        Err(response) => return response,
    };

    let bus_warning = options.split == Split::Stems && bus_chain_is_nonlinear(&sequence);
    let request = ExportRequest {
        sequence,
        dir,
        name: options.name,
        split: options.split,
        bit_depth,
        overwrite: options.overwrite,
    };
    match export(request, &state.synths) {
        Ok(report) => {
            tracing::info!("Export finished ({}): {}", options.split.as_str(), summary);
            JsonRpcResponse::tool_text(
                id,
                export_finished_text(&report, options.split, bit_depth, bus_warning),
            )
        }
        Err(e) => {
            tracing::error!("Export failed: {}", e);
            JsonRpcResponse::tool_error(id, format!("Export failed: {}", e))
        }
    }
}

fn export_finished_text(
    report: &ExportReport,
    split: Split,
    bit_depth: BitDepth,
    bus_warning: bool,
) -> String {
    let what = match split {
        Split::Mixdown => "stereo mixdown",
        Split::Stems => "stems",
        Split::Tracks => "dry tracks",
    };
    let dir = report
        .files
        .first()
        .and_then(|f| f.path.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut out = format!("💾 Exported {} {} to {}:\n", report.files.len(), what, dir);
    for file in &report.files {
        out.push_str(&format!("  {}\n", file.path.display()));
    }
    out.push_str(&format!(
        "Duration {:.1} s (including effect tails), rendered in {:.1} s.",
        report.duration.as_secs_f64(),
        report.render_time.as_secs_f64()
    ));
    if bus_warning {
        out.push_str(
            "\n⚠️ MIDI channel stems each ran their own copy of the bus chain (compressor, distortion or Time Fracture delay), so they will not sum back to the mixdown exactly.",
        );
    }
    if bit_depth != BitDepth::Float32 {
        let hot: Vec<String> = report
            .files
            .iter()
            .filter(|f| f.peak > 1.0)
            .map(|f| {
                f.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.path.display().to_string())
            })
            .collect();
        if !hot.is_empty() {
            out.push_str(&format!(
                "\n⚠️ Clamped at 0 dBFS: {}. Pass \"bit_depth\": 32 to keep the full range.",
                hot.join(", ")
            ));
        }
    }
    out
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

const R2D2_EMOTIONS: [(&str, &str); 9] = [
    ("Happy", "cheerful bouncy warble"),
    ("Sad", "slow descending whine"),
    ("Excited", "rapid high staccato bursts"),
    ("Worried", "nervous trembling"),
    ("Curious", "rising question"),
    ("Affirmative", "steady confident confirmation"),
    ("Negative", "sharp low rejection"),
    ("Surprised", "sudden upward sweep"),
    ("Thoughtful", "deep slow pondering"),
];

fn handle_list_sounds(state: &ServerState, arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    let section = arguments
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let want = |name: &str| section == "all" || section == name;
    let mut out = String::new();

    if want("synths") {
        let library = PatchLibrary::new();
        out.push_str(&format!(
            "# Synth patches ({} built-in) — use \"synth\": \"<name>\" on a note, or define_synth for your own\n",
            library.count()
        ));
        for (category, patches) in library.catalog() {
            out.push_str(&format!("\n## {} ({})\n", category.as_str(), patches.len()));
            for patch in patches {
                out.push_str(&format!("- {} — {}\n", patch.name, patch.description));
            }
        }
        if !state.synths.is_empty() {
            let mut mine: Vec<&Patch> = state.synths.values().collect();
            mine.sort_by(|a, b| a.name.cmp(&b.name));
            out.push_str(&format!("\n## defined this session ({})\n", mine.len()));
            for patch in mine {
                out.push_str(&format!("- {} — {}\n", patch.name, patch.description));
            }
        }
        let algorithms = FmAlgorithm::ALL
            .iter()
            .map(FmAlgorithm::as_str)
            .collect::<Vec<_>>()
            .join("/");
        let tables = TableName::ALL
            .iter()
            .map(TableName::as_str)
            .collect::<Vec<_>>()
            .join("/");
        let percussion_kinds = [
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
        .map(PercussionKind::as_str)
        .collect::<Vec<_>>()
        .join("/");
        let sources = GrainSource::ALL
            .iter()
            .map(GrainSource::as_str)
            .collect::<Vec<_>>()
            .join("/");
        let lfo_targets = LfoTarget::ALL
            .iter()
            .map(LfoTarget::as_str)
            .collect::<Vec<_>>()
            .join("/");
        let lfo_waves = LfoWave::ALL
            .iter()
            .map(LfoWave::as_str)
            .collect::<Vec<_>>()
            .join("/");
        out.push_str(&format!(
            "\nEngines for inline patches: subtractive, fm (algorithms {algorithms}), wavetable (tables {tables}), granular (sources {sources}), percussion (kinds {percussion_kinds}).\nOptional lfo targets: {lfo_targets} (waves {lfo_waves}).\n\n"
        ));
    }

    if want("instruments") {
        use crate::midi::gm_names::{GM_FAMILIES, GM_INSTRUMENTS};
        out.push_str(
            "# General MIDI instruments — use instrument: <number> on channels 0-8 and 10-15\n",
        );
        for (family_index, family) in GM_FAMILIES.iter().enumerate() {
            out.push_str(&format!("\n## {}\n", family));
            for offset in 0..8 {
                let program = family_index * 8 + offset;
                out.push_str(&format!("- {}: {}\n", program, GM_INSTRUMENTS[program]));
            }
        }
        out.push('\n');
    }

    if want("drums") {
        use crate::midi::gm_names::GM_DRUM_KEYS;
        out.push_str("# GM drum kit — use channel: 9 and note: <key>\n");
        for (key, name) in GM_DRUM_KEYS {
            out.push_str(&format!("- {}: {}\n", key, name));
        }
        out.push_str("\nSynthesized drums are the tr_808_kick, tr_909_snare, tr_909_hihat, tr_808_hihat and crash_cymbal patches, or a percussion patch of your own.\n\n");
    }

    if want("r2d2") {
        out.push_str("# R2D2 emotions — note_type: \"r2d2\" with r2d2_emotion, r2d2_intensity (0-1), r2d2_complexity (1-5)\n");
        for (name, description) in R2D2_EMOTIONS {
            out.push_str(&format!("- {}: {}\n", name, description));
        }
        out.push('\n');
    }

    if want("effects") {
        let library = crate::expressive::EffectsPresetLibrary::new();
        let mut names: Vec<&String> = library.get_preset_names();
        names.sort();
        out.push_str("# Effects\n\n## Effect types for the `effects` chain\n");
        out.push_str(&format!(
            "- reverb: room_size, dampening, wet_level, pre_delay\n- delay: delay_time, feedback, wet_level, sync_tempo; Time Fracture: random_beats [min, max], random_rate, pitch_intervals, pitch_mode ({})\n",
            pitch_modes().join("/")
        ));
        out.push_str("- chorus: rate, depth, feedback\n- filter: filter_type (LowPass/HighPass/BandPass/Notch/Peak/LowShelf/HighShelf), cutoff, resonance\n- compressor: threshold (dB), ratio, attack, release\n- distortion: drive, tone, output_level\n");
        out.push_str(&format!("\n## effects_preset names ({})\n", names.len()));
        for name in names {
            out.push_str(&format!("- {}\n", name));
        }
        out.push_str("\nMIDI notes also accept reverb and chorus depths 0-127 (SoundFont built-in effects).\n");
    }

    if want("midi_outputs") {
        out.push_str(&midi_outputs_section(&state.external.outputs()));
    }

    if out.is_empty() {
        return JsonRpcResponse::error(
            id,
            INVALID_PARAMS,
            format!("Unknown section '{}'", section),
        );
    }
    JsonRpcResponse::tool_text(id, out.trim_end().to_string())
}

/// The `midi_outputs` catalog section: the virtual port, every destination
/// on the machine, and the one-time DAW setup.
fn midi_outputs_section(outputs: &Outputs) -> String {
    let mut out = String::from(
        "# MIDI outputs — use \"midi_out\": \"<name>\" on a note to play an instrument on this machine (a DAW such as Bitwig, a software or hardware synth)\n",
    );
    match &outputs.virtual_port {
        Ok(()) => out.push_str(&format!(
            "- {} — this server's virtual port, available while the server runs; select it as a MIDI input in your DAW\n",
            VIRTUAL_PORT_NAME
        )),
        Err(reason) => out.push_str(&format!(
            "- (no virtual port: {}; send to one of the destinations below)\n",
            reason
        )),
    }
    if outputs.destinations.is_empty() {
        out.push_str("- no other MIDI destinations found on this machine right now\n");
    }
    for name in &outputs.destinations {
        out.push_str(&format!("- {}\n", name));
    }
    out.push_str(&format!(
        "\nBitwig: Settings > Controllers > Add Controller > Generic > \"MIDI Keyboard\", MIDI input {port}; notes then play on the selected/armed instrument track. To address several tracks, set each track's input chooser to {port} and one channel (channel 0 here is channel 1 in Bitwig), and arm them all. Other DAWs: enable {port} as a MIDI input and route it to a track.\nNotes sent out keep their channel, send a program change only when instrument is given, and cannot carry effects or effects_preset; export_audio cannot record them.\n",
        port = VIRTUAL_PORT_NAME
    ));
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::export::ExportedFile;
    use serde_json::json;

    fn call(state: &mut ServerState, tool: &str, args: Value) -> JsonRpcResponse {
        dispatch_tool(
            state,
            ToolCallParams {
                name: tool.to_string(),
                arguments: args,
            },
            Some(json!(1)),
        )
    }

    fn text(r: &JsonRpcResponse) -> String {
        r.result.as_ref().unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn define_synth_stores_a_validated_patch_and_echoes_usage() {
        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "Blip", "subtractive": {"osc1": {"wave": "square"}}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        let t = text(&r);
        assert!(
            t.contains("Blip") && t.contains("\"synth\": \"Blip\""),
            "{t}"
        );
        assert!(state.synths.contains_key("blip"));
    }

    #[test]
    fn define_synth_rejects_unknown_fields_and_out_of_range_values_as_invalid_params() {
        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "x", "subtractive": {"cutoff": 1}}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("cutoff"));
        assert!(state.synths.is_empty());

        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "x", "subtractive": {"filter": {"cutoff": 1}}}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(
            r.error
                .as_ref()
                .unwrap()
                .message
                .contains("subtractive.filter.cutoff")
        );
        assert!(state.synths.is_empty());
    }

    #[test]
    fn effects_schema_describes_time_fracture_and_it_validates_through_define_synth() {
        let props = &effects_schema()["items"]["properties"];
        assert_eq!(props["random_beats"]["minItems"], 2);
        assert_eq!(
            props["pitch_mode"]["enum"],
            json!(["random", "up", "down", "up_down"])
        );
        // Both the schema enum and the catalog come from `PitchMode::ALL`, so
        // adding a mode cannot leave either behind.
        assert_eq!(props["pitch_mode"]["enum"], json!(pitch_modes()));
        let catalog = text(&handle_list_sounds(
            &ServerState::new(),
            json!({"section": "effects"}),
            Some(json!(1)),
        ));
        assert!(
            catalog.contains(&pitch_modes().join("/")),
            "the effects catalog lists the pitch modes: {catalog}"
        );
        assert!(
            props["delay_time"]["description"]
                .as_str()
                .unwrap()
                .contains("8 s"),
            "delay_time documents the cap"
        );
        assert!(
            effects_schema()["description"]
                .as_str()
                .unwrap()
                .contains("-32602"),
            "the flat property list says other keys are rejected"
        );
        assert_eq!(props["pitch_intervals"]["maxItems"], 12);
        assert!(props["random_rate"].is_object());
        assert!(
            !props["sync_tempo"]["description"]
                .as_str()
                .unwrap()
                .contains("reserved")
        );

        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "shimmer", "subtractive": {},
                "effects": [{"type": "delay", "random_beats": [0.5, 1.0], "random_rate": 0.5,
                             "pitch_intervals": [7, 12], "pitch_mode": "up_down", "feedback": 0.5, "intensity": 0.6}]}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "bad", "subtractive": {},
                "effects": [{"type": "delay", "random_beats": [0.5, 9.0]}]}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("random_beats"));
    }

    #[test]
    fn a_tempo_outside_the_supported_range_is_a_parameter_error() {
        let notes = json!([{"note": 60, "duration": 0.1}]);
        let mut state = ServerState::new();
        for tempo in [10, 400] {
            for (tool, args) in [
                ("play_notes", json!({"notes": notes, "tempo": tempo})),
                ("play_sequence", json!({"notes": notes, "tempo": tempo})),
                (
                    "define_sequence_pattern",
                    json!({"name": "p", "notes": notes, "tempo": tempo}),
                ),
            ] {
                let r = call(&mut state, tool, args);
                let e = r
                    .error
                    .as_ref()
                    .unwrap_or_else(|| panic!("{tool} accepted tempo {tempo}"));
                assert_eq!(e.code, INVALID_PARAMS, "{tool}: {}", e.message);
                assert!(
                    e.message.contains("tempo")
                        && e.message.contains("20")
                        && e.message.contains("300"),
                    "{tool}: {}",
                    e.message
                );
            }
        }
        // The bounds themselves are accepted. `define_sequence_pattern` is the
        // one tool that validates the tempo without opening an audio device.
        for tempo in [20, 300] {
            let r = call(
                &mut state,
                "define_sequence_pattern",
                json!({"name": "ok", "notes": notes, "tempo": tempo}),
            );
            assert!(r.error.is_none(), "tempo {tempo}: {:?}", r.error);
        }
        // Every schema that advertises a tempo advertises the same range.
        let tools = handle_tools_list(Some(json!(1))).result.unwrap()["tools"].clone();
        let mut seen = 0;
        for tool in tools.as_array().unwrap() {
            if let Some(schema) = tool["inputSchema"]["properties"]["tempo"].as_object() {
                assert_eq!(schema["minimum"], json!(20), "{}", tool["name"]);
                assert_eq!(schema["maximum"], json!(300), "{}", tool["name"]);
                seen += 1;
            }
        }
        assert_eq!(
            seen, 4,
            "play_notes, play_sequence, export_audio and the pattern tool"
        );
    }

    #[test]
    fn the_midi_outputs_section_lists_the_virtual_port_and_destinations_or_says_why_not() {
        let with = midi_outputs_section(&Outputs {
            virtual_port: Ok(()),
            destinations: vec!["IAC Driver Bus 1".into()],
        });
        assert!(with.contains("- mcp-muse —"), "{with}");
        assert!(with.contains("- IAC Driver Bus 1\n"));
        assert!(with.contains("Bitwig"));
        let without = midi_outputs_section(&Outputs {
            virtual_port: Err("loopMIDI".into()),
            destinations: vec![],
        });
        assert!(without.contains("no virtual port: loopMIDI"), "{without}");
        assert!(without.contains("no other MIDI destinations"));

        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "list_sounds",
            json!({"section": "midi_outputs"}),
        );
        assert!(text(&r).starts_with("# MIDI outputs"));
        let all = call(&mut state, "list_sounds", json!({}));
        assert!(text(&all).contains("# MIDI outputs"));
    }

    #[test]
    fn a_midi_out_note_with_effects_is_a_parameter_error_and_the_summary_names_the_port() {
        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "play_notes",
            json!({"notes": [{"note": 60, "midi_out": "mcp-muse", "effects_preset": "studio"}]}),
        );
        let err = r.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(
            err.message.contains("midi_out") && err.message.contains("note 1"),
            "{}",
            err.message
        );

        let notes: Vec<SimpleNote> = serde_json::from_value(json!([
            {"note": 60, "midi_out": "mcp-muse"},
            {"note": 62, "midi_out": " mcp-muse"},
            {"note": 64, "midi_out": "IAC Driver Bus 1"},
            {"note": 36, "channel": 9}
        ]))
        .unwrap();
        assert_eq!(
            describe_sources(&notes),
            "MIDI instruments + external MIDI (IAC Driver Bus 1, mcp-muse)"
        );
    }

    #[test]
    fn export_audio_refuses_a_midi_out_note_instead_of_dropping_it() {
        let mut state = ServerState::new();
        let dir = export_dir("midi-out");
        let r = call(
            &mut state,
            "export_audio",
            json!({"notes": [{"note": 60, "midi_out": "mcp-muse", "duration": 0.1}],
                   "path": dir.to_string_lossy()}),
        );
        let body = text(&r);
        assert_eq!(r.result.as_ref().unwrap()["isError"], json!(true), "{body}");
        assert!(
            body.contains("Note 1") && body.contains("midi_out"),
            "{body}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_sounds_synths_section_names_builtins_and_session_patches() {
        let mut state = ServerState::new();
        call(
            &mut state,
            "define_synth",
            json!({"name": "mine", "percussion": {"kind": "snare"}}),
        );
        let r = handle_list_sounds(&state, json!({"section": "synths"}), Some(json!(1)));
        let t = text(&r);
        assert!(
            t.contains("minimoog_bass") && t.contains("tr_808_kick") && t.contains("mine"),
            "{t}"
        );
        assert!(!t.contains("Acoustic Grand Piano"));
    }

    #[test]
    fn the_synth_r2d2_conflict_is_reported_before_a_missing_r2d2_field() {
        let note: SimpleNote = serde_json::from_value(json!({
            "synth": "sub_bass", "note_type": "r2d2", "start_time": 0.0, "duration": 0.2
        }))
        .unwrap();
        let err = validate_notes(&[note]).unwrap_err();
        assert!(
            err.contains("both note_type \"r2d2\" and synth"),
            "the synth/R2D2 conflict should win over the missing emotion: {err}"
        );
    }

    #[test]
    fn tools_list_has_eight_tools_and_the_note_schema_has_synth() {
        let r = handle_tools_list(Some(json!(1)));
        let tools = r.result.unwrap()["tools"].clone();
        assert_eq!(tools.as_array().unwrap().len(), 8);
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"define_synth"));
        let schema = note_schema();
        assert!(schema["properties"]["synth"].is_object());
        assert!(
            schema["properties"]["midi_out"]["description"]
                .as_str()
                .unwrap()
                .contains("mcp-muse")
        );
        assert!(schema["properties"].get("synth_type").is_none());
        assert!(schema["properties"].get("preset_name").is_none());
        assert!(names.contains(&"export_audio"));
        let export = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "export_audio")
            .unwrap();
        let props = &export["inputSchema"]["properties"];
        assert!(props["path"].is_object());
        assert_eq!(
            props["split"]["enum"],
            json!(["mixdown", "stems", "tracks"])
        );
        assert_eq!(props["bit_depth"]["enum"], json!([16, 24, 32]));
        assert!(props["patterns"]["items"]["properties"]["pattern_name"].is_object());
        assert!(props.get("mode").is_none(), "an export has no play mode");
        assert_eq!(export["inputSchema"]["required"], json!(["path"]));
    }

    fn export_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcp-muse-mcp-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn blip() -> Value {
        json!({"synth": {"name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}},
               "note": 84, "duration": 0.1})
    }

    #[test]
    fn export_audio_rejects_bad_options_as_invalid_params() {
        let mut state = ServerState::new();
        let dir = export_dir("bad").to_string_lossy().into_owned();
        for (args, needle) in [
            (
                json!({"notes": [blip()], "path": "relative/dir"}),
                "absolute",
            ),
            (
                json!({"notes": [blip()], "path": dir, "bit_depth": 20}),
                "bit_depth",
            ),
            (
                json!({"notes": [blip()], "path": dir, "split": "loud"}),
                "split",
            ),
            (
                json!({"notes": [blip()], "path": dir, "name": "  "}),
                "name",
            ),
            (json!({"notes": [blip()]}), "path"),
        ] {
            let r = call(&mut state, "export_audio", args);
            let err = r
                .error
                .as_ref()
                .unwrap_or_else(|| panic!("expected -32602 for {}", needle));
            assert_eq!(err.code, INVALID_PARAMS, "{}", needle);
            assert!(
                err.message.contains(needle),
                "{} not in {}",
                needle,
                err.message
            );
        }
    }

    #[test]
    fn export_audio_writes_a_mixdown_and_reports_the_path() {
        let mut state = ServerState::new();
        let dir = export_dir("mix");
        let r = call(
            &mut state,
            "export_audio",
            json!({"notes": [blip()], "path": dir.to_string_lossy(), "name": "Demo Take"}),
        );
        let t = text(&r);
        assert!(r.result.as_ref().unwrap().get("isError").is_none(), "{}", t);
        let expected = dir.join("Demo_Take.wav");
        assert!(t.contains(&expected.display().to_string()), "{}", t);
        assert!(t.contains("mixdown"), "{}", t);
        assert!(expected.exists());
    }

    #[test]
    fn export_audio_reports_runtime_failures_as_tool_errors() {
        let mut state = ServerState::new();
        let dir = export_dir("toolerr");
        let r = call(
            &mut state,
            "export_audio",
            json!({"patterns": [{"pattern_name": "nope"}], "path": dir.to_string_lossy()}),
        );
        assert_eq!(r.result.as_ref().unwrap()["isError"], json!(true));
        assert!(text(&r).contains("nope"));

        let r = call(
            &mut state,
            "export_audio",
            json!({"notes": [{"synth": "no_such_patch", "note": 60}], "path": dir.to_string_lossy()}),
        );
        assert_eq!(r.result.as_ref().unwrap()["isError"], json!(true));
        assert!(text(&r).contains("no_such_patch"));
    }

    #[test]
    fn export_audio_names_hot_files_when_they_are_clamped() {
        let report = ExportReport {
            files: vec![
                ExportedFile {
                    path: "/tmp/x/a.wav".into(),
                    peak: 1.3,
                },
                ExportedFile {
                    path: "/tmp/x/b.wav".into(),
                    peak: 0.5,
                },
            ],
            duration: Duration::from_secs_f64(3.3),
            render_time: Duration::from_millis(400),
        };
        let t = export_finished_text(&report, Split::Stems, BitDepth::Int24, false);
        assert!(t.contains("2 stems"), "{}", t);
        assert!(t.contains("2 stems to /tmp/x:"), "{}", t);
        assert!(t.contains("/tmp/x/a.wav"), "{}", t);
        assert!(t.contains("Duration 3.3 s"), "{}", t);
        let clamped = t.lines().last().unwrap();
        assert!(
            clamped.contains("Clamped") && clamped.contains("a.wav"),
            "{}",
            t
        );
        assert!(clamped.contains("bit_depth"), "{}", t);
        assert!(
            !clamped.contains("b.wav"),
            "quiet files are not listed as clamped: {}",
            t
        );
        let f = export_finished_text(&report, Split::Stems, BitDepth::Float32, false);
        assert!(!f.contains("Clamped"), "float never clamps: {}", f);
    }

    #[test]
    fn export_audio_warns_when_the_bus_chain_is_nonlinear() {
        let report = ExportReport {
            files: vec![ExportedFile {
                path: "/tmp/x/ch00_flute.wav".into(),
                peak: 0.5,
            }],
            duration: Duration::from_secs_f64(1.0),
            render_time: Duration::from_millis(50),
        };
        let quiet = export_finished_text(&report, Split::Stems, BitDepth::Int24, false);
        assert!(!quiet.contains("⚠️ MIDI channel stems"), "{}", quiet);
        let warned = export_finished_text(&report, Split::Stems, BitDepth::Int24, true);
        assert!(
            warned.contains(
                "⚠️ MIDI channel stems each ran their own copy of the bus chain (compressor, distortion or Time Fracture delay), so they will not sum back to the mixdown exactly."
            ),
            "{}",
            warned
        );
    }

    #[test]
    fn patch_schema_describes_fm_and_wavetable_and_they_validate_through_define_synth() {
        let schema = patch_schema();
        let fm = &schema["properties"]["fm"]["properties"];
        assert_eq!(
            fm["algorithm"]["enum"],
            json!(["stack", "pairs", "fan_in", "parallel"])
        );
        assert_eq!(fm["operators"]["maxItems"], 4);
        assert!(fm["operators"]["items"]["properties"]["ratio"].is_object());
        let wt = &schema["properties"]["wavetable"]["properties"];
        assert_eq!(
            wt["table"]["enum"],
            json!([
                "basic", "warm", "bright", "digital", "vocal", "pwm", "organ", "noise"
            ])
        );
        assert!(wt["morph"].is_object());

        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "bell", "fm": {"algorithm": "stack",
                "operators": [{"ratio": 1}, {"ratio": 3.5, "level": 0.5}]}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(text(&r).contains("fm"), "{}", text(&r));
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "org", "wavetable": {"table": "organ"}}),
        );
        assert!(r.error.is_none());
        assert!(text(&r).contains("wavetable"));
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "bad", "fm": {"operators": [{"ratio": 99}]}}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(
            r.error
                .as_ref()
                .unwrap()
                .message
                .contains("fm.operators[0].ratio")
        );
    }

    #[test]
    fn patch_schema_describes_lfo_and_granular_and_they_validate_through_define_synth() {
        let schema = patch_schema();
        let lfo = &schema["properties"]["lfo"]["properties"];
        assert_eq!(
            lfo["wave"]["enum"],
            json!(["sine", "triangle", "saw", "square", "sample_hold"])
        );
        assert_eq!(
            lfo["target"]["enum"],
            json!([
                "off",
                "cutoff",
                "pitch",
                "amplitude",
                "morph",
                "grain_density"
            ])
        );
        let g = &schema["properties"]["granular"]["properties"];
        assert_eq!(
            g["source"]["enum"],
            json!(["harmonics", "noise", "formant", "inharmonic"])
        );
        for field in [
            "grain_ms",
            "density",
            "pitch_semitones",
            "randomness",
            "stereo_width",
            "env",
            "level",
        ] {
            assert!(g[field].is_object(), "{field}");
        }

        let mut state = ServerState::new();
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "cloud",
                "granular": {"source": "formant", "stereo_width": 0.9},
                "lfo": {"rate": 0.2, "depth": 0.4, "target": "grain_density"}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(
            text(&r).contains("granular") && text(&r).contains("lfo"),
            "{}",
            text(&r)
        );
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "bad", "granular": {"density": 500}}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(
            r.error
                .as_ref()
                .unwrap()
                .message
                .contains("granular.density")
        );
        let r = call(
            &mut state,
            "define_synth",
            json!({"name": "bad2", "subtractive": {}, "lfo": {"rate": 99}}),
        );
        assert_eq!(r.error.as_ref().unwrap().code, INVALID_PARAMS);
        assert!(r.error.as_ref().unwrap().message.contains("lfo.rate"));
    }
}
