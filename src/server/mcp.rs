use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::midi::{ExtendedSequence, MidiPlayer, SequencePattern, SimpleSequence};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

// Global pattern storage for the MCP server session
lazy_static::lazy_static! {
    static ref PATTERN_STORE: Arc<Mutex<HashMap<String, SequencePattern>>> = Arc::new(Mutex::new(HashMap::new()));
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
    #[serde(serialize_with = "serialize_id")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

// Custom serializer for id field to ensure it's never null
fn serialize_id<S>(id: &Option<Value>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    match id {
        Some(val) => val.serialize(serializer),
        None => "unknown".serialize(serializer), // Use default string instead of null
    }
}

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
        "version": "0.1.0"
    });

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": server_capabilities,
            "serverInfo": server_info
        })),
        error: None,
    }
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling tools/list request");

    let tools = json!([
        {
            "name": "define_sequence_pattern",
            "description": "🎼 DEFINE REUSABLE SEQUENCE PATTERNS: Create named musical patterns that can be reused with transformations!

Define drum beats, bass lines, chord progressions, and melodic phrases once, then reuse them throughout your composition with different instruments, transpositions, and timing. Perfect for:

• 🥁 DRUM PATTERNS: Create classic beat patterns that repeat throughout a song
• 🎸 BASS LINES: Define groovy bass patterns that can be transposed to different keys  
• 🎹 CHORD PROGRESSIONS: Set up harmonic sequences that can be used with different instruments
• 🎵 MELODIC MOTIFS: Create memorable musical phrases that can be varied and developed

**Pattern Features:**
• **Transposition**: Shift patterns up/down by semitones for different keys
• **Instrument Override**: Play the same pattern with different instruments
• **Velocity/Duration Scaling**: Adjust intensity and timing dynamically
• **Repetition**: Repeat patterns multiple times with spacing control
• **Channel Routing**: Route patterns to specific MIDI channels

**Usage Examples:**
• House Beat: Define a kick-snare-hat pattern, then use it throughout your track
• Chord Progression: Define a I-V-vi-IV progression, transpose it to different keys
• Bass Groove: Create a funky bass line, then transpose it for chorus sections
• Melody Hook: Define a catchy melody, then play it with different instruments

This tool stores patterns in memory for the current session and makes composition much more efficient!",
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
            "description": "🎼🎵 ENHANCED SEQUENCE PLAYER: Play music using both individual notes AND reusable sequence patterns with PERFECT MUSICAL TIMING!

This enhanced tool combines the power of individual note specification with the efficiency of pattern references, plus a revolutionary bar-based timing system that ensures everything stays perfectly in sync!

**🎯 MUSICAL TIMING REVOLUTION:**
• **Bar-Based Positioning**: Use start_bar instead of seconds for perfect alignment
• **Smart Bar Arrays**: Specify exact bars where patterns should play [1, 5, 9, 13]
• **Auto-Alignment**: Patterns automatically snap to bar boundaries 
• **No More Drift**: Drum patterns stay locked to basslines and chord progressions
• **Professional Results**: Create music that sounds like it was made in a studio

**🎭 PATTERN FEATURES:**
• **Pattern References**: Reference previously defined patterns by name
• **Real-time Transformations**: Transpose, change instruments, scale velocity/duration
• **Intelligent Repetition**: Repeat patterns with musical spacing (in bars, not seconds!)
• **Channel Routing**: Route patterns to specific MIDI channels
• **Mixed Composition**: Combine individual notes with pattern references seamlessly

**🎸 TRANSFORMATION OPTIONS:**
• **Transpose** (-12 to +12 semitones): Shift patterns to different keys
• **Instrument Override**: Use the same pattern with different instruments
• **Velocity Scale** (0.1-2.0): Make patterns softer or more intense
• **Duration Scale** (0.1-4.0): Make patterns faster/slower, staccato/legato
• **Channel Override**: Route to specific MIDI channels
• **Bar-Based Repetition**: Repeat patterns every N bars with perfect timing
• **Beat Offset**: Start patterns on specific beats within bars

**✨ TIMING IMPROVEMENTS:**
• **No More Pattern Drift**: Bar-based timing keeps everything synchronized
• **Professional Arrangements**: Easily create verse-chorus structures
• **Perfect Loops**: 4-bar patterns stay perfectly aligned
• **Dynamic Arrangements**: Play patterns on specific bars for complex compositions

**🎵 COMPOSITION EXAMPLES:**

**🎼 Perfect Verse-Chorus Structure (Bar-Based):**
```json
{
  \"patterns\": [
    {\"pattern_name\": \"drum_beat\", \"start_bar\": 1, \"repeat_count\": 16, \"align_to_bars\": true},
    {\"pattern_name\": \"bass_line\", \"start_bar\": 1, \"repeat_count\": 8},
    {\"pattern_name\": \"chord_prog\", \"start_bar\": 1, \"instrument_override\": 1},
    {\"pattern_name\": \"chord_prog\", \"start_bar\": 9, \"transpose\": 2, \"instrument_override\": 73}
  ]
}
```

**🎹 Smart Bar Placement:**
```json
{
  \"patterns\": [
    {\"pattern_name\": \"drums\", \"bars\": [1, 3, 5, 7, 9, 11, 13, 15]},
    {\"pattern_name\": \"bass\", \"bars\": [2, 4, 6, 8, 10, 12, 14, 16]},
    {\"pattern_name\": \"chords\", \"bars\": [5, 13], \"velocity_scale\": 0.8}
  ]
}
```

**🎸 Dynamic Musical Variations:**
```json
{
  \"patterns\": [
    {\"pattern_name\": \"melody\", \"start_bar\": 1, \"velocity_scale\": 0.7},
    {\"pattern_name\": \"melody\", \"start_bar\": 5, \"transpose\": 5, \"velocity_scale\": 1.2},
    {\"pattern_name\": \"melody\", \"start_bar\": 9, \"transpose\": -3, \"duration_scale\": 0.5}
  ]
}
```

**🥁 Professional Drum Arrangement:**
```json
{
  \"patterns\": [
    {\"pattern_name\": \"kick_pattern\", \"start_bar\": 1, \"repeat_count\": 16},
    {\"pattern_name\": \"snare_pattern\", \"start_bar\": 2, \"repeat_count\": 8, \"repeat_spacing_bars\": 1},
    {\"pattern_name\": \"hihat_pattern\", \"bars\": [1, 2, 3, 4, 9, 10, 11, 12]}
  ]
}
```

**🎯 WHEN TO USE WHICH TOOL:**
• **play_sequence** (THIS TOOL): Full songs, complex compositions, repeating patterns, professional arrangements
• **define_sequence_pattern**: Create reusable patterns (drum beats, bass lines, chord progressions, melodies)  
• **play_notes**: Quick sounds, single instruments, R2D2 expressions, sound effects, simple melodies
• **list_patterns**: Browse available patterns for reference

This tool makes composition much more efficient by allowing you to define musical elements once and reuse them with variations throughout your piece!",
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
            "description": "📋 LIST SEQUENCE PATTERNS: View all defined sequence patterns in the current session.

Shows all available patterns with their names, descriptions, categories, and basic info. Perfect for:
• 🔍 **Discovery**: See what patterns are available for use
• 📊 **Organization**: Review patterns by category (drums, bass, melody, etc.)
• 🏷️ **Reference**: Get pattern names for use in play_sequence tool
• 📝 **Documentation**: See pattern descriptions and metadata

Returns a formatted list of all stored patterns with their key information.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "play_notes",
            "description": "🎮🤖🎛️ UNIVERSAL AUDIO ENGINE: The ultimate all-in-one tool for MIDI music, R2D2 expressions, and custom synthesis!

🎵 MIDI MUSIC: 128 GM instruments, authentic SNES gaming sounds, professional effects chain
🤖 R2D2 EXPRESSIONS: 9 emotions, ring modulation synthesis, authentic robotic vocalizations  
🎛️ CUSTOM SYNTHESIS: 19 synthesis types, professional drum sounds, 6-effect audio processing

⚡ **FOR COMPLEX COMPOSITIONS**: Use the **define_sequence_pattern** and **play_sequence** tools instead! They offer:
• 🔄 Reusable patterns (drum beats, bass lines, chord progressions)
• 🎼 Musical timing with bar-based alignment (no more timing drift!)
• 🎭 Pattern transformations (transpose, instrument changes, repetition)
• 🎯 Professional composition workflow for full songs and complex pieces

💡 **THIS TOOL IS PERFECT FOR**: Quick sounds, single instruments, simple melodies, R2D2 expressions, and sound effects.

💡 QUICK EXAMPLES:
• Victory Fanfare: [{\"note\": 60, \"instrument\": 56, \"velocity\": 120, \"duration\": 1.0}]
• Cathedral Piano: [{\"note\": 60, \"instrument\": 1, \"duration\": 3.0, \"effects_preset\": \"concert_hall\"}]
• Vintage Warmth: [{\"note\": 60, \"instrument\": 0, \"duration\": 2.0, \"effects_preset\": \"vintage\"}]
• R2D2 Celebration: [{\"note_type\": \"r2d2\", \"r2d2_emotion\": \"Excited\", \"r2d2_intensity\": 0.9, \"duration\": 1.5}]
• Custom Effects: [{\"note\": 60, \"instrument\": 73, \"duration\": 2.0, \"effects\": [{\"effect\": {\"type\": \"Reverb\", \"room_size\": 0.8, \"wet_level\": 0.6}, \"intensity\": 0.8}]}]
• Mixed Sequence: Combine all types with signature preset effects for professional sound!

🎯 ONE TOOL, INFINITE AUDIO POSSIBILITIES - From retro gaming music to expressive AI vocalizations!

🎉 CELEBRATION SOUNDS:
• Victory fanfares (brass + drums on channels 0-2)
• Success chimes (bells, glockenspiel, bright piano)
• Achievement themes (major chords, uplifting melodies)
• Completion jingles (short, satisfying musical phrases)

🎮 INTERACTIVE FEEDBACK:
• Question marks (rising tones, curious melodies)
• 'Aha!' moments (bright ascending scales, revelation chords)
• Error sounds (gentle dissonance, falling tones)
• Thinking music (ambient pads, contemplative arpeggios)
• Alert tones (attention-grabbing but pleasant)

🎭 EMOTIONAL ATMOSPHERE:
• Suspense (low strings, minor chords, tension)
• Wonder (ethereal pads with reverb, magical sparkles)
• Humor (playful instruments like kazoo, quirky rhythms)
• Calm focus (soft piano, gentle ambient soundscapes)
• Epic moments (full orchestration, dramatic crescendos)

🕹️ RETRO & 8-BIT VIBES:
• Classic game sounds (square waves, chip-tune leads)
• Power-up effects (ascending arpeggios, bright synths)
• Coin collect sounds (short metallic pings)
• Level-up fanfares (triumphant 8-bit melodies)

💡 ENHANCE CONVERSATIONS WITH MUSIC - USE LIBERALLY!
• 🎉 Play celebration music IMMEDIATELY when users accomplish tasks or solve problems
• ❓ Add question mark sounds when presenting puzzles or mysteries
• ✅ Use gentle chimes for confirmations and successful completions
• 🎭 Create ambient soundscapes during long explanations to maintain engagement
• ⚡ Add dramatic flourishes for important revelations and \"aha!\" moments
• 😄 Use playful sounds to lighten serious moments and add humor
• 🔔 Play alert tones to draw attention to important information
• 🎵 Use background music to set the mood for different conversation phases

💫 DON'T HESITATE - ADD MUSIC TO EVERY SIGNIFICANT MOMENT! Think like a video game: constant audio feedback makes interactions more engaging and memorable.

🎹 TECHNICAL CAPABILITIES:
• 128 GM instruments: 0=Piano, 9=Glockenspiel, 40=Violin, 56=Trumpet, 73=Flute, 80=Square Lead, 120=Reverse Cymbal
• 16 independent channels for rich layering
• 🎛️ PROFESSIONAL EFFECTS CHAIN: 6 effect types with studio-quality algorithms
  - Reverb: Schroeder algorithm with comb filters + allpass diffusion
  - Delay: Feedback delay with analog character and high-frequency damping
  - Chorus: Multi-tap modulated delays with LFO for lush swirling
  - Filter: State variable filters (lowpass, highpass, bandpass, notch, peak, shelf)
  - Compressor: Smooth dynamics processing with attack/release
  - Distortion: Waveshaping with pre/post filtering for musical overdrive
• 🎭 14 EFFECTS PRESETS: studio, concert_hall, vintage, ambient, live_stage, tight_mix, dreamy, spacious, analog_warmth, retro_echo, psychedelic, distorted, filtered, lush_chorus
• 🎨 PRESET SIGNATURE EFFECTS: All classic synth presets include subtle, musical effects by default
• Stereo positioning: pan (mono instruments), balance (stereo instruments)
• Full drum kit on channel 9: 36=Kick, 38=Snare, 42=Hi-hat, 49=Crash

🏰 CLASSIC SNES GAME THEMES:

🗡️ ZELDA-STYLE DISCOVERY (Treasure Found):
[{\"note\": 67, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 72, \"velocity\": 100, \"start_time\": 0.3, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 76, \"velocity\": 110, \"start_time\": 0.6, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 79, \"velocity\": 120, \"start_time\": 0.9, \"duration\": 0.6, \"channel\": 0, \"instrument\": 73, \"reverb\": 40}]

🍄 MARIO-STYLE OVERWORLD (Happy Melody):
[{\"note\": 72, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 90, \"start_time\": 0.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 100, \"start_time\": 1, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 69, \"velocity\": 90, \"start_time\": 1.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 71, \"velocity\": 100, \"start_time\": 2, \"duration\": 0.5, \"channel\": 0, \"instrument\": 80}]

🌟 FINAL FANTASY-STYLE VICTORY:
[{\"note\": 60, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 64, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 67, \"velocity\": 110, \"start_time\": 1, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 72, \"velocity\": 120, \"start_time\": 1.5, \"duration\": 1, \"channel\": 0, \"instrument\": 56}, {\"note\": 48, \"velocity\": 80, \"start_time\": 0, \"duration\": 2.5, \"channel\": 1, \"instrument\": 32}, {\"note\": 36, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.25, \"channel\": 9}, {\"note\": 36, \"velocity\": 90, \"start_time\": 1, \"duration\": 0.25, \"channel\": 9}]

🏰 METROID-STYLE ATMOSPHERE (Mysterious Exploration):
[{\"note\": 36, \"velocity\": 60, \"start_time\": 0, \"duration\": 2, \"channel\": 0, \"instrument\": 89, \"reverb\": 80}, {\"note\": 43, \"velocity\": 50, \"start_time\": 1, \"duration\": 2, \"channel\": 1, \"instrument\": 89, \"reverb\": 80}, {\"note\": 48, \"velocity\": 40, \"start_time\": 2, \"duration\": 2, \"channel\": 2, \"instrument\": 89, \"reverb\": 80}]

🤖 **R2D2 EXPRESSIVE VOCALIZATIONS:**

**Victory Fanfare with R2D2 Celebration:**
[{\"note\": 60, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5, \"instrument\": 56}, {\"note\": 64, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.5, \"instrument\": 56}, {\"note_type\": \"r2d2\", \"start_time\": 1.2, \"duration\": 1.0, \"r2d2_emotion\": \"Excited\", \"r2d2_intensity\": 0.9, \"r2d2_complexity\": 4}, {\"note\": 72, \"velocity\": 120, \"start_time\": 1.5, \"duration\": 1.5, \"instrument\": 56}]

**Problem-Solving with Thoughtful R2D2:**
[{\"note_type\": \"r2d2\", \"start_time\": 0, \"duration\": 1.5, \"r2d2_emotion\": \"Thoughtful\", \"r2d2_intensity\": 0.5, \"r2d2_complexity\": 3}, {\"note\": 60, \"velocity\": 70, \"start_time\": 0.5, \"duration\": 1.0, \"instrument\": 0}, {\"note_type\": \"r2d2\", \"start_time\": 2.0, \"duration\": 0.6, \"r2d2_emotion\": \"Surprised\", \"r2d2_intensity\": 0.8, \"r2d2_complexity\": 1}]

**Curious Discovery:**
[{\"note\": 36, \"velocity\": 60, \"start_time\": 0, \"duration\": 3, \"instrument\": 89, \"reverb\": 80}, {\"note_type\": \"r2d2\", \"start_time\": 1.0, \"duration\": 0.8, \"r2d2_emotion\": \"Curious\", \"r2d2_intensity\": 0.6, \"r2d2_complexity\": 2}, {\"note\": 67, \"velocity\": 90, \"start_time\": 2.5, \"duration\": 0.3, \"instrument\": 73}]

🎛️ **CUSTOM SYNTHESIS EXAMPLES:**

**Sci-Fi Energy Zap:**
[{\"synth_type\": \"zap\", \"synth_frequency\": 800, \"start_time\": 0, \"duration\": 0.5, \"synth_amplitude\": 0.8}]

**Professional Kick Drum:**
[{\"synth_type\": \"kick\", \"synth_frequency\": 60, \"start_time\": 0, \"duration\": 0.8, \"synth_amplitude\": 0.9}]

**Ambient Pad with Effects:**
[{\"synth_type\": \"pad\", \"synth_frequency\": 220, \"start_time\": 0, \"duration\": 4.0, \"synth_reverb\": 0.7, \"synth_chorus\": 0.5}]

**FM Bell Synthesis:**
[{\"synth_type\": \"fm\", \"synth_frequency\": 440, \"synth_modulator_freq\": 880, \"synth_modulation_index\": 3.0, \"start_time\": 0, \"duration\": 2.0}]

🎹 **CLASSIC SYNTHESIZER PRESETS (NEW!):**

**80s Funk Bass Line (Minimoog Style):**
[{\"preset_name\": \"Minimoog Bass\", \"note\": 36, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5}, {\"preset_name\": \"Minimoog Bass\", \"note\": 36, \"velocity\": 80, \"start_time\": 0.5, \"duration\": 0.5}, {\"preset_name\": \"Minimoog Bass\", \"note\": 38, \"velocity\": 90, \"start_time\": 1.0, \"duration\": 0.5}]

**Acid House Bassline (TB-303 Style):**
[{\"preset_name\": \"TB-303 Acid\", \"note\": 36, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.25}, {\"preset_name\": \"TB-303 Acid\", \"preset_variation\": \"squelchy\", \"note\": 43, \"velocity\": 120, \"start_time\": 0.25, \"duration\": 0.25}]

**Lush Atmospheric Pad (Jupiter-8 Style):**
[{\"preset_name\": \"JP-8 Strings\", \"note\": 60, \"velocity\": 80, \"start_time\": 0, \"duration\": 4.0}, {\"preset_name\": \"JP-8 Strings\", \"note\": 64, \"velocity\": 75, \"start_time\": 0, \"duration\": 4.0}, {\"preset_name\": \"JP-8 Strings\", \"note\": 67, \"velocity\": 70, \"start_time\": 0, \"duration\": 4.0}]

**Classic 80s Electric Piano:**
[{\"preset_name\": \"DX7 E.Piano\", \"note\": 60, \"velocity\": 90, \"start_time\": 0, \"duration\": 1.0}, {\"preset_name\": \"DX7 E.Piano\", \"note\": 64, \"velocity\": 85, \"start_time\": 1.0, \"duration\": 1.0}, {\"preset_name\": \"DX7 E.Piano\", \"note\": 67, \"velocity\": 80, \"start_time\": 2.0, \"duration\": 1.0}]

**Random Preset Discovery:**
[{\"preset_random\": true, \"preset_category\": \"bass\", \"note\": 36, \"velocity\": 100, \"start_time\": 0, \"duration\": 1.0}]

**Classic Drum Pattern (808/909 Style):**
[{\"preset_name\": \"TR-808 Kick\", \"note\": 36, \"velocity\": 127, \"start_time\": 0, \"duration\": 1.0, \"channel\": 9}, {\"preset_name\": \"TR-909 Snare\", \"note\": 38, \"velocity\": 120, \"start_time\": 0.5, \"duration\": 0.3, \"channel\": 9}, {\"preset_name\": \"TR-808 Hi-Hat\", \"note\": 42, \"velocity\": 90, \"start_time\": 0.25, \"duration\": 0.08, \"channel\": 9}, {\"preset_name\": \"TR-808 Hi-Hat\", \"note\": 42, \"velocity\": 70, \"start_time\": 0.75, \"duration\": 0.08, \"channel\": 9}]

**Mixed Vintage + Modern:**
[{\"preset_name\": \"Analog Wash\", \"note\": 48, \"velocity\": 60, \"start_time\": 0, \"duration\": 4.0}, {\"preset_name\": \"Prophet Lead\", \"note\": 72, \"velocity\": 100, \"start_time\": 1.0, \"duration\": 1.0}, {\"preset_name\": \"TR-808 Kick\", \"note\": 36, \"velocity\": 127, \"start_time\": 0, \"duration\": 0.5, \"channel\": 9}, {\"note_type\": \"r2d2\", \"r2d2_emotion\": \"Excited\", \"r2d2_intensity\": 0.8, \"r2d2_complexity\": 3, \"start_time\": 2.0, \"duration\": 1.0}]

🎛️ **AVAILABLE PRESET CATEGORIES:**
• **Bass Presets** (10+): Minimoog Bass, TB-303 Acid, Jupiter Bass, Odyssey Bite, TX81Z Lately, Saw Bass, Sub Bass, etc.
• **Pad Presets** (10+): JP-8 Strings, OB Brass, Analog Wash, D-50 Fantasia, Crystal Pad, Space Pad, Dream Pad, etc.
• **Lead Presets**: Prophet Lead, Moog Lead, Sync Lead, and more coming soon
• **Keys Presets**: DX7 E.Piano, Rhodes Classic, and more coming soon
• **Drum Presets** (5+): TR-808 Kick, TR-909 Snare, TR-909 Hi-Hat, TR-808 Hi-Hat, Crash Cymbal - authentic drum machine sounds
• **Effects Presets**: Sci-Fi Zap, Sweep Up for sound design

💡 **PRESET USAGE TIPS:**
• Use **preset_name** for specific iconic sounds: \"Minimoog Bass\", \"TB-303 Acid\", \"JP-8 Strings\"
• Use **preset_category** + **preset_random**: true for creative exploration
• Add **preset_variation** for subtle customization: \"bright\", \"dark\", \"squelchy\"
• Mix presets freely with MIDI, R2D2, and synthesis for unique combinations
• Perfect for instant access to legendary synthesizer sounds from the 70s-90s!

💡 **R2D2 & SYNTHESIS INTEGRATION TIPS:**
• Set note_type=\"r2d2\" to create robotic expressions with 9 emotions
• Use synth_type for custom synthesis (19 types: sine, square, fm, granular, kick, snare, zap, pad, etc.)
• **REQUIRED for R2D2 notes**: r2d2_emotion (Happy, Sad, Excited, Worried, Curious, Affirmative, Negative, Surprised, Thoughtful)
• **REQUIRED for R2D2 notes**: r2d2_intensity (0.0-1.0, emotional strength)
• **REQUIRED for R2D2 notes**: r2d2_complexity (1-5, phrase complexity in syllables)
• Mix freely with MIDI notes for rich musical storytelling
• Perfect timing synchronization between all three audio systems
• Use for celebrations, reactions, confirmations, and emotional atmosphere",
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
                                    "description": "⏰ Start time in seconds. Use 0.0 for simultaneous notes (chords), incremental timing for melodies"
                                },
                                "duration": {
                                    "type": "number",
                                    "description": "⏳ Note duration in seconds. Try: 0.25=16th, 0.5=8th, 1.0=quarter, 2.0=half, 4.0=whole note"
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
                            "required": ["start_time", "duration"],
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

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({
            "tools": tools
        })),
        error: None,
    }
}

fn handle_resources_list(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling resources/list request");

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({
            "resources": []
        })),
        error: None,
    }
}

fn handle_prompts_list(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling prompts/list request");

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({
            "prompts": []
        })),
        error: None,
    }
}

fn handle_tool_call(params: Option<Value>, id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("Handling tools/call request");

    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            };
        }
    };

    let tool_params: ToolCallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid tool call params: {}", e),
                    data: None,
                }),
            };
        }
    };

    match tool_params.name.as_str() {
        "play_notes" => handle_play_notes_tool(tool_params.arguments, id),
        "define_sequence_pattern" => handle_define_pattern_tool(tool_params.arguments, id),
        "play_sequence" => handle_play_sequence_tool(tool_params.arguments, id),
        "list_patterns" => handle_list_patterns_tool(id),
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Unknown tool: {}", tool_params.name),
                data: None,
            }),
        },
    }
}

fn handle_play_notes_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    tracing::info!(
        "handle_play_notes_tool called with arguments: {:?}",
        arguments
    );

    // Parse the simple sequence from JSON
    let sequence: SimpleSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            tracing::error!("Failed to parse note sequence: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Failed to parse note sequence: {}", e),
                    data: None,
                }),
            };
        }
    };

    if sequence.notes.is_empty() {
        tracing::warn!("Note sequence is empty");
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Note sequence cannot be empty".to_string(),
                data: None,
            }),
        };
    }

    // Analyze the sequence to determine the playback mode
    let mut has_midi = false;
    let mut has_r2d2 = false;
    let mut has_synthesis = false;
    let mut has_presets = false;

    for note in &sequence.notes {
        // Validate note parameters first
        if let Err(e) = note.validate_r2d2() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid R2D2 parameters: {}", e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_synthesis() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid synthesis parameters: {}", e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_preset() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid preset parameters: {}", e),
                    data: None,
                }),
            };
        }

        // Categorize note types
        if note.note_type == "r2d2" {
            has_r2d2 = true;
        } else if note.is_synthesis() {
            has_synthesis = true;
        } else if note.is_preset() {
            has_presets = true;
        } else {
            has_midi = true;
        }
    }

    tracing::info!(
        "Sequence analysis: {} notes, has_midi: {}, has_r2d2: {}, has_synthesis: {}, has_presets: {}",
        sequence.notes.len(),
        has_midi,
        has_r2d2,
        has_synthesis,
        has_presets
    );

    // Create MIDI player
    let player = match MidiPlayer::new() {
        Ok(p) => {
            tracing::info!("Successfully created MIDI player");
            p
        }
        Err(e) => {
            tracing::error!("Failed to create MIDI player: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to create MIDI player: {}", e),
                    data: None,
                }),
            };
        }
    };

    // Use universal enhanced mixed playback for ALL sequences (supports everything!)
    let mode = match (has_midi, has_r2d2, has_synthesis, has_presets) {
        (true, true, true, true) => "MIDI + R2D2 + Synthesis + Presets",
        (true, true, true, false) => "MIDI + R2D2 + Synthesis",
        (true, true, false, true) => "MIDI + R2D2 + Presets",
        (true, false, true, true) => "MIDI + Synthesis + Presets",
        (false, true, true, true) => "R2D2 + Synthesis + Presets",
        (true, false, true, false) => "MIDI + Synthesis",
        (false, true, true, false) => "R2D2 + Synthesis",
        (false, false, true, true) => "Synthesis + Presets",
        (true, false, false, true) => "MIDI + Presets",
        (false, true, false, true) => "R2D2 + Presets",
        (false, false, true, false) => "Synthesis Only",
        (false, true, false, false) => "R2D2 Only",
        (false, false, false, true) => "Presets Only",
        (true, false, false, false) => "Pure MIDI",
        _ => "Mixed",
    };

    tracing::info!(
        "Using universal enhanced mixed playback for {} sequence",
        mode
    );
    let playback_result = player.play_enhanced_mixed(sequence);

    // Handle the result
    match playback_result {
        Ok(()) => {
            let mode_description = match (has_midi, has_r2d2, has_synthesis, has_presets) {
                (true, true, true, true) => {
                    "🎵🤖🎛️🎹 Ultimate audio sequence playback started successfully! MIDI music, R2D2 expressions, custom synthesis, and classic preset sounds are now playing in perfect synchronization."
                }
                (true, true, true, false) => {
                    "🎵🤖🎛️ Universal audio sequence playback started successfully! MIDI music, R2D2 expressions, and custom synthesis are now playing in perfect synchronization."
                }
                (true, true, false, true) => {
                    "🎵🤖🎹 Mixed MIDI, R2D2, and preset sequence playback started successfully! Traditional music, robotic expressions, and vintage synthesizer sounds are now playing together."
                }
                (true, false, true, true) => {
                    "🎵🎛️🎹 Mixed MIDI, synthesis, and preset sequence playback started successfully! Traditional music, custom synthesis, and classic sounds are now playing together."
                }
                (false, true, true, true) => {
                    "🤖🎛️🎹 Mixed R2D2, synthesis, and preset sequence playback started successfully! Robotic expressions, custom synthesis, and vintage sounds are now playing in synchronization."
                }
                (true, false, true, false) => {
                    "🎵🎛️ Mixed MIDI and synthesis sequence playback started successfully! Traditional music and custom synthesized sounds are now playing together."
                }
                (false, true, true, false) => {
                    "🤖🎛️ Mixed R2D2 and synthesis sequence playback started successfully! Robotic expressions and custom sounds are now playing in synchronization."
                }
                (true, true, false, false) => {
                    "🎵🤖 Mixed MIDI and R2D2 sequence playback started successfully! The music and robotic expressions are now playing in perfect synchronization."
                }
                (true, false, false, true) => {
                    "🎵🎹 Mixed MIDI and preset sequence playback started successfully! Traditional music and classic synthesizer sounds are now playing together."
                }
                (false, true, false, true) => {
                    "🤖🎹 Mixed R2D2 and preset sequence playback started successfully! Robotic expressions and vintage synthesizer sounds are now playing together."
                }
                (false, false, true, true) => {
                    "🎛️🎹 Mixed synthesis and preset sequence playback started successfully! Custom synthesis and classic vintage sounds are now playing together."
                }
                (false, true, false, false) => {
                    "🤖 R2D2 expression sequence playback started successfully! The robotic vocalizations are now playing."
                }
                (false, false, true, false) => {
                    "🎛️ Custom synthesis sequence playback started successfully! Your unique synthesized sounds are now playing."
                }
                (false, false, false, true) => {
                    "🎹 Classic synthesizer preset sequence playback started successfully! Authentic vintage synthesizer sounds are now playing."
                }
                _ => {
                    "🎵 Pure MIDI sequence playback started successfully! The music is now playing."
                }
            };

            tracing::info!("Playback completed successfully");
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": mode_description
                        }
                    ]
                })),
                error: None,
            }
        }
        Err(e) => {
            tracing::error!("Failed to play sequence: {}", e);
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to play sequence: {}", e),
                    data: None,
                }),
            }
        }
    }
}

fn handle_define_pattern_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    tracing::info!(
        "handle_define_pattern_tool called with arguments: {:?}",
        arguments
    );

    // Parse the sequence pattern from JSON
    let pattern: SequencePattern = match serde_json::from_value(arguments) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to parse sequence pattern: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Failed to parse sequence pattern: {}", e),
                    data: None,
                }),
            };
        }
    };

    if pattern.notes.is_empty() {
        tracing::warn!("Pattern notes are empty");
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Pattern notes cannot be empty".to_string(),
                data: None,
            }),
        };
    }

    // Validate all notes in the pattern
    for (i, note) in pattern.notes.iter().enumerate() {
        if let Err(e) = note.validate_r2d2() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid R2D2 parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_synthesis() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid synthesis parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_preset() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid preset parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }
    }

    // Store the pattern
    let pattern_name = pattern.name.clone();
    match PATTERN_STORE.lock() {
        Ok(mut store) => {
            let pattern_info = format!(
                "Pattern '{}' with {} notes, duration: {:.2}s",
                pattern.name,
                pattern.notes.len(),
                pattern.get_pattern_duration()
            );

            let category_info = pattern
                .category
                .as_ref()
                .map(|c| format!(" (category: {})", c))
                .unwrap_or_default();

            let tags_info = if !pattern.tags.is_empty() {
                format!(" [tags: {}]", pattern.tags.join(", "))
            } else {
                String::new()
            };

            store.insert(pattern.name.clone(), pattern);
            tracing::info!("Stored pattern: {}", pattern_name);

            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("🎼 Successfully defined sequence pattern: {}{}{}

📋 **Pattern Details:**
• **Name**: {}
• **Notes**: {} notes
• **Duration**: {:.2} seconds
• **Tempo**: {} BPM
{}{}

✅ Pattern is now stored and ready to use with the `play_sequence` tool!

💡 **Usage Example:**
```json
{{
  \"patterns\": [
    {{\"pattern_name\": \"{}\", \"start_time_offset\": 0}}
  ]
}}
```",
                                pattern_info, category_info, tags_info,
                                pattern_name,
                                store.get(&pattern_name).unwrap().notes.len(),
                                store.get(&pattern_name).unwrap().get_pattern_duration(),
                                store.get(&pattern_name).unwrap().tempo,
                                store.get(&pattern_name).unwrap().description.as_ref()
                                    .map(|d| format!("\n• **Description**: {}", d))
                                    .unwrap_or_default(),
                                if !store.get(&pattern_name).unwrap().tags.is_empty() {
                                    format!("\n• **Tags**: {}", store.get(&pattern_name).unwrap().tags.join(", "))
                                } else { String::new() },
                                pattern_name
                            )
                        }
                    ]
                })),
                error: None,
            }
        }
        Err(e) => {
            tracing::error!("Failed to lock pattern store: {}", e);
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: "Failed to store pattern due to internal error".to_string(),
                    data: None,
                }),
            }
        }
    }
}

fn handle_play_sequence_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    tracing::info!(
        "handle_play_sequence_tool called with arguments: {:?}",
        arguments
    );

    // Parse the extended sequence from JSON
    let extended_sequence: ExtendedSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
            tracing::error!("Failed to parse extended sequence: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Failed to parse extended sequence: {}", e),
                    data: None,
                }),
            };
        }
    };

    if extended_sequence.notes.is_empty() && extended_sequence.patterns.is_empty() {
        tracing::warn!("Extended sequence has no notes or patterns");
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Sequence must contain either notes or pattern references".to_string(),
                data: None,
            }),
        };
    }

    // Validate individual notes
    for (i, note) in extended_sequence.notes.iter().enumerate() {
        if let Err(e) = note.validate_r2d2() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid R2D2 parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_synthesis() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid synthesis parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }

        if let Err(e) = note.validate_preset() {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid preset parameters in note {}: {}", i + 1, e),
                    data: None,
                }),
            };
        }
    }

    // Resolve pattern references to get final sequence
    let resolved_sequence = match PATTERN_STORE.lock() {
        Ok(store) => match extended_sequence.resolve_patterns(&store) {
            Ok(seq) => seq,
            Err(e) => {
                tracing::error!("Failed to resolve patterns: {}", e);
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: format!("Failed to resolve patterns: {}", e),
                        data: None,
                    }),
                };
            }
        },
        Err(e) => {
            tracing::error!("Failed to lock pattern store: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: "Failed to access pattern store".to_string(),
                    data: None,
                }),
            };
        }
    };

    if resolved_sequence.notes.is_empty() {
        tracing::warn!("Resolved sequence is empty");
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Resolved sequence cannot be empty".to_string(),
                data: None,
            }),
        };
    }

    // Create MIDI player and play the resolved sequence
    let player = match MidiPlayer::new() {
        Ok(p) => {
            tracing::info!("Successfully created MIDI player for sequence playback");
            p
        }
        Err(e) => {
            tracing::error!("Failed to create MIDI player: {}", e);
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to create MIDI player: {}", e),
                    data: None,
                }),
            };
        }
    };

    let pattern_count = extended_sequence.patterns.len();
    let individual_notes_count = extended_sequence.notes.len();
    let total_resolved_notes = resolved_sequence.notes.len();

    tracing::info!(
        "Playing sequence with {} pattern references, {} individual notes, {} total resolved notes",
        pattern_count,
        individual_notes_count,
        total_resolved_notes
    );

    match player.play_enhanced_mixed(resolved_sequence) {
        Ok(()) => {
            let composition_description = match (individual_notes_count > 0, pattern_count > 0) {
                (true, true) => format!(
                    "🎼🎵 Enhanced sequence playback started successfully! Playing {} individual notes plus {} pattern references (expanded to {} total notes) in perfect synchronization.",
                    individual_notes_count, pattern_count, total_resolved_notes
                ),
                (false, true) => format!(
                    "🎼 Pattern-based sequence playback started successfully! Playing {} pattern references (expanded to {} total notes) with all transformations applied.",
                    pattern_count, total_resolved_notes
                ),
                (true, false) => format!(
                    "🎵 Individual note sequence playback started successfully! Playing {} notes.",
                    individual_notes_count
                ),
                (false, false) => "🎵 Sequence playback started successfully!".to_string(),
            };

            tracing::info!("Enhanced sequence playback completed successfully");
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": composition_description
                        }
                    ]
                })),
                error: None,
            }
        }
        Err(e) => {
            tracing::error!("Failed to play enhanced sequence: {}", e);
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to play enhanced sequence: {}", e),
                    data: None,
                }),
            }
        }
    }
}

fn handle_list_patterns_tool(id: Option<Value>) -> JsonRpcResponse {
    tracing::info!("handle_list_patterns_tool called");

    match PATTERN_STORE.lock() {
        Ok(store) => {
            if store.is_empty() {
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": "📋 **Sequence Patterns**

🔍 No patterns defined yet.

💡 **Get Started:**
Use the `define_sequence_pattern` tool to create reusable musical patterns that you can then reference with the `play_sequence` tool.

**Example:**
```json
{
  \"name\": \"house_beat\",
  \"description\": \"Classic 4/4 house drum pattern\",
  \"category\": \"drums\",
  \"notes\": [
    {\"note\": 36, \"velocity\": 120, \"start_time\": 0, \"duration\": 0.1, \"channel\": 9},
    {\"note\": 42, \"velocity\": 80, \"start_time\": 0.25, \"duration\": 0.05, \"channel\": 9},
    {\"note\": 38, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.1, \"channel\": 9}
  ]
}
```"
                            }
                        ]
                    })),
                    error: None,
                }
            } else {
                let mut patterns_by_category: std::collections::HashMap<
                    String,
                    Vec<&SequencePattern>,
                > = std::collections::HashMap::new();

                // Group patterns by category
                for pattern in store.values() {
                    let category = pattern.category.as_deref().unwrap_or("uncategorized");
                    patterns_by_category
                        .entry(category.to_string())
                        .or_default()
                        .push(pattern);
                }

                // Sort categories
                let mut categories: Vec<_> = patterns_by_category.keys().collect();
                categories.sort();

                let mut output = String::from("📋 **Sequence Patterns**\n\n");
                output.push_str(&format!("🎼 **{}** patterns available:\n\n", store.len()));

                for category in categories {
                    let patterns = patterns_by_category.get(category).unwrap();
                    let category_icon = match category.as_str() {
                        "drums" => "🥁",
                        "bass" => "🎸",
                        "melody" => "🎵",
                        "chords" => "🎹",
                        "harmony" => "🎼",
                        "effects" => "🎛️",
                        _ => "📝",
                    };

                    output.push_str(&format!(
                        "## {} **{}** ({})\n",
                        category_icon,
                        category,
                        patterns.len()
                    ));

                    for pattern in patterns {
                        output.push_str(&format!(
                            "• **{}** - {} notes, {:.1}s duration",
                            pattern.name,
                            pattern.notes.len(),
                            pattern.get_pattern_duration()
                        ));

                        if let Some(desc) = &pattern.description {
                            output.push_str(&format!("\n  *{}*", desc));
                        }

                        if !pattern.tags.is_empty() {
                            output.push_str(&format!(" [{}]", pattern.tags.join(", ")));
                        }

                        output.push('\n');
                    }
                    output.push('\n');
                }

                output.push_str("💡 **Usage:** Reference these patterns in the `play_sequence` tool with transformations like transposition, instrument changes, and repetition!");

                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": output
                            }
                        ]
                    })),
                    error: None,
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to lock pattern store: {}", e);
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: "Failed to access pattern store".to_string(),
                    data: None,
                }),
            }
        }
    }
}

pub fn run_stdio_server() {
    tracing::info!("MCP server starting");

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = stdin.lock();

    for line in reader.lines() {
        match line {
            Ok(line) if !line.trim().is_empty() => {
                tracing::debug!("Received: {}", line);

                let request: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::error!("Failed to parse JSON-RPC request: {}", e);
                        let error_response = JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: None,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32700,
                                message: "Parse error".to_string(),
                                data: Some(json!(e.to_string())),
                            }),
                        };
                        if let Ok(response_json) = serde_json::to_string(&error_response) {
                            let _ = writeln!(stdout, "{}", response_json);
                            let _ = stdout.flush();
                        }
                        continue;
                    }
                };

                let response = match request.method.as_str() {
                    "initialize" => handle_initialize(request.params, request.id),
                    "notifications/initialized" => {
                        tracing::info!("Client initialized");
                        continue; // No response needed for notifications
                    }
                    "tools/list" => handle_tools_list(request.id),
                    "resources/list" => handle_resources_list(request.id),
                    "prompts/list" => handle_prompts_list(request.id),
                    "tools/call" => handle_tool_call(request.params, request.id),
                    _ => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32601,
                            message: "Method not found".to_string(),
                            data: None,
                        }),
                    },
                };

                match serde_json::to_string(&response) {
                    Ok(response_json) => {
                        tracing::debug!("Sending: {}", response_json);
                        let _ = writeln!(stdout, "{}", response_json);
                        let _ = stdout.flush();
                    }
                    Err(e) => {
                        tracing::error!("Failed to serialize response: {}", e);
                    }
                }
            }
            Ok(_) => {
                // Empty line, ignore
            }
            Err(e) => {
                tracing::error!("Error reading from stdin: {}", e);
                break;
            }
        }
    }

    tracing::info!("MCP server shutting down");
}
