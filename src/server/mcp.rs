use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::expressive::{ExpressiveSynth, R2D2Emotion, R2D2Expression, R2D2Voice};
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
            "description": "🎮🤖 Compose authentic 16-bit SNES-style music with inline R2D2 expressions! Create rich, expressive musical storytelling where robotic emotions are perfectly synchronized with MIDI accompaniment. This enhanced synthesizer combines classic Super Nintendo gaming sounds with authentic R2D2-style vocalizations:\n\n🎉 CELEBRATION SOUNDS:\n• Victory fanfares (brass + drums on channels 0-2)\n• Success chimes (bells, glockenspiel, bright piano)\n• Achievement themes (major chords, uplifting melodies)\n• Completion jingles (short, satisfying musical phrases)\n\n🎮 INTERACTIVE FEEDBACK:\n• Question marks (rising tones, curious melodies)\n• 'Aha!' moments (bright ascending scales, revelation chords)\n• Error sounds (gentle dissonance, falling tones)\n• Thinking music (ambient pads, contemplative arpeggios)\n• Alert tones (attention-grabbing but pleasant)\n\n🎭 EMOTIONAL ATMOSPHERE:\n• Suspense (low strings, minor chords, tension)\n• Wonder (ethereal pads with reverb, magical sparkles)\n• Humor (playful instruments like kazoo, quirky rhythms)\n• Calm focus (soft piano, gentle ambient soundscapes)\n• Epic moments (full orchestration, dramatic crescendos)\n\n🕹️ RETRO & 8-BIT VIBES:\n• Classic game sounds (square waves, chip-tune leads)\n• Power-up effects (ascending arpeggios, bright synths)\n• Coin collect sounds (short metallic pings)\n• Level-up fanfares (triumphant 8-bit melodies)\n\n💡 ENHANCE CONVERSATIONS WITH MUSIC - USE LIBERALLY!\n• 🎉 Play celebration music IMMEDIATELY when users accomplish tasks or solve problems\n• ❓ Add question mark sounds when presenting puzzles or mysteries\n• ✅ Use gentle chimes for confirmations and successful completions\n• 🎭 Create ambient soundscapes during long explanations to maintain engagement\n• ⚡ Add dramatic flourishes for important revelations and \"aha!\" moments\n• 😄 Use playful sounds to lighten serious moments and add humor\n• 🔔 Play alert tones to draw attention to important information\n• 🎵 Use background music to set the mood for different conversation phases\n\n💫 DON'T HESITATE - ADD MUSIC TO EVERY SIGNIFICANT MOMENT! Think like a video game: constant audio feedback makes interactions more engaging and memorable.\n\n🎹 TECHNICAL CAPABILITIES:\n• 128 GM instruments: 0=Piano, 9=Glockenspiel, 40=Violin, 56=Trumpet, 73=Flute, 80=Square Lead, 120=Reverse Cymbal\n• 16 independent channels for rich layering\n• Professional effects: reverb (space), chorus (shimmer), expression (dynamics)\n• Stereo positioning: pan (mono instruments), balance (stereo instruments)\n• Full drum kit on channel 9: 36=Kick, 38=Snare, 42=Hi-hat, 49=Crash\n\n🏰 CLASSIC SNES GAME THEMES:\n\n🗡️ ZELDA-STYLE DISCOVERY (Treasure Found):\n[{\"note\": 67, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 72, \"velocity\": 100, \"start_time\": 0.3, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 76, \"velocity\": 110, \"start_time\": 0.6, \"duration\": 0.3, \"channel\": 0, \"instrument\": 73}, {\"note\": 79, \"velocity\": 120, \"start_time\": 0.9, \"duration\": 0.6, \"channel\": 0, \"instrument\": 73, \"reverb\": 40}]\n\n🍄 MARIO-STYLE OVERWORLD (Happy Melody):\n[{\"note\": 72, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 90, \"start_time\": 0.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 72, \"velocity\": 100, \"start_time\": 1, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 69, \"velocity\": 90, \"start_time\": 1.5, \"duration\": 0.25, \"channel\": 0, \"instrument\": 80}, {\"note\": 71, \"velocity\": 100, \"start_time\": 2, \"duration\": 0.5, \"channel\": 0, \"instrument\": 80}]\n\n🌟 FINAL FANTASY-STYLE VICTORY:\n[{\"note\": 60, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 64, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 67, \"velocity\": 110, \"start_time\": 1, \"duration\": 0.5, \"channel\": 0, \"instrument\": 56}, {\"note\": 72, \"velocity\": 120, \"start_time\": 1.5, \"duration\": 1, \"channel\": 0, \"instrument\": 56}, {\"note\": 48, \"velocity\": 80, \"start_time\": 0, \"duration\": 2.5, \"channel\": 1, \"instrument\": 32}, {\"note\": 36, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.25, \"channel\": 9}, {\"note\": 36, \"velocity\": 90, \"start_time\": 1, \"duration\": 0.25, \"channel\": 9}]\n\n🏰 METROID-STYLE ATMOSPHERE (Mysterious Exploration):\n[{\"note\": 36, \"velocity\": 60, \"start_time\": 0, \"duration\": 2, \"channel\": 0, \"instrument\": 89, \"reverb\": 80}, {\"note\": 43, \"velocity\": 50, \"start_time\": 1, \"duration\": 2, \"channel\": 1, \"instrument\": 89, \"reverb\": 80}, {\"note\": 48, \"velocity\": 40, \"start_time\": 2, \"duration\": 2, \"channel\": 2, \"instrument\": 89, \"reverb\": 80}]\n\n🎨 CREATIVE STARTER TEMPLATES - CUSTOMIZE FOR YOUR CONTEXT:\n\n**🎯 BE CREATIVE! These are inspiration templates - adapt them to match your specific scenario:**\n\n🎺 \"DISAPPOINTMENT\" IN DIFFERENT FLAVORS:\n**Gentle Letdown** (F major): [{\"note\": 53, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 68}, {\"note\": 50, \"velocity\": 60, \"start_time\": 0.4, \"duration\": 0.6, \"channel\": 0, \"instrument\": 68}]\n*Oboe for warmth - try trombone (57) for deeper, cello (42) for sadness, or different keys like Am (57→55)*\n\n🍄 POWER-UP VARIATIONS - MATCH THE ENERGY:\n**Mysterious Upgrade** (D minor): [{\"note\": 62, \"velocity\": 80, \"start_time\": 0, \"duration\": 0.15, \"channel\": 0, \"instrument\": 73}, {\"note\": 65, \"velocity\": 85, \"start_time\": 0.15, \"duration\": 0.15, \"channel\": 0, \"instrument\": 73}, {\"note\": 69, \"velocity\": 90, \"start_time\": 0.3, \"duration\": 0.2, \"channel\": 0, \"instrument\": 73}, {\"note\": 74, \"velocity\": 95, \"start_time\": 0.5, \"duration\": 0.4, \"channel\": 0, \"instrument\": 73}]\n*Flute in minor key - try violin (40) for elegant, synth lead (81) for futuristic, or major keys for happy*\n\n🔔 SUCCESS CHIMES - DIFFERENT MOODS:\n**Contemplative Win** (A minor): [{\"note\": 57, \"velocity\": 75, \"start_time\": 0, \"duration\": 0.5, \"channel\": 0, \"instrument\": 0}, {\"note\": 60, \"velocity\": 80, \"start_time\": 0.3, \"duration\": 0.5, \"channel\": 1, \"instrument\": 42}, {\"note\": 64, \"velocity\": 85, \"start_time\": 0.6, \"duration\": 0.7, \"channel\": 0, \"instrument\": 0}]\n*Piano+cello combo - try harpsichord (6) for ancient, vibraphone (11) for jazzy, bells (14) for festive*\n\n❓ INQUIRY SOUNDS - MATCH YOUR QUESTION TYPE:\n**Philosophical Wonder** (B♭ major): [{\"note\": 58, \"velocity\": 60, \"start_time\": 0, \"duration\": 0.3, \"channel\": 0, \"instrument\": 48, \"reverb\": 40}, {\"note\": 65, \"velocity\": 70, \"start_time\": 0.4, \"duration\": 0.4, \"channel\": 1, \"instrument\": 40, \"reverb\": 40}, {\"note\": 70, \"velocity\": 75, \"start_time\": 0.8, \"duration\": 0.5, \"channel\": 0, \"instrument\": 48, \"reverb\": 40}]\n*Strings with reverb - try French horn (60) for majestic, choir (52) for ethereal, or descending for confusion*\n\n💡 \"EUREKA!\" MOMENTS - CUSTOMIZE THE REVELATION:\n**Scientific Discovery** (E major): [{\"note\": 40, \"velocity\": 50, \"start_time\": 0, \"duration\": 0.2, \"channel\": 1, \"instrument\": 42}, {\"note\": 52, \"velocity\": 70, \"start_time\": 0.2, \"duration\": 0.25, \"channel\": 0, \"instrument\": 1}, {\"note\": 64, \"velocity\": 90, \"start_time\": 0.45, \"duration\": 0.3, \"channel\": 2, \"instrument\": 73}, {\"note\": 76, \"velocity\": 110, \"start_time\": 0.75, \"duration\": 0.5, \"channel\": 0, \"instrument\": 9, \"reverb\": 60}]\n*Cello→piano→flute→bells progression - build excitement with instruments that match your domain*\n\n🚨 ALERTS - DIFFERENT URGENCY LEVELS:\n**Friendly Reminder** (G major): [{\"note\": 67, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.2, \"channel\": 0, \"instrument\": 11}, {\"note\": 71, \"velocity\": 75, \"start_time\": 0.25, \"duration\": 0.2, \"channel\": 0, \"instrument\": 11}]\n*Vibraphone for gentle - try marimba (12) for wooden, brass (56) for official, or minor keys for serious*\n\n🎭 REVELATIONS - MATCH THE DRAMA LEVEL:\n**Personal Insight** (F# minor): [{\"note\": 30, \"velocity\": 40, \"start_time\": 0, \"duration\": 1.2, \"channel\": 1, \"instrument\": 89, \"reverb\": 70}, {\"note\": 42, \"velocity\": 65, \"start_time\": 0.6, \"duration\": 1, \"channel\": 0, \"instrument\": 0}, {\"note\": 54, \"velocity\": 85, \"start_time\": 1.2, \"duration\": 0.8, \"channel\": 2, \"instrument\": 73, \"reverb\": 50}]\n*Pad→piano→flute with reverb - scale the instruments to match your revelation's importance*\n\n🪙 REWARDS - MATCH THE PRIZE VALUE:\n**Rare Treasure** (D major): [{\"note\": 74, \"velocity\": 90, \"start_time\": 0, \"duration\": 0.15, \"channel\": 0, \"instrument\": 8}, {\"note\": 78, \"velocity\": 100, \"start_time\": 0.08, \"duration\": 0.2, \"channel\": 1, \"instrument\": 9}, {\"note\": 82, \"velocity\": 110, \"start_time\": 0.16, \"duration\": 0.25, \"channel\": 2, \"instrument\": 11}]\n*Celesta+glockenspiel+vibraphone - try church organ (19) for sacred, harp (46) for magical*\n\n🎉 CELEBRATIONS - SCALE TO THE ACHIEVEMENT:\n**Quiet Personal Victory** (C major): [{\"note\": 48, \"velocity\": 70, \"start_time\": 0, \"duration\": 0.4, \"channel\": 1, \"instrument\": 0}, {\"note\": 60, \"velocity\": 85, \"start_time\": 0.1, \"duration\": 0.5, \"channel\": 0, \"instrument\": 73}, {\"note\": 64, \"velocity\": 90, \"start_time\": 0.3, \"duration\": 0.4, \"channel\": 2, \"instrument\": 9}]\n*Piano+flute+bells gently layered - build bigger with more instruments for bigger wins*\n\n🎮 SETBACKS - DIFFERENT EMOTIONAL RESPONSES:\n**Learning Opportunity** (E minor): [{\"note\": 64, \"velocity\": 80, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 1}, {\"note\": 60, \"velocity\": 70, \"start_time\": 0.4, \"duration\": 0.4, \"channel\": 0, \"instrument\": 1}, {\"note\": 59, \"velocity\": 60, \"start_time\": 0.8, \"duration\": 0.6, \"channel\": 0, \"instrument\": 1}]\n*Bright piano in minor - try guitar (25) for folk, strings (48) for cinematic, or major keys for optimistic*\n\n✨ MAGIC - DIFFERENT MYSTICAL FLAVORS:\n**Ancient Wisdom** (Pentatonic): [{\"note\": 72, \"velocity\": 45, \"start_time\": 0, \"duration\": 0.4, \"channel\": 0, \"instrument\": 104, \"reverb\": 90}, {\"note\": 77, \"velocity\": 50, \"start_time\": 0.2, \"duration\": 0.4, \"channel\": 1, \"instrument\": 104, \"reverb\": 90}, {\"note\": 79, \"velocity\": 55, \"start_time\": 0.4, \"duration\": 0.5, \"channel\": 2, \"instrument\": 104, \"reverb\": 90}]\n*Sitar with heavy reverb - try shakuhachi (77) for zen, choir (52) for divine, or music box (10) for nostalgic*\n\n🎨 CREATIVE USAGE GUIDE:\n• **DON'T COPY - ADAPT!** These are starting points for your unique scenarios\n• **MATCH THE CONTEXT**: Cooking success? Try pizzicato strings. Coding breakthrough? Electronic sounds\n• **EXPERIMENT WITH KEYS**: Major=happy, minor=mysterious/sad, modal=exotic/ancient\n• **MIX INSTRUMENTS CREATIVELY**: Layer 2-3 that complement the emotional tone\n• **VARY TIMING**: Quick for urgency, slow for contemplation, syncopated for playfulness\n• **USE EFFECTS MEANINGFULLY**: Reverb for space/mystery, chorus for richness/beauty\n• **TELL YOUR STORY**: What musical journey matches your specific situation?\n\n🎵 MUSICAL CREATIVITY TOOLKIT:\n• **Emotional Keys**: C major=pure joy, G major=bright optimism, D major=triumphant, A major=warm confidence\n• **Mysterious Keys**: A minor=melancholy, E minor=introspective, B minor=dark/serious, F# minor=profound\n• **Exotic Scales**: Pentatonic (C-D-E-G-A) for Asian, dorian mode for medieval, blues scale for soulful\n• **Instrument Personalities**: Piano=universal, strings=emotional, brass=bold, woodwinds=expressive, bells=magical\n• **Rhythm Emotions**: Even=stable, dotted=elegant, syncopated=playful, accelerating=building excitement\n\n🤖 **NEW: INLINE R2D2 EXPRESSIONS!**\n\n**Victory Fanfare with R2D2 Celebration:**\n[{\"note\": 60, \"velocity\": 100, \"start_time\": 0, \"duration\": 0.5, \"instrument\": 56}, {\"note\": 64, \"velocity\": 100, \"start_time\": 0.5, \"duration\": 0.5, \"instrument\": 56}, {\"note_type\": \"r2d2\", \"start_time\": 1.2, \"duration\": 1.0, \"r2d2_emotion\": \"Excited\", \"r2d2_intensity\": 0.9, \"r2d2_complexity\": 4}, {\"note\": 72, \"velocity\": 120, \"start_time\": 1.5, \"duration\": 1.5, \"instrument\": 56}]\n\n**Problem-Solving with Thoughtful R2D2:**\n[{\"note_type\": \"r2d2\", \"start_time\": 0, \"duration\": 1.5, \"r2d2_emotion\": \"Thoughtful\", \"r2d2_intensity\": 0.5, \"r2d2_complexity\": 3}, {\"note\": 60, \"velocity\": 70, \"start_time\": 0.5, \"duration\": 1.0, \"instrument\": 0}, {\"note_type\": \"r2d2\", \"start_time\": 2.0, \"duration\": 0.6, \"r2d2_emotion\": \"Surprised\", \"r2d2_intensity\": 0.8, \"r2d2_complexity\": 1}]\n\n**Curious Discovery:**\n[{\"note\": 36, \"velocity\": 60, \"start_time\": 0, \"duration\": 3, \"instrument\": 89, \"reverb\": 80}, {\"note_type\": \"r2d2\", \"start_time\": 1.0, \"duration\": 0.8, \"r2d2_emotion\": \"Curious\", \"r2d2_intensity\": 0.6, \"r2d2_complexity\": 2}, {\"note\": 67, \"velocity\": 90, \"start_time\": 2.5, \"duration\": 0.3, \"instrument\": 73}]\n\n💡 **R2D2 INTEGRATION TIPS:**\n• Set note_type=\"r2d2\" to create robotic expressions\n• r2d2_emotion is REQUIRED for R2D2 notes (Happy, Sad, Excited, Worried, Curious, Affirmative, Negative, Surprised, Thoughtful)\n• r2d2_intensity controls emotional strength (0.0-1.0)\n• r2d2_complexity sets phrase length (1-5 syllables)\n• Mix freely with MIDI notes for rich musical storytelling\n• Perfect timing synchronization between music and R2D2 expressions\n• Use for celebrations, reactions, confirmations, and emotional atmosphere",
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
                                    "description": "🤖 R2D2 emotion when note_type='r2d2': Choose from 9 distinct emotional expressions",
                                    "enum": ["Happy", "Sad", "Excited", "Worried", "Curious", "Affirmative", "Negative", "Surprised", "Thoughtful"]
                                },
                                "r2d2_intensity": {
                                    "type": "number",
                                    "description": "🔥 R2D2 emotional intensity (0.0-1.0): 0.3=subtle, 0.6=moderate, 0.9=dramatic",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "r2d2_complexity": {
                                    "type": "integer",
                                    "description": "🗣️ R2D2 phrase complexity (1-5 syllables): 1=simple beep, 3=conversational, 5=complex phrase",
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
        },
        {
            "name": "play_r2d2_expression",
            "description": "🤖 Express emotions through authentic R2D2-style robotic vocalizations! This advanced synthesizer uses ring modulation, formant filtering, and emotional parameter mapping to create expressive robot sounds that enhance AI conversations:\n\n🎭 EMOTIONAL EXPRESSIONS:\n• Happy: Rising pitch contours with bright harmonics - perfect for celebrating user successes\n• Sad: Falling pitch with reduced harmonics - gentle empathy for disappointments\n• Excited: Rapid modulation and high energy bursts - enthusiasm for discoveries\n• Worried: Tremulous modulation with unstable pitch - concern for problems\n• Curious: Rising question-like intonations - engagement with mysteries\n• Affirmative: Confident, stable pitch patterns - agreement and confirmation\n• Negative: Sharp, decisive rejection patterns - clear disagreement\n• Surprised: Sudden pitch jumps with expanded range - shock and amazement\n• Thoughtful: Slow, contemplative patterns - deep consideration\n\n🔧 TECHNICAL FEATURES:\n• Ring modulation synthesis for authentic robotic character\n• Multi-formant filtering for organic vocal-like qualities\n• Dynamic pitch contours that match emotional states\n• Phrase complexity control (1-5 syllables for varied expressions)\n• Intensity scaling for subtle to dramatic emotional range\n• Real-time parameter modulation for expressive dynamics\n\n💬 CONVERSATION ENHANCEMENT:\n• Use Happy expressions when users solve problems or achieve goals\n• Express Curiosity when presenting questions or exploring topics\n• Show Surprise for unexpected revelations or plot twists\n• Demonstrate Thoughtfulness during complex explanations\n• Provide Affirmative responses for confirmations and agreements\n• Use Worried tones when discussing problems or concerns\n• Express Excitement for breakthroughs and discoveries\n\n🎵 USAGE EXAMPLES:\n**Celebrating Success**: emotion=\"Happy\", intensity=0.8, duration=1.2, phrase_complexity=3\n**Asking Questions**: emotion=\"Curious\", intensity=0.6, duration=0.8, phrase_complexity=2  \n**Showing Concern**: emotion=\"Worried\", intensity=0.7, duration=1.0, phrase_complexity=2\n**Expressing Wonder**: emotion=\"Surprised\", intensity=0.9, duration=0.6, phrase_complexity=1\n\n🌟 ADD PERSONALITY TO EVERY INTERACTION! R2D2-style expressions make AI conversations more engaging, memorable, and emotionally resonant. Use liberally to create a rich, expressive robotic personality that users will love!",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "emotion": {
                        "type": "string",
                        "description": "🎭 R2D2 emotional state",
                        "enum": ["Happy", "Sad", "Excited", "Worried", "Curious", "Affirmative", "Negative", "Surprised", "Thoughtful"]
                    },
                    "intensity": {
                        "type": "number",
                        "description": "🔥 Emotional intensity (0.0-1.0): 0.3=subtle, 0.6=moderate, 0.9=dramatic",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "duration": {
                        "type": "number",
                        "description": "⏱️ Expression duration in seconds: 0.5=quick, 1.0=normal, 2.0=extended",
                        "minimum": 0.1,
                        "maximum": 5.0
                    },
                    "phrase_complexity": {
                        "type": "integer",
                        "description": "🗣️ Number of syllables (1-5): 1=simple beep, 3=conversational, 5=complex phrase",
                        "minimum": 1,
                        "maximum": 5
                    },
                    "pitch_range": {
                        "type": "array",
                        "description": "🎵 Frequency range [min_hz, max_hz]: [200,600]=low, [300,800]=normal, [400,1000]=high",
                        "items": {
                            "type": "number"
                        },
                        "minItems": 2,
                        "maxItems": 2
                    },
                    "context": {
                        "type": "string",
                        "description": "💭 Optional conversation context for enhanced expression adaptation"
                    }
                },
                "required": ["emotion", "intensity", "duration", "phrase_complexity", "pitch_range"]
            }
        },
        {
            "name": "play_expressive_synth",
            "description": "🎨 Create expressive synthesized sounds and music using advanced FunDSP synthesis techniques! Perfect for unique audio experiences beyond traditional MIDI:\n\n🎛️ SYNTHESIS TYPES:\n• Oscillator: sine, square, sawtooth, triangle, noise with advanced shaping\n• FM Synthesis: carrier/modulator with complex timbres and feedback\n• Granular: textured, evolving soundscapes from grain clouds\n• Wavetable: morphing between sonic characters with smooth transitions\n\n🥁 PERCUSSION & DRUMS:\n• Kick: punchy, sustained bass drums with click and body control\n• Snare: snappy, buzzy snare drums with tone and noise balance\n• Hi-hat: metallic, decaying cymbals with brightness control\n• Cymbal: realistic cymbal synthesis with size and material options\n• Custom: design your own percussion sounds with full parameter control\n\n🎭 SOUND EFFECTS:\n• Swoosh: directional movement effects with frequency sweeps\n• Zap: energy bursts and laser sounds with harmonic control\n• Chime: harmonic bell-like tones with inharmonicity\n• Burst: explosive sounds with spectral shaping\n• Custom: create unique sound effects for specific contexts\n\n🌊 AMBIENT & TEXTURES:\n• Pad: warm, evolving harmonic textures with movement\n• Texture: rough, evolving soundscapes with spectral control\n• Drone: sustained tones with complex overtone structures\n• Atmosphere: environmental sounds and ambient textures\n\n✨ ADVANCED FEATURES:\n• Real-time filter sweeps and modulation with multiple LFOs\n• Complex envelope shaping (ADSR+ with curve types)\n• Multiple simultaneous effect chains with routing\n• Spatial positioning and movement in stereo field\n• Granular synthesis with custom grain sources\n• Wavetable morphing with smooth interpolation\n\n🎵 USE CASES:\n• Unique sound design for specific moments and contexts\n• Electronic music elements and synthesized instruments\n• Game-style sound effects and interactive audio\n• Ambient soundscapes and evolving textures\n• Experimental audio experiences and sonic exploration\n• Custom percussion and drum sounds\n• Musical accompaniment with synthesized elements\n\n💡 CREATIVE EXAMPLES:\n• Sci-fi atmosphere with granular textures and filtered noise\n• Retro game sounds with square waves and FM synthesis\n• Ambient meditation music with evolving pads and drones\n• Electronic percussion with custom kick and snare synthesis\n• Sound effects for storytelling and dramatic moments\n• Experimental music with wavetable morphing and complex modulation",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sounds": {
                        "type": "array",
                        "description": "Array of synthesized sounds to play",
                        "items": {
                            "type": "object",
                            "properties": {
                                "synth_type": {
                                    "type": "string",
                                    "enum": [
                                        "sine", "square", "sawtooth", "triangle", "noise",
                                        "fm", "granular", "wavetable",
                                        "kick", "snare", "hihat", "cymbal",
                                        "swoosh", "zap", "chime", "burst",
                                        "pad", "texture", "drone"
                                    ],
                                    "description": "Type of synthesis to use"
                                },
                                "frequency": {
                                    "type": "number",
                                    "description": "Base frequency in Hz (20-20000)",
                                    "minimum": 20,
                                    "maximum": 20000
                                },
                                "duration": {
                                    "type": "number",
                                    "description": "Duration in seconds",
                                    "minimum": 0.1,
                                    "maximum": 30.0
                                },
                                "amplitude": {
                                    "type": "number",
                                    "description": "Amplitude level (0.0-1.0)",
                                    "minimum": 0.0,
                                    "maximum": 1.0
                                },
                                "envelope": {
                                    "type": "object",
                                    "description": "ADSR envelope parameters",
                                    "properties": {
                                        "attack": {"type": "number", "minimum": 0.0, "maximum": 5.0},
                                        "decay": {"type": "number", "minimum": 0.0, "maximum": 5.0},
                                        "sustain": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                                        "release": {"type": "number", "minimum": 0.0, "maximum": 10.0}
                                    }
                                },
                                "filter": {
                                    "type": "object",
                                    "description": "Filter parameters",
                                    "properties": {
                                        "filter_type": {"type": "string", "enum": ["lowpass", "highpass", "bandpass"]},
                                        "cutoff": {"type": "number", "minimum": 20, "maximum": 20000},
                                        "resonance": {"type": "number", "minimum": 0.0, "maximum": 1.0}
                                    }
                                },
                                "effects": {
                                    "type": "array",
                                    "description": "Array of effects to apply",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "effect_type": {"type": "string", "enum": ["reverb", "chorus", "delay"]},
                                            "intensity": {"type": "number", "minimum": 0.0, "maximum": 1.0}
                                        }
                                    }
                                },
                                // Synthesis-specific parameters
                                "pulse_width": {"type": "number", "minimum": 0.1, "maximum": 0.9},
                                "modulator_freq": {"type": "number", "minimum": 0.1, "maximum": 1000},
                                "modulation_index": {"type": "number", "minimum": 0.0, "maximum": 10.0},
                                "grain_size": {"type": "number", "minimum": 0.01, "maximum": 0.5},
                                "texture_roughness": {"type": "number", "minimum": 0.0, "maximum": 1.0}
                            },
                            "required": ["synth_type", "duration"]
                        }
                    }
                },
                "required": ["sounds"]
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
        "play_r2d2_expression" => handle_play_r2d2_expression_tool(tool_params.arguments, id),
        "play_expressive_synth" => handle_play_expressive_synth_tool(tool_params.arguments, id),
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

    for note in &sequence.notes {
        if note.note_type == "r2d2" {
            has_r2d2 = true;
        } else {
            has_midi = true;
        }
    }

    tracing::info!(
        "Sequence analysis: {} notes, has_midi: {}, has_r2d2: {}",
        sequence.notes.len(),
        has_midi,
        has_r2d2
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

    // Choose the appropriate playback method
    let playback_result = if has_midi && has_r2d2 {
        // Mixed sequence - use hybrid audio engine
        tracing::info!("Using mixed mode playback (MIDI + R2D2)");
        player.play_mixed(sequence)
    } else if has_r2d2 {
        // Pure R2D2 sequence - use hybrid engine (with empty MIDI)
        tracing::info!("Using R2D2-only playback via hybrid engine");
        player.play_mixed(sequence)
    } else {
        // Pure MIDI sequence - use traditional MIDI player
        tracing::info!("Using pure MIDI playback");
        player.play_simple(sequence)
    };

    // Handle the result
    match playback_result {
        Ok(()) => {
            let mode_description = if has_midi && has_r2d2 {
                "🎵🤖 Mixed MIDI and R2D2 sequence playback started successfully! The music and robotic expressions are now playing in perfect synchronization."
            } else if has_r2d2 {
                "🤖 R2D2 expression sequence playback started successfully! The robotic vocalizations are now playing."
            } else {
                "🎵 Pure MIDI sequence playback started successfully! The music is now playing."
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

fn handle_play_r2d2_expression_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    // Parse the R2D2 expression parameters
    #[derive(serde::Deserialize)]
    struct R2D2ExpressionArgs {
        emotion: String,
        intensity: f32,
        duration: f32,
        phrase_complexity: u8,
        pitch_range: Vec<f32>,
        context: Option<String>,
    }

    let args: R2D2ExpressionArgs = match serde_json::from_value(arguments) {
        Ok(args) => args,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Failed to parse R2D2 expression arguments: {}", e),
                    data: None,
                }),
            };
        }
    };

    // Validate arguments
    if args.pitch_range.len() != 2 {
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "pitch_range must contain exactly 2 values [min_hz, max_hz]".to_string(),
                data: None,
            }),
        };
    }

    if args.intensity < 0.0 || args.intensity > 1.0 {
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "intensity must be between 0.0 and 1.0".to_string(),
                data: None,
            }),
        };
    }

    // Parse emotion
    let emotion = match args.emotion.as_str() {
        "Happy" => R2D2Emotion::Happy,
        "Sad" => R2D2Emotion::Sad,
        "Excited" => R2D2Emotion::Excited,
        "Worried" => R2D2Emotion::Worried,
        "Curious" => R2D2Emotion::Curious,
        "Affirmative" => R2D2Emotion::Affirmative,
        "Negative" => R2D2Emotion::Negative,
        "Surprised" => R2D2Emotion::Surprised,
        "Thoughtful" => R2D2Emotion::Thoughtful,
        _ => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Unknown emotion: {}", args.emotion),
                    data: None,
                }),
            };
        }
    };

    // Create R2D2 expression
    let expression = R2D2Expression {
        emotion,
        intensity: args.intensity,
        duration: args.duration,
        phrase_complexity: args.phrase_complexity,
        pitch_range: (args.pitch_range[0], args.pitch_range[1]),
        context: args.context,
    };

    // Create expressive synthesizer
    let synth = match ExpressiveSynth::new() {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: format!("Failed to create expressive synthesizer: {}", e),
                    data: None,
                }),
            };
        }
    };

    // Create R2D2 voice generator
    let r2d2_voice = R2D2Voice::new();

    // Generate synthesis parameters
    let synth_params = match r2d2_voice.generate_expression_params(&expression) {
        Some(params) => params,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: "Failed to generate R2D2 expression parameters".to_string(),
                    data: None,
                }),
            };
        }
    };

    // Play the R2D2 expression
    match synth.play_r2d2_expression(
        synth_params.base_freq,
        expression.intensity, // Use the original intensity, not modulation_depth!
        synth_params.pitch_contour,
        synth_params.duration,
    ) {
        Ok(()) => {
            tracing::info!(
                "Successfully started R2D2 expression playback: {:?}",
                expression.emotion
            );
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("🤖 R2D2 {} expression played successfully using FunDSP synthesizer! The robotic vocalization conveyed the emotion with {:.1}% intensity over {:.1} seconds.",
                                expression.emotion,
                                expression.intensity * 100.0,
                                expression.duration
                            )
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
                message: format!("Failed to play R2D2 expression: {}", e),
                data: None,
            }),
        },
    }
}

fn handle_play_expressive_synth_tool(arguments: Value, id: Option<Value>) -> JsonRpcResponse {
    use crate::expressive::synth::{ExpressiveSynth, SynthParams, SynthType, EnvelopeParams, FilterParams, FilterType, EffectParams, EffectType, NoiseColor};
    
    #[derive(Deserialize)]
    struct ExpressiveSynthArgs {
        sounds: Vec<SynthSound>,
    }
    
    #[derive(Deserialize)]
    struct SynthSound {
        synth_type: String,
        frequency: Option<f32>,
        duration: f32,
        amplitude: Option<f32>,
        envelope: Option<EnvelopeSpec>,
        filter: Option<FilterSpec>,
        effects: Option<Vec<EffectSpec>>,
        // Synthesis-specific parameters
        pulse_width: Option<f32>,
        modulator_freq: Option<f32>,
        modulation_index: Option<f32>,
        grain_size: Option<f32>,
        texture_roughness: Option<f32>,
    }
    
    #[derive(Deserialize)]
    struct EnvelopeSpec {
        attack: Option<f32>,
        decay: Option<f32>,
        sustain: Option<f32>,
        release: Option<f32>,
    }
    
    #[derive(Deserialize)]
    struct FilterSpec {
        filter_type: Option<String>,
        cutoff: Option<f32>,
        resonance: Option<f32>,
    }
    
    #[derive(Deserialize)]
    struct EffectSpec {
        effect_type: String,
        intensity: Option<f32>,
    }
    
    match serde_json::from_value::<ExpressiveSynthArgs>(arguments) {
        Ok(args) => {
            match ExpressiveSynth::new() {
                Ok(synth) => {
                    // Convert SynthSound to SynthParams
                    let mut synth_params = Vec::new();
                    
                    for sound in args.sounds {
                        // Parse synthesis type
                        let synth_type = match sound.synth_type.as_str() {
                            "sine" => SynthType::Sine,
                            "square" => SynthType::Square { 
                                pulse_width: sound.pulse_width.unwrap_or(0.5) 
                            },
                            "sawtooth" => SynthType::Sawtooth,
                            "triangle" => SynthType::Triangle,
                            "noise" => SynthType::Noise { color: NoiseColor::White },
                            "fm" => SynthType::FM { 
                                modulator_freq: sound.modulator_freq.unwrap_or(440.0),
                                modulation_index: sound.modulation_index.unwrap_or(1.0)
                            },
                            "granular" => SynthType::Granular {
                                grain_size: sound.grain_size.unwrap_or(0.1),
                                overlap: 0.5,
                                density: 0.8
                            },
                            "wavetable" => SynthType::Wavetable {
                                position: 0.5,
                                morph_speed: 1.0
                            },
                            "kick" => SynthType::Kick {
                                punch: 0.8,
                                sustain: 0.6,
                                click_freq: 1000.0,
                                body_freq: 60.0
                            },
                            "snare" => SynthType::Snare {
                                snap: 0.7,
                                buzz: 0.8,
                                tone_freq: 200.0,
                                noise_amount: 0.6
                            },
                            "hihat" => SynthType::HiHat {
                                metallic: 0.9,
                                decay: 0.1,
                                brightness: 0.8
                            },
                            "cymbal" => SynthType::Cymbal {
                                size: 0.8,
                                metallic: 0.9,
                                strike_intensity: 0.8
                            },
                            "swoosh" => SynthType::Swoosh {
                                direction: 0.0,
                                intensity: 0.7,
                                frequency_sweep: (200.0, 2000.0)
                            },
                            "zap" => SynthType::Zap {
                                energy: 0.8,
                                decay: 0.3,
                                harmonic_content: 0.7
                            },
                            "chime" => SynthType::Chime {
                                fundamental: sound.frequency.unwrap_or(440.0),
                                harmonic_count: 4,
                                decay: 0.8,
                                inharmonicity: 0.1
                            },
                            "burst" => SynthType::Burst {
                                center_freq: sound.frequency.unwrap_or(1000.0),
                                bandwidth: 500.0,
                                intensity: 0.8,
                                shape: 0.5
                            },
                            "pad" => SynthType::Pad {
                                warmth: 0.7,
                                movement: 0.3,
                                space: 0.6,
                                harmonic_evolution: 0.5
                            },
                            "texture" => SynthType::Texture {
                                roughness: sound.texture_roughness.unwrap_or(0.5),
                                evolution: 0.3,
                                spectral_tilt: 0.5,
                                modulation_depth: 0.4
                            },
                            "drone" => SynthType::Drone {
                                fundamental: sound.frequency.unwrap_or(110.0),
                                overtone_spread: 0.3,
                                modulation: 0.2
                            },
                            _ => {
                                return JsonRpcResponse {
                                    jsonrpc: "2.0".to_string(),
                                    id,
                                    result: None,
                                    error: Some(JsonRpcError {
                                        code: -32602,
                                        message: format!("Unknown synthesis type: {}", sound.synth_type),
                                        data: None,
                                    }),
                                };
                            }
                        };
                        
                        // Create envelope
                        let envelope = if let Some(env) = sound.envelope {
                            EnvelopeParams {
                                attack: env.attack.unwrap_or(0.1),
                                decay: env.decay.unwrap_or(0.1),
                                sustain: env.sustain.unwrap_or(0.8),
                                release: env.release.unwrap_or(0.3),
                            }
                        } else {
                            EnvelopeParams {
                                attack: 0.1,
                                decay: 0.1,
                                sustain: 0.8,
                                release: 0.3,
                            }
                        };
                        
                        // Create filter if specified
                        let filter = if let Some(filt) = sound.filter {
                            let filter_type = match filt.filter_type.as_deref().unwrap_or("lowpass") {
                                "lowpass" => FilterType::LowPass,
                                "highpass" => FilterType::HighPass,
                                "bandpass" => FilterType::BandPass,
                                _ => FilterType::LowPass,
                            };
                            Some(FilterParams {
                                cutoff: filt.cutoff.unwrap_or(1000.0),
                                resonance: filt.resonance.unwrap_or(0.5),
                                filter_type,
                            })
                        } else {
                            None
                        };
                        
                        // Create effects
                        let effects = if let Some(fx) = sound.effects {
                            fx.into_iter().map(|effect| {
                                let effect_type = match effect.effect_type.as_str() {
                                    "reverb" => EffectType::Reverb,
                                    "chorus" => EffectType::Chorus,
                                    "delay" => EffectType::Delay { delay_time: 0.2 },
                                    _ => EffectType::Reverb,
                                };
                                EffectParams {
                                    effect_type,
                                    intensity: effect.intensity.unwrap_or(0.5),
                                }
                            }).collect()
                        } else {
                            Vec::new()
                        };
                        
                        let params = SynthParams {
                            synth_type,
                            frequency: sound.frequency.unwrap_or(440.0),
                            amplitude: sound.amplitude.unwrap_or(0.5),
                            duration: sound.duration,
                            envelope,
                            filter,
                            effects,
                        };
                        
                        synth_params.push(params);
                    }
                    
                    // Play the synthesized sequence
                    match synth.play_synthesized_sequence(synth_params.clone()) {
                        Ok(_) => {
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: Some(json!({
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": format!("🎨 Expressive synthesis sequence played successfully! Generated {} synthesized sound(s) with advanced audio processing.", synth_params.len())
                                        }
                                    ]
                                })),
                                error: None,
                            }
                        }
                        Err(e) => {
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32603,
                                    message: format!("Synthesis playback failed: {}", e),
                                    data: None,
                                }),
                            }
                        }
                    }
                }
                Err(e) => {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Failed to create ExpressiveSynth: {}", e),
                            data: None,
                        }),
                    }
                }
            }
        }
        Err(e) => {
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid arguments: {}", e),
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
