
<div align="center">
  
  <table>
    <tr>
      <td>
        <a href="https://youtu.be/0ZWhG9d-SQA">
          <img src="https://img.youtube.com/vi/0ZWhG9d-SQA/maxresdefault.jpg" alt="Watch MCP Muse Demo Video" width="400">
        </a>
      </td>
      <td width="50"></td>
      <td>
        <a href="https://youtu.be/aPbnR2KNRfs?si=iUDkXY1Jxe8h_mfS&t=39">
          <img src="https://img.youtube.com/vi/aPbnR2KNRfs/hqdefault.jpg" alt="Watch MCP Muse Demo Video" width="400">
        </a>
      </td>
    </tr>
  </table>
  
  **🎬 Click the images above to watch the demo videos! 🎬**
  
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
- 🤖 **R2D2 Expressions**: 9 distinct robotic emotions (Happy, Sad, Excited, Worried, Curious, etc.)
- 🎭 **Mixed Mode**: Combine SNES music with R2D2 reactions in perfect synchronization
- 🏰 **Classic Game Themes**: Zelda, Mario, Final Fantasy, Metroid-style compositions  
- ⚡ **Instant Feedback**: Musical reactions for celebrations, questions, discoveries
- 🎵 **128 Gaming Instruments**: Square waves, chiptune leads, retro synths, classic drums
- 🔌 **MCP Integration**: Easy AI agent integration with copy-paste examples
- ⚙️ **Zero Setup**: Automatic SoundFont download and dual-engine configuration
- 🎧 **Nostalgic Audio**: Transport users back to the golden age of gaming
- 🧪 **Ready to Play**: Pre-built SNES-style effects and R2D2 expressions

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

This **interactively** guides you through:
- **Step 1: SoundFont Configuration**
  - Option to specify a custom SoundFont file location
  - Or use the default FluidR3_GM SoundFont
- **Step 2: SoundFont Download** (if using default)
  - Shows exact download URL: [keymusician01.s3.amazonaws.com](https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip)
  - Displays file size (~130MB) and destination path
  - Asks for user permission before downloading
  - Downloads FluidR3_GM SoundFont (142MB) for authentic SNES sound
- **Step 3: MCP Host Configuration**
  - Shows configuration file location (~/.cursor/mcp.json)
  - Asks for user permission before configuring Cursor
  - Provides manual configuration instructions if skipped

**NEW: Custom SoundFont Support**
- Bring your own SoundFont (.sf2 file) for personalized audio
- Configuration saved to platform-specific data directory:
  - **Linux**: `~/.local/share/mcp-muse/config.json`
  - **macOS**: `~/Library/Application Support/mcp-muse/config.json`
  - **Windows**: `%APPDATA%\mcp-muse\config.json`
- Automatically used by the audio engine when configured

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

## 🤖 Mixed Mode Examples (SNES + R2D2)

### 🎺 Victory Fanfare with R2D2 Celebration
```json
{
  "notes": [
    {"note": 60, "velocity": 100, "start_time": 0, "duration": 0.5, "instrument": 56, "note_type": "midi"},
    {"note": 64, "velocity": 100, "start_time": 0.5, "duration": 0.5, "instrument": 56, "note_type": "midi"},
    {"note": 67, "velocity": 110, "start_time": 1, "duration": 0.5, "instrument": 56, "note_type": "midi"},
    {"note_type": "r2d2", "start_time": 1.2, "duration": 1.0, "r2d2_emotion": "Excited", "r2d2_intensity": 0.9, "r2d2_complexity": 4, "r2d2_pitch_range": [400, 1000]},
    {"note": 72, "velocity": 120, "start_time": 1.5, "duration": 1.5, "instrument": 56, "note_type": "midi"}
  ]
}
```

**Perfect for**: User accomplishments with AI celebrating alongside

### 🔍 Curious Discovery
```json
{
  "notes": [
    {"note": 36, "velocity": 60, "start_time": 0, "duration": 3, "instrument": 89, "reverb": 80, "note_type": "midi"},
    {"note_type": "r2d2", "start_time": 1.0, "duration": 0.8, "r2d2_emotion": "Curious", "r2d2_intensity": 0.6, "r2d2_complexity": 2, "r2d2_pitch_range": [250, 600]},
    {"note": 67, "velocity": 90, "start_time": 2.5, "duration": 0.3, "instrument": 73, "note_type": "midi"}
  ]
}
```

**Perfect for**: Exploring ideas with inquisitive AI companion

### 🧠 Problem-Solving Journey
```json
{
  "notes": [
    {"note_type": "r2d2", "start_time": 0, "duration": 1.5, "r2d2_emotion": "Thoughtful", "r2d2_intensity": 0.5, "r2d2_complexity": 3, "r2d2_pitch_range": [150, 400]},
    {"note": 60, "velocity": 70, "start_time": 0.5, "duration": 1.0, "instrument": 0, "note_type": "midi"},
    {"note_type": "r2d2", "start_time": 2.0, "duration": 0.6, "r2d2_emotion": "Surprised", "r2d2_intensity": 0.8, "r2d2_complexity": 1, "r2d2_pitch_range": [300, 800]}
  ]
}
```

**Perfect for**: AI thinking through problems and having breakthroughs

## 🤖 R2D2 Emotional Range

### **Positive Emotions**
- **Happy**: Cheerful warbling with musical frequencies `"r2d2_emotion": "Happy"`
- **Excited**: High-energy rapid beeps `"r2d2_emotion": "Excited"`
- **Affirmative**: Confident confirmations `"r2d2_emotion": "Affirmative"`

### **Interactive Emotions**  
- **Curious**: Rising question tones `"r2d2_emotion": "Curious"`
- **Surprised**: Dramatic upward sweeps `"r2d2_emotion": "Surprised"`
- **Thoughtful**: Deep contemplative sounds `"r2d2_emotion": "Thoughtful"`

### **Concern Emotions**
- **Sad**: Gentle descending whimpers `"r2d2_emotion": "Sad"`
- **Worried**: Nervous trembling patterns `"r2d2_emotion": "Worried"`
- **Negative**: Sharp disapproval tones `"r2d2_emotion": "Negative"`

### **R2D2 Parameters**
- **`r2d2_emotion`**: Choose from 9 distinct emotions (required for R2D2 notes)
- **`r2d2_intensity`**: 0.0-1.0 emotional strength (0.5=moderate, 0.9=dramatic)
- **`r2d2_complexity`**: 1-5 syllables (1=simple beep, 5=complex phrase)
- **`r2d2_pitch_range`**: [min_hz, max_hz] frequency range ([200,600]=low, [400,1000]=high)
- **`note_type`**: Set to "r2d2" for robotic expressions, "midi" for musical notes

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

### **Dual-Engine Audio System**
- **OxiSynth Engine**: Pure Rust SoundFont synthesis for authentic SNES sound
- **ExpressiveSynth Engine**: Ring modulation synthesis for R2D2-style vocalizations
- **FluidR3_GM SoundFont**: 142MB retro gaming instrument collection downloaded from [keymusician01.s3.amazonaws.com](https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip)
- **HybridAudioSource**: Real-time mixing of both engines with sample-accurate timing
- **16-Bit Gaming Audio**: Authentic console-quality synthesis and playback
- **R2D2 Character**: Ring modulation with emotion-specific pitch contours
- **Real-Time Feedback**: Instant musical reactions and robotic expressions for AI conversations

### **AI Conversation Tools**
- **`play_notes`**: Mixed mode JSON interface supporting both MIDI and R2D2 in single sequences
- **`play_r2d2_expression`**: Dedicated robotic expression tool with 9 distinct emotions
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

**Custom SoundFont issues:**
```bash
# Check configuration file
# Linux:
cat ~/.local/share/mcp-muse/config.json
# macOS:
cat ~/Library/Application\ Support/mcp-muse/config.json
# Windows (Command Prompt):
type %APPDATA%\mcp-muse\config.json
# Windows (PowerShell):
Get-Content $env:APPDATA\mcp-muse\config.json

# Verify custom SoundFont exists and is valid
ls -la /path/to/your/soundfont.sf2

# Reset to default SoundFont (removes config file)
# Linux:
rm ~/.local/share/mcp-muse/config.json
# macOS:
rm ~/Library/Application\ Support/mcp-muse/config.json
# Windows:
del %APPDATA%\mcp-muse\config.json
./target/release/mcp-muse --setup
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

### 🤖 **R2D2 Expressive Emotions**
- **Robotic Personality** - 9 distinct emotional expressions add character to AI conversations
- **Authentic R2D2 Sound** - Ring modulation synthesis creates genuine robotic vocalizations
- **Perfect Timing** - Expressive sounds synchronized perfectly with musical moments
- **Mixed Mode** - Combine SNES music with R2D2 reactions in single sequences

### 🎭 **Rich Musical Storytelling**
- **Victory + Celebration** - MIDI fanfare with excited R2D2 cheering
- **Discovery + Curiosity** - Mysterious pads with inquisitive R2D2 sounds
- **Problem Solving** - Thoughtful R2D2 contemplation followed by surprised realization
- **Emotional Enhancement** - Every AI interaction enriched with expressive character

### 🎵 **Dual-Engine Architecture**
- **SNES Gaming Sounds** - Classic 16-bit music and sound effects
- **R2D2 Expressions** - Authentic robotic vocalizations with 9 emotions
- **Mixed Sequences** - Both engines working together in perfect synchronization
- **AI Conversation Focus** - Purpose-built for enhancing AI interactions
