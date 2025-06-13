
<div align="center">
  
  [![Watch MCP Muse Demo Video](https://img.youtube.com/vi/0ZWhG9d-SQA/maxresdefault.jpg)](https://youtu.be/0ZWhG9d-SQA)
  
  **🎬 Click the image above to watch the demo video! 🎬**
  
  <br/>
  <br/>
  
  [![Create Issue](https://img.shields.io/badge/Create-GitHub%20Issue-red?style=for-the-badge&logo=github)](https://github.com/alextrzyna/mcp-muse/issues/new)
  
  <br/>
  
  <h2>🎮 Give your AI agent authentic 16-bit SNES gaming sound! 🎮</h2>
  
  <p><strong>Nostalgic retro voice for AI agents - transport users back to the golden age of video games</strong></p>
</div>

## 🎮 The Ultimate Retro Gaming Voice for AI Agents

**MCP-MUSE brings authentic 16-bit SNES sound to your AI conversations!**

### 🏰 **Classic Gaming Vibes**
- **Authentic SNES Sound** - FluidR3_GM captures that beloved 16-bit console tone
- **Nostalgic Game Themes** - Zelda discoveries, Mario power-ups, Final Fantasy victories
- **Interactive Feedback** - Question marks, celebrations, "aha!" moments, and alerts

### 🎵 **Real-Time Musical Reactions**
- **Victory Fanfares** - Celebrate user accomplishments with epic boss-defeat themes
- **8-Bit Sound Effects** - Classic gaming feedback for every interaction
- **Atmospheric Music** - Dungeon ambience, overworld melodies, menu music
- **Instant Responses** - Musical reactions that enhance every conversation

### ✨ **Magical Gaming Moments**
- **Treasure Discovery** - That perfect Zelda-style "you found something!" chime
- **Power-Up Effects** - Ascending arpeggios that sound like gaining abilities
- **Coin Collect** - Satisfying metallic pings for small achievements
- **Dramatic Stings** - Musical punctuation for important revelations

### 🎮 **Easy Integration**
- **Copy-Paste Examples** - Ready-to-use SNES-style compositions
- **Simple JSON** - No complex music theory required
- **Instant Nostalgia** - Every sound transports users to their childhood gaming memories

## Features

- 🎮 **16-Bit SNES Sound**: Authentic retro gaming audio using FluidR3_GM SoundFont
- 🏰 **Classic Game Themes**: Zelda, Mario, Final Fantasy, Metroid-style compositions  
- ⚡ **Instant Feedback**: Musical reactions for celebrations, questions, discoveries
- 🎵 **128 Gaming Instruments**: Square waves, chiptune leads, retro synths, classic drums
- 🔌 **MCP Integration**: Easy AI agent integration with copy-paste examples
- ⚙️ **Zero Setup**: Automatic retro SoundFont download and configuration
- 🎧 **Nostalgic Audio**: Transport users back to the golden age of gaming
- 🧪 **Ready to Play**: Pre-built SNES-style sound effects and musical phrases

## Installation

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Audio output device (speakers/headphones)
- ~150MB disk space for SoundFont

### From Source

```bash
git clone https://github.com/alextrzyna/mcp-muse.git
cd mcp-muse
cargo build --release
```

## Quick Start

### 1. Run Setup

For **Cursor**:
```bash
./target/release/mcp-muse --setup
```

This automatically:
- Downloads FluidR3_GM SoundFont (142MB) from [keymusician01.s3.amazonaws.com](https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip) for authentic SNES sound
- Configures `~/.cursor/mcp.json`
- Sets up retro gaming audio synthesis

### 2. Restart Cursor

Close and reopen Cursor for the MCP server to be available.

### 3. Add Retro Gaming Magic

Ask your AI agent to enhance conversations with nostalgic sound:

```
"Play a celebration sound when I solve this problem"
"Add a Zelda-style discovery sound for important moments"
"Create some atmospheric dungeon music while I work"
"Play a Mario power-up sound when I get the right answer"
```

## 🎮 Classic SNES Gaming Examples

### 🗡️ Zelda-Style Discovery (Treasure Found!)
```json
{
  "notes": [
    {"note": 67, "velocity": 90, "start_time": 0, "duration": 0.3, "channel": 0, "instrument": 73},
    {"note": 72, "velocity": 100, "start_time": 0.3, "duration": 0.3, "channel": 0, "instrument": 73},
    {"note": 76, "velocity": 110, "start_time": 0.6, "duration": 0.3, "channel": 0, "instrument": 73},
    {"note": 79, "velocity": 120, "start_time": 0.9, "duration": 0.6, "channel": 0, "instrument": 73, "reverb": 40}
  ]
}
```

**Perfect for**: Important discoveries, successful completions, "aha!" moments

### 🍄 Mario-Style Power-Up
```json
{
  "notes": [
    {"note": 64, "velocity": 100, "start_time": 0, "duration": 0.1, "channel": 0, "instrument": 80},
    {"note": 67, "velocity": 100, "start_time": 0.1, "duration": 0.1, "channel": 0, "instrument": 80},
    {"note": 72, "velocity": 100, "start_time": 0.2, "duration": 0.1, "channel": 0, "instrument": 80},
    {"note": 76, "velocity": 100, "start_time": 0.3, "duration": 0.1, "channel": 0, "instrument": 80},
    {"note": 79, "velocity": 110, "start_time": 0.4, "duration": 0.3, "channel": 0, "instrument": 80}
  ]
}
```

**Perfect for**: Leveling up, gaining new abilities, getting correct answers

### 🌟 Final Fantasy Victory Fanfare
```json
{
  "notes": [
    {"note": 60, "velocity": 100, "start_time": 0, "duration": 0.5, "channel": 0, "instrument": 56},
    {"note": 64, "velocity": 100, "start_time": 0.5, "duration": 0.5, "channel": 0, "instrument": 56},
    {"note": 67, "velocity": 110, "start_time": 1, "duration": 0.5, "channel": 0, "instrument": 56},
    {"note": 72, "velocity": 120, "start_time": 1.5, "duration": 1, "channel": 0, "instrument": 56},
    {"note": 36, "velocity": 90, "start_time": 0, "duration": 0.25, "channel": 9},
    {"note": 36, "velocity": 90, "start_time": 1, "duration": 0.25, "channel": 9}
  ]
}
```

**Perfect for**: Major accomplishments, completing difficult tasks, celebrating victories

## 🎮 Gaming Sound Parameters

### **Basic Sound Properties**
- **`note`**: 60=C4 (middle C), 72=C5 (high), 48=C3 (low)
- **`velocity`**: 80=normal, 100=strong, 127=maximum impact
- **`start_time`**: 0=immediate, 0.5=half second delay
- **`duration`**: 0.1=quick blip, 0.5=short, 1.0=sustained

### **Gaming Instruments**
- **`channel`**: 0-8=melody instruments, 9=drums, 10-15=effects
- **`instrument`**: 80=Square Lead (classic 8-bit), 73=Flute (Zelda), 56=Trumpet (fanfares)

### **Retro Effects**
- **`volume`**: 90-127=prominent effects, 60-80=background ambience  
- **`reverb`**: 40=dungeon echo, 80=magical sparkle, 127=cathedral
- **`chorus`**: 0=clean retro, 60=lush SNES sound, 100=dreamy

## 🎮 Classic Gaming Instruments

| Range | SNES Style | Gaming Use Cases |
|-------|------------|------------------|
| 0-7 | **Classic Piano** | 0=Soft melodies, menu music |
| 56-63 | **Epic Brass** | 56=Victory fanfares, boss themes |
| 68-79 | **Magical Winds** | 73=Zelda discoveries, fairy sounds |
| 80-87 | **8-Bit Leads** | 80=Mario melodies, chiptune leads |
| 88-95 | **Atmospheric** | 89=Dungeon ambience, mysterious pads |
| 9 | **Sound Effects** | 9=Glockenspiel coins, magical chimes |
| Channel 9 | **Retro Drums** | 36=Kick, 38=Snare, 42=Hi-hat |

## Technical Architecture

### **Retro Gaming Audio Engine**
- **OxiSynth Synthesizer**: Pure Rust SoundFont synthesis for authentic SNES sound
- **FluidR3_GM SoundFont**: 142MB retro gaming instrument collection downloaded from [keymusician01.s3.amazonaws.com](https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip)
- **16-Bit Gaming Audio**: Authentic console-quality synthesis and playback
- **Real-Time Feedback**: Instant musical reactions for AI conversations

### **Gaming-Focused Tools**
- **`play_notes`**: Gaming-optimized JSON interface with copy-paste examples
- **`play_midi`**: Classic MIDI support for authentic retro compositions

### **Tool Schema**

```json
{
  "name": "play_notes",
  "description": "🎮 16-bit SNES gaming sound for AI conversation enhancement...",
  "parameters": {
    "notes": [{
      "note": "🎵 Sound pitch (60=middle, 72=high, 48=low)",
      "velocity": "🔊 Impact strength (80=normal, 127=maximum)",
      "start_time": "⏰ When to play (0=immediate)",
      "duration": "⏳ How long (0.1=blip, 0.5=short, 1.0=sustained)",
      "channel": "🎮 Sound type (0-8=melody, 9=drums)",
      "instrument": "🎹 Gaming instrument (80=8-bit, 73=magical, 56=fanfare)",
      "volume": "🔊 How loud (90-127=effects, 60-80=background)",
      "reverb": "🏰 Echo effect (40=dungeon, 80=magical)",
      "chorus": "✨ SNES shimmer (60=classic, 100=dreamy)"
    }]
  }
}
```

## Development

### Running Tests
```bash
cargo test
```

### Building
```bash
cargo build --release
```

### Development Mode
```bash
cargo run -- --setup  # Setup with SoundFont download
cargo run             # Run MCP server
```

## Troubleshooting

### Audio Issues

**No sound output:**
1. Check system audio settings and volume
2. Verify audio device is not muted
3. Test other applications for audio
4. Check logs for audio device errors

**Poor audio quality:**
- Ensure FluidR3_GM.sf2 is properly downloaded (142MB)
- Verify SoundFont integrity
- Check available disk space

### MCP Connection Issues

**Agent can't find tools:**
1. Restart your MCP host (Cursor)
2. Verify MCP configuration file syntax
3. Check binary path in configuration
4. Examine logs for connection errors

**Setup issues:**
```bash
# Re-run setup to re-download SoundFont
./target/release/mcp-muse --setup

# Check SoundFont exists and size
ls -la assets/FluidR3_GM.sf2  # Should be ~142MB

# Manual download if needed
curl -L https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip -o FluidR3_GM.zip
unzip FluidR3_GM.zip -d assets/
```

## License

MIT License - See LICENSE file for details.

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

---

<div align="center">
  <h3>🎮 Give your AI the voice of classic SNES games! 🎮</h3>
  <p>Transform conversations with nostalgic 16-bit sounds that instantly transport users back to the golden age of gaming.</p>
</div>
