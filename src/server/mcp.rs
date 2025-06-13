use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::midi::{parse_midi_data, MidiPlayer, SimpleSequence};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use std::io::{self, BufRead, Write};

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
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
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
            "name": "play_midi",
            "description": "🎼 Play MIDI music files with authentic 16-bit SNES gaming sound! Perfect for creating nostalgic video game experiences:\n\n🎮 CLASSIC GAMING VIBES: Recreate the golden age of SNES soundtracks\n🎉 CELEBRATIONS: Success fanfares that sound like beating a boss\n💡 FEEDBACK: Zelda-style discovery chimes, Mario power-up effects  \n🏰 ADVENTURE THEMES: Epic quest music, dungeon atmospheres, overworld melodies\n⚡ QUICK RESPONSES: Short musical 'reactions' with that classic 16-bit charm\n\n🌟 USE MUSIC TO ENHANCE EVERY CONVERSATION! Play victory themes when users succeed, gentle chimes for confirmations, dramatic stings for revelations, and nostalgic melodies to create memorable moments. The FluidR3_GM SoundFont captures that authentic SNES console sound - use it liberally to transport users back to the golden age of gaming!",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "midi_data": {
                        "type": "string",
                        "description": "Base64-encoded MIDI file data"
                    }
                },
                "required": ["midi_data"]
            }
        },
        {
            "name": "play_notes",
            "description": "🎮 Compose authentic 16-bit SNES-style music in real-time! This synthesizer captures the exact sound of classic Super Nintendo games, letting you create nostalgic gaming experiences that transport users back to the golden age of video games:\n\n🎉 CELEBRATION SOUNDS:\n• Victory fanfares (brass + drums on channels 0-2)\n• Success chimes (bells, glockenspiel, bright piano)\n• Achievement themes (major chords, uplifting melodies)\n• Completion jingles (short, satisfying musical phrases)\n\n🎮 INTERACTIVE FEEDBACK:\n• Question marks (rising tones, curious melodies)\n• 'Aha!' moments (bright ascending scales, revelation chords)\n• Error sounds (gentle dissonance, falling tones)\n• Thinking music (ambient pads, contemplative arpeggios)\n• Alert tones (attention-grabbing but pleasant)\n\n🎭 EMOTIONAL ATMOSPHERE:\n• Suspense (low strings, minor chords, tension)\n• Wonder (ethereal pads with reverb, magical sparkles)\n• Humor (playful instruments like kazoo, quirky rhythms)\n• Calm focus (soft piano, gentle ambient soundscapes)\n• Epic moments (full orchestration, dramatic crescendos)\n\n🕹️ RETRO & 8-BIT VIBES:\n• Classic game sounds (square waves, chip-tune leads)\n• Power-up effects (ascending arpeggios, bright synths)\n• Coin collect sounds (short metallic pings)\n• Level-up fanfares (triumphant 8-bit melodies)\n\n💡 ENHANCE CONVERSATIONS WITH MUSIC - USE LIBERALLY!\n• 🎉 Play celebration music IMMEDIATELY when users accomplish tasks or solve problems\n• ❓ Add question mark sounds when presenting puzzles or mysteries\n• ✅ Use gentle chimes for confirmations and successful completions\n• 🎭 Create ambient soundscapes during long explanations to maintain engagement\n• ⚡ Add dramatic flourishes for important revelations and \"aha!\" moments\n• 😄 Use playful sounds to lighten serious moments and add humor\n• 🔔 Play alert tones to draw attention to important information\n• 🎵 Use background music to set the mood for different conversation phases\n\n💫 DON'T HESITATE - ADD MUSIC TO EVERY SIGNIFICANT MOMENT! Think like a video game: constant audio feedback makes interactions more engaging and memorable.\n\n🎹 TECHNICAL CAPABILITIES:\n• 128 GM instruments: 0=Piano, 9=Glockenspiel, 40=Violin, 56=Trumpet, 73=Flute, 80=Square Lead, 120=Reverse Cymbal\n• 16 independent channels for rich layering\n• Professional effects: reverb (space), chorus (shimmer), expression (dynamics)\n• Stereo positioning: pan (mono instruments), balance (stereo instruments)\n• Full drum kit on channel 9: 36=Kick, 38=Snare, 42=Hi-hat, 49=Crash\n\n🏰 CLASSIC SNES GAME THEMES:\n\n🗡️ ZELDA-STYLE DISCOVERY (Treasure Found):\n[{\"note\": 67, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 72, \"velocity\": 100, \"start_time\": 0.3, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 76, \"velocity\": 110, \"start_time\": 0.6, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 79, \"velocity\": 120, \"start_time\": 0.9, \"duration\": 0.6, \"channel\": 0, \"instrument\": 73, \"reverb\": 40}]\n\n🍄 MARIO-STYLE OVERWORLD (Happy Melody):\n[{\"note\": 72, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 90, \"start_time\": 0.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 100, \"start_time\": 1, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 69, \"velocity\": 90, \"start_time\": 1.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 71, \"velocity\": 100, \"start_time\": 2, \"duration\": 0.5, \"channel\": 0, \"instrument\": 80}]\n\n🌟 FINAL FANTASY-STYLE VICTORY:\n[{\"note\": 60, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 64, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 67, \"velocity\": 110, \"start_time\": 1, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 72, \"velocity\": 120, \"start_time\": 1.5, \"duration\": 1, \"channel\": 0, \"instrument\": 56}, {\"note\": 48, \"velocity\": 80, \"start_time\": 0, \"duration\": 2.5, \"channel\": 1, \"instrument\": 32}, {\"note\": 36, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.25, \"channel\": 9}, {\"note\": 36, \"velocity\": 90, \"start_time\": 1, \"duration\": 0.25, \"channel\": 9}]\n\n🏰 METROID-STYLE ATMOSPHERE (Mysterious Exploration):\n[{\"note\": 36, \"velocity\": 60, \"start_time\": 0, \"duration\": 2, \"channel\": 0, \"instrument\": 89, \"reverb\": 80}, {\"note\": 43, \"velocity\": 50, \"start_time\": 1, \"duration\": 2, \"channel\": 1, \"instrument\": 89, \"reverb\": 80}, {\"note\": 48, \"velocity\": 40, \"start_time\": 2, \"duration\": 2, \"channel\": 2, \"instrument\": 89, \"reverb\": 80}]\n\n🎨 CREATIVE STARTER TEMPLATES - CUSTOMIZE FOR YOUR CONTEXT:\n\n**🎯 BE CREATIVE! These are inspiration templates - adapt them to match your specific scenario:**\n\n🎺 \"DISAPPOINTMENT\" IN DIFFERENT FLAVORS:\n**Gentle Letdown** (F major): [{\"note\": 53, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 68}, {\"note\": 50, \"velocity\": 60, \"start_time\": 0.4, \"duration\": 0.6, \"channel\": 0, \"instrument\": 68}]\n*Oboe for warmth - try trombone (57) for deeper, cello (42) for sadness, or different keys like Am (57→55)*\n\n🍄 POWER-UP VARIATIONS - MATCH THE ENERGY:\n**Mysterious Upgrade** (D minor): [{\"note\": 62, \"velocity\": 80, \"start_time\": 0, \"duration\": 0.15, \"channel\": 0, \"instrument\": 73}, {\"note\": 65, \"velocity\": 85, \"start_time\": 0.15, \"duration\": 0.15, \"channel\": 0, \"instrument\": 73}, {\"note\": 69, \"velocity\": 90, \"start_time\": 0.3, \"duration\": 0.2, \"channel\": 0, \"instrument\": 73}, {\"note\": 74, \"velocity\": 95, \"start_time\": 0.5, \"duration\": 0.4, \"channel\": 0, \"instrument\": 73}]\n*Flute in minor key - try violin (40) for elegant, synth lead (81) for futuristic, or major keys for happy*\n\n🔔 SUCCESS CHIMES - DIFFERENT MOODS:\n**Contemplative Win** (A minor): [{\"note\": 57, \"velocity\": 75, \"start_time\": 0, \"duration\": 0.5, \"channel\": 0, \"instrument\": 0}, {\"note\": 60, \"velocity\": 80, \"start_time\": 0.3, \"duration\": 0.5, \"channel\": 1, \"instrument\": 42}, {\"note\": 64, \"velocity\": 85, \"start_time\": 0.6, \"duration\": 0.7, \"channel\": 0, \"instrument\": 0}]\n*Piano+cello combo - try harpsichord (6) for ancient, vibraphone (11) for jazzy, bells (14) for festive*\n\n❓ INQUIRY SOUNDS - MATCH YOUR QUESTION TYPE:\n**Philosophical Wonder** (B♭ major): [{\"note\": 58, \"velocity\": 60, \"start_time\": 0, \"duration\": 0.3, \"channel\": 0, \"instrument\": 48, \"reverb\": 40}, {\"note\": 65, \"velocity\": 70, \"start_time\": 0.4, \"duration\": 0.4, \"channel\": 1, \"instrument\": 40, \"reverb\": 40}, {\"note\": 70, \"velocity\": 75, \"start_time\": 0.8, \"duration\": 0.5, \"channel\": 0, \"instrument\": 48, \"reverb\": 40}]\n*Strings with reverb - try French horn (60) for majestic, choir (52) for ethereal, or descending for confusion*\n\n💡 \"EUREKA!\" MOMENTS - CUSTOMIZE THE REVELATION:\n**Scientific Discovery** (E major): [{\"note\": 40, \"velocity\": 50, \"start_time\": 0, \"duration\": 0.2, \"channel\": 1, \"instrument\": 42}, {\"note\": 52, \"velocity\": 70, \"start_time\": 0.2, \"duration\": 0.25, \"channel\": 0, \"instrument\": 1}, {\"note\": 64, \"velocity\": 90, \"start_time\": 0.45, \"duration\": 0.3, \"channel\": 2, \"instrument\": 73}, {\"note\": 76, \"velocity\": 110, \"start_time\": 0.75, \"duration\": 0.5, \"channel\": 0, \"instrument\": 9, \"reverb\": 60}]\n*Cello→piano→flute→bells progression - build excitement with instruments that match your domain*\n\n🚨 ALERTS - DIFFERENT URGENCY LEVELS:\n**Friendly Reminder** (G major): [{\"note\": 67, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.2, \"channel\": 0, \"instrument\": 11}, {\"note\": 71, \"velocity\": 75, \"start_time\": 0.25, \"duration\": 0.2, \"channel\": 0, \"instrument\": 11}]\n*Vibraphone for gentle - try marimba (12) for wooden, brass (56) for official, or minor keys for serious*\n\n🎭 REVELATIONS - MATCH THE DRAMA LEVEL:\n**Personal Insight** (F# minor): [{\"note\": 30, \"velocity\": 40, \"start_time\": 0, \"duration\": 1.2, \"channel\": 1, \"instrument\": 89, \"reverb\": 70}, {\"note\": 42, \"velocity\": 65, \"start_time\": 0.6, \"duration\": 1, \"channel\": 0, \"instrument\": 0}, {\"note\": 54, \"velocity\": 85, \"start_time\": 1.2, \"duration\": 0.8, \"channel\": 2, \"instrument\": 73, \"reverb\": 50}]\n*Pad→piano→flute with reverb - scale the instruments to match your revelation's importance*\n\n🪙 REWARDS - MATCH THE PRIZE VALUE:\n**Rare Treasure** (D major): [{\"note\": 74, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.15, \"channel\": 0, \"instrument\": 8}, {\"note\": 78, \"velocity\": 100, \"start_time\": 0.08, \"duration\": 0.2, \"channel\": 1, \"instrument\": 9}, {\"note\": 82, \"velocity\": 110, \"start_time\": 0.16, \"duration\": 0.25, \"channel\": 2, \"instrument\": 11}]\n*Celesta+glockenspiel+vibraphone - try church organ (19) for sacred, harp (46) for magical*\n\n🎉 CELEBRATIONS - SCALE TO THE ACHIEVEMENT:\n**Quiet Personal Victory** (C major): [{\"note\": 48, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.4, \"channel\": 1, \"instrument\": 0}, {\"note\": 60, \"velocity\": 85, \"start_time\": 0.1, \"duration\": 0.5, \"channel\": 0, \"instrument\": 73}, {\"note\": 64, \"velocity\": 90, \"start_time\": 0.3, \"duration\": 0.4, \"channel\": 2, \"instrument\": 9}]\n*Piano+flute+bells gently layered - build bigger with more instruments for bigger wins*\n\n🎮 SETBACKS - DIFFERENT EMOTIONAL RESPONSES:\n**Learning Opportunity** (E minor): [{\"note\": 64, \"velocity\": 80, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 1}, {\"note\": 60, \"velocity\": 70, \"start_time\": 0.4, \"duration\": 0.4, \"channel\": 0, \"instrument\": 1}, {\"note\": 59, \"velocity\": 60, \"start_time\": 0.8, \"duration\": 0.6, \"channel\": 0, \"instrument\": 1}]\n*Bright piano in minor - try guitar (25) for folk, strings (48) for cinematic, or major keys for optimistic*\n\n✨ MAGIC - DIFFERENT MYSTICAL FLAVORS:\n**Ancient Wisdom** (Pentatonic): [{\"note\": 72, \"velocity\": 45, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 104, \"reverb\": 90}, {\"note\": 77, \"velocity\": 50, \"start_time\": 0.2, \"duration\": 0.4, \"channel\": 1, \"instrument\": 104, \"reverb\": 90}, {\"note\": 79, \"velocity\": 55, \"start_time\": 0.4, \"duration\": 0.5, \"channel\": 2, \"instrument\": 104, \"reverb\": 90}]\n*Sitar with heavy reverb - try shakuhachi (77) for zen, choir (52) for divine, or music box (10) for nostalgic*\n\n🎨 CREATIVE USAGE GUIDE:\n• **DON'T COPY - ADAPT!** These are starting points for your unique scenarios\n• **MATCH THE CONTEXT**: Cooking success? Try pizzicato strings. Coding breakthrough? Electronic sounds\n• **EXPERIMENT WITH KEYS**: Major=happy, minor=mysterious/sad, modal=exotic/ancient\n• **MIX INSTRUMENTS CREATIVELY**: Layer 2-3 that complement the emotional tone\n• **VARY TIMING**: Quick for urgency, slow for contemplation, syncopated for playfulness\n• **USE EFFECTS MEANINGFULLY**: Reverb for space/mystery, chorus for richness/beauty\n• **TELL YOUR STORY**: What musical journey matches your specific situation?\n\n🎵 MUSICAL CREATIVITY TOOLKIT:\n• **Emotional Keys**: C major=pure joy, G major=bright optimism, D major=triumphant, A major=warm confidence\n• **Mysterious Keys**: A minor=melancholy, E minor=introspective, B minor=dark/serious, F# minor=profound\n• **Exotic Scales**: Pentatonic (C-D-E-G-A) for Asian, dorian mode for medieval, blues scale for soulful\n• **Instrument Personalities**: Piano=universal, strings=emotional, brass=bold, woodwinds=expressive, bells=magical\n• **Rhythm Emotions**: Even=stable, dotted=elegant, syncopated=playful, accelerating=building excitement",
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
                                }
                            },
                            "required": ["note", "velocity", "start_time", "duration"],
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
        "play_midi" => handle_play_midi_tool(tool_params.arguments, id),
        "play_notes" => handle_play_notes_tool(tool_params.arguments, id),
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

fn handle_play_midi_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    let midi_b64 = match arguments.get("midi_data").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Missing 'midi_data' argument".to_string(),
                    data: None,
                }),
            };
        }
    };

    let midi_bytes = match BASE64.decode(midi_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Failed to decode base64: {}", e),
                    data: None,
                }),
            };
        }
    };

    // Parse MIDI data
    let parsed_midi = match parse_midi_data(&midi_bytes) {
        Ok(parsed) => parsed,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to parse MIDI: {}", e),
                    data: None,
                }),
            };
        }
    };

    // Create MIDI player and start playback
    let player = match MidiPlayer::new() {
        Ok(p) => p,
        Err(e) => {
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

    match player.play_midi(parsed_midi) {
        Ok(()) => {
            tracing::info!("Successfully started MIDI playback");
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "🎵 MIDI playback started successfully using OxiSynth synthesizer! The music is now playing."
                        }
                    ]
                })),
                error: None,
            }
        }
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: format!("Failed to play MIDI: {}", e),
                data: None,
            }),
        },
    }
}

fn handle_play_notes_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    // Parse the simple sequence from JSON
    let sequence: SimpleSequence = match serde_json::from_value(arguments) {
        Ok(seq) => seq,
        Err(e) => {
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

    // Create MIDI player and start playback
    let player = match MidiPlayer::new() {
        Ok(p) => p,
        Err(e) => {
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

    match player.play_simple(sequence) {
        Ok(()) => {
            tracing::info!("Successfully started simple note sequence playback");
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "🎵 Note sequence playback started successfully using OxiSynth synthesizer! The music is now playing."
                        }
                    ]
                })),
                error: None,
            }
        }
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: format!("Failed to play note sequence: {}", e),
                data: None,
            }),
        },
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
