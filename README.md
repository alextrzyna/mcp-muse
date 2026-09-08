
<div align="center">
  
  <table>
    <tr>
      <td align="center">
        <a href="https://youtu.be/0ZWhG9d-SQA">
          <img src="https://img.youtube.com/vi/0ZWhG9d-SQA/maxresdefault.jpg" alt="Watch MCP Muse Demo Video" width="300">
        </a>
        <br>
        <strong>MCP Muse Intro</strong>
      </td>
      <td width="30"></td>
      <td align="center">
        <a href="https://youtu.be/aPbnR2KNRfs?si=iUDkXY1Jxe8h_mfS&t=39">
          <img src="https://img.youtube.com/vi/aPbnR2KNRfs/hqdefault.jpg" alt="Watch MCP Muse Demo Video" width="300">
        </a>
        <br>
        <strong>MCP Muse Makes Grunge and Metal</strong>
      </td>
      <td width="30"></td>
      <td align="center">
        <a href="https://youtu.be/sG9LZWkvGNA">
          <img src="https://img.youtube.com/vi/sG9LZWkvGNA/maxresdefault.jpg" alt="Watch MCP Muse Demo Video" width="300">
        </a>
        <br>
        <strong>MCP Muse Makes RPG Music In Realtime</strong>
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

## 🎮🤖🎛️ The Ultimate Universal Audio Engine for AI Agents

**MCP-MUSE brings comprehensive audio capabilities to your AI conversations - from nostalgic retro gaming sounds to expressive robotic vocalizations to professional music synthesis!**

### 🎵 **Universal Audio Capabilities (All Tested & Confirmed)**
- **🎮 Authentic SNES Gaming Sounds** - 128 GM instruments with FluidR3_GM for classic 16-bit console tone
- **🤖 R2D2 Expressive Emotions** - 9 distinct robotic vocalizations (Happy, Excited, Curious, Worried, etc.)
- **🎹 Built-in Synth Patches** - vintage-style recreations across bass, pad, lead and drums (Minimoog Bass, TB-303 Acid, Jupiter Pads, TR-808 Drums, etc.)
- **🎛️ Agent-Defined Synth Patches** - `define_synth` builds a subtractive, fm, wavetable, granular or percussion patch with its own effects chain, or pass one inline on a note

### 🏆 **Comprehensive Audio Features**
- **Mixed Mode Magic** - All 4 audio systems work together in perfect synchronization
- **Huge Sound Vocabulary** - 128 GM instruments, 9 R2D2 emotions and 44 built-in synth patches, plus any patch you define
- **Real-Time Processing** - Instant musical reactions without conflicts or delays
- **Professional Quality** - Research-driven algorithms for authentic sound reproduction

### 🎯 **Perfect for Every AI Interaction**
- **Victory Celebrations** - MIDI fanfares + R2D2 excited expressions + synthesized flourishes
- **Thoughtful Moments** - Ambient pads + contemplative R2D2 sounds + atmospheric textures
- **Discovery & Learning** - Classic Zelda-style chimes + curious R2D2 + synthesized sparkles
- **Creative Projects** - Professional bass lines + vintage drum machines + custom sound effects

### ✨ **Tested & Production-Ready**
- **10-Scenario Test Suite** - Comprehensive validation of all audio systems
- **Live Verified** - All features confirmed working through AI interface
- **Zero Latency Issues** - Perfect real-time performance across all audio types
- **Instant Integration** - Copy-paste examples for immediate use

## Features ✅ **All Tested & Confirmed Working**

- 🎮 **16-Bit SNES Sound**: Authentic retro gaming audio using FluidR3_GM SoundFont
- 🤖 **R2D2 Expressions**: 9 distinct robotic emotions with ring modulation synthesis
- 🎹 **Built-in Synth Patches**: vintage-style recreations (Minimoog, TB-303, Jupiter-8, TR-808, TR-909, etc.)
- 🎛️ **Agent-Defined Synth Patches**: `define_synth` stores a validated subtractive, fm, wavetable, granular or percussion patch with its own effects chain; notes reference it by name via `synth`
- 🎭 **Universal Mixed Mode**: All audio systems work together in perfect synchronization
- 🏆 **Huge Sound Vocabulary**: 128 GM instruments + 9 R2D2 emotions + 44 built-in synth patches + your own
- ⚡ **Real-Time Performance**: Zero latency issues, perfect timing across all audio types
- 🔌 **Seven Focused Tools**: `play_notes`, `define_sequence_pattern`, `play_sequence`, `list_patterns`, `define_synth`, `list_sounds`, `stop_playback`
- 🎚️ **Stateful Effects**: reverb, delay, chorus, filter, compressor and distortion rendered per patch (shared tails), per R2D2 note and per MIDI bus, stereo output
- ⚙️ **Zero Setup**: Automatic SoundFont download and multi-engine configuration
- 🧪 **Production Validated**: Comprehensive 10-scenario test suite confirms all functionality

## Installation

### Method 1: Install from crates.io (Recommended)

The easiest way to install MCP-Muse:

```bash
cargo install mcp-muse
```

This will download and install the latest stable release. Then proceed to [Quick Start](#quick-start) below.

### Method 2: Download Pre-built Binary

Download the latest binary for your platform from [GitHub Releases](https://github.com/alextrzyna/mcp-muse/releases):

- **Linux**: `mcp-muse-linux-x86_64.tar.gz`
- **macOS (Intel)**: `mcp-muse-macos-x86_64.tar.gz`
- **macOS (Apple Silicon)**: `mcp-muse-macos-aarch64.tar.gz`
- **Windows**: `mcp-muse-windows-x86_64.zip`

Extract the archive and add the binary to your PATH, or specify the full path in your MCP configuration.

### Method 3: Build from Source

For development or latest features:

```bash
git clone https://github.com/alextrzyna/mcp-muse.git
cd mcp-muse
cargo build --release
```

The binary will be available at `./target/release/mcp-muse`.

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/)) - only needed for cargo install or building from source
- Audio output device (speakers/headphones)
- ~150MB disk space for SoundFont (auto-downloaded during setup)

## Quick Start

### 1. Run Setup

For **Cursor** (choose based on your installation method):

```bash
# If installed via cargo:
mcp-muse setup

# If built from source:
./target/release/mcp-muse setup

# If using downloaded binary:
/path/to/mcp-muse setup
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

Ask your AI agent to enhance conversations with comprehensive audio:

```
"Play a celebration sound when I solve this problem"
"Add a Zelda-style discovery sound for important moments"  
"Create some atmospheric dungeon music while I work"
"Play a Mario power-up sound when I get the right answer"
"Use R2D2 excited sounds for breakthroughs"
"Play vintage Minimoog bass for that classic synth feel"
"Add professional drum sounds for rhythm"
"Create custom sound effects for unique moments"
```

## 🧪 **Comprehensive Testing Results - 100% Success!**

The system has been thoroughly validated through a comprehensive 10-scenario test suite covering all audio capabilities:

**✅ All Audio Systems Confirmed Working:**
- **MIDI Instruments** - Piano, Trumpet, Flute sequences
- **R2D2 Expressions** - Happy, Excited, Curious emotions  
- **Built-in Synth Patches** - Minimoog Bass, TB-303 Acid, Jupiter Pads
- **Agent-Defined Synth Patches** - custom subtractive and percussion patches, Zap Effects, Professional Drums
- **Mixed Combinations** - All systems working together in perfect sync
- **Inline Patches** - one-off synth patches passed directly on a note
- **Professional Quality** - Authentic vintage sound reproductions

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

## 🎛️ Synth Patches (agent-defined instruments)

Every synthesized sound is a **patch**: a JSON object with one or more engines, envelopes and an effects chain. Use a built-in patch by name, define your own once with `define_synth`, or pass a patch inline on a note. All notes of a patch in one call share its effects, so reverb and delay tails are real.

### Built-in patch by name
```json
{"notes": [
  {"synth": "minimoog_bass", "note": 36, "velocity": 120, "start_time": 0, "duration": 1},
  {"synth": "minimoog_bass", "note": 43, "velocity": 100, "start_time": 1, "duration": 1},
  {"synth": "tr_808_kick", "start_time": 0, "duration": 0.5},
  {"synth": "tr_909_snare", "start_time": 0.5, "duration": 0.3}
]}
```
Call `list_sounds` with `{"section": "synths"}` for the full list (bass, pad, lead, drums, fx).

### Define your own
```json
{"name": "rubber_bass", "category": "bass",
 "subtractive": {
   "osc1": {"wave": "saw"},
   "osc2": {"wave": "square", "mix": 0.3, "detune_cents": 6},
   "filter": {"type": "low_pass", "cutoff": 500, "resonance": 0.4, "slope": 24,
              "env_amount": 0.7, "env": {"attack": 0.005, "decay": 0.25, "sustain": 0.1, "release": 0.2}},
   "env": {"attack": 0.005, "decay": 0.3, "sustain": 0.6, "release": 0.15}},
 "effects": [{"type": "distortion", "drive": 3, "intensity": 0.4},
             {"type": "compressor", "threshold": -18, "ratio": 4, "intensity": 1}]}
```
Then `{"notes": [{"synth": "rubber_bass", "note": 36, "duration": 0.5}]}`.

An FM example:
```json
{"name": "glass_bell", "category": "keys",
 "fm": {"algorithm": "stack",
        "operators": [{"ratio": 1, "env": {"attack": 0.002, "decay": 1.5, "sustain": 0.2, "release": 2.5}},
                       {"ratio": 3.5, "level": 0.55, "env": {"decay": 0.6, "sustain": 0}}]},
 "effects": [{"type": "reverb", "room_size": 0.7, "intensity": 0.35}]}
```
`list_sounds` now also lists `keys` patches, including `dx7_e_piano`, `fm_bell`, `wt_organ`, `wt_glass_keys` and `shimmer_keys`.

A granular example, with an LFO breathing the grain density:
```json
{"name": "cloud", "category": "pad", "level": 0.6,
 "granular": {"source": "formant", "grain_ms": 120, "density": 15, "pitch_semitones": 7,
              "randomness": 0.7, "stereo_width": 0.9, "env": {"attack": 1.5, "release": 3}},
 "lfo": {"rate": 0.2, "depth": 0.4, "target": "grain_density"},
 "effects": [{"type": "reverb", "room_size": 0.9, "intensity": 0.5}]}
```

A shimmer delay example, where each repeat climbs a fifth then an octave (Time Fracture):
```json
{"name": "shimmer", "category": "keys",
 "fm": {"algorithm": "stack",
        "operators": [{"ratio": 1, "env": {"decay": 1.5, "sustain": 0.2, "release": 2}},
                       {"ratio": 3.5, "level": 0.5, "env": {"decay": 0.5, "sustain": 0}}]},
 "effects": [{"type": "delay", "random_beats": [0.75, 0.75], "pitch_intervals": [7, 12], "pitch_mode": "up",
              "feedback": 0.5, "intensity": 0.6},
             {"type": "reverb", "room_size": 0.8, "intensity": 0.4}]}
```
The built-in `shimmer_keys` patch does the same thing without `define_synth`.

### Engines
- **subtractive**: `osc1`/`osc2` (`sine|saw|square|triangle|noise`, `pulse_width`, osc2 `mix`, `detune_cents`, `octave`), `filter` (`low_pass|high_pass|band_pass`, `cutoff`, `resonance` 0-1, `slope` 12|24, `env_amount` -1..1 with its own `env`), amplitude `env`.
- **fm**: four operators (`ratio` 0.25-16, `level`, `detune_cents`, `env`) routed by `algorithm` `stack|pairs|fan_in|parallel`, plus `feedback`. Operator 1 is always a carrier; a modulator's level is its depth.
- **wavetable**: `table` `basic|warm|bright|digital|vocal|pwm|organ|noise`, `morph` 0-1 toward the next table, `env`. Tables are band-limited per octave.
- **granular**: `source` `harmonics|noise|formant|inharmonic`, `grain_ms` 5-500, `density` 1-50 grains/s, `pitch_semitones` ±24, `randomness`, `stereo_width`, `env`. True stereo.
- **lfo** (not an engine, one per patch): `rate` 0.1-20 Hz, `depth` 0-1, `wave` `sine|triangle|saw|square|sample_hold`, `target` `off|cutoff|pitch|amplitude|morph|grain_density`.
- **percussion**: `kind` `kick|snare|hihat|cymbal|zap|swoosh|chime|burst` with that kind's parameters (`punch`, `snap`, `metallic`, `sweep`, ...) and a `frequency`. Ignores the note's pitch.
- **effects**: ordered chain of `reverb`, `delay`, `chorus`, `filter`, `compressor`, `distortion`, each with an `intensity` 0-1. A synth note takes its effects from here, so `effects`/`effects_preset` on the note itself is rejected. The `delay` also does Time Fracture: `random_beats: [min, max]` in beats of the sequence tempo, `random_rate` Hz, `pitch_intervals` in semitones with `pitch_mode` `random|up|down|up_down`; `sync_tempo: true` puts `delay_time` in beats.

### Migrating from presets and `synth_*` fields

The `preset_*` and `synth_*` note fields are gone; every synthesized sound
is now a patch behind the single `synth` field. Removed fields fail with
`-32602` naming the field, so nothing is silently ignored.

| Before | Now |
|--------|-----|
| `{"preset_name": "Minimoog Bass"}` | `{"synth": "minimoog_bass"}` |
| `{"synth_type": "kick", "synth_frequency": 60}` | `{"synth": "tr_808_kick"}`, or `{"synth": {"name": "kick", "percussion": {"kind": "kick", "frequency": 60}}}` |
| `{"synth_type": "sawtooth", "synth_attack": 0.01, "synth_decay": 0.2, "synth_sustain": 0.6, "synth_release": 0.3, "synth_cutoff": 800}` | `{"synth": {"name": "saw_lead", "subtractive": {"osc1": {"wave": "saw"}, "filter": {"type": "low_pass", "cutoff": 800}, "env": {"attack": 0.01, "decay": 0.2, "sustain": 0.6, "release": 0.3}}}}` |

Preset names map to the built-in patch of the same name in snake_case
(`"TB-303 Acid"` → `tb_303_acid`, `"JP-8 Strings"` → `jp_8_strings`); call
`list_sounds` with `{"section": "synths"}` for the current list.

## 🎭 **Universal Mixed Mode Examples (All Systems Together)**

### 🏆 Ultimate Victory Celebration (MIDI + R2D2 + Synth Patches)
```json
{
  "notes": [
    {"synth": "jp_8_strings", "note": 60, "velocity": 70, "start_time": 0, "duration": 4},
    {"note": 72, "velocity": 100, "start_time": 1, "duration": 1, "instrument": 56, "reverb": 60},
    {"note_type": "r2d2", "r2d2_emotion": "Excited", "r2d2_intensity": 0.9, "start_time": 2.5, "duration": 1},
    {"synth": "chime", "start_time": 3.5, "duration": 0.5}
  ]
}
```

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

### **Retro Effects (Legacy MIDI Parameters)**
- **`volume`**: 90-127=prominent effects, 60-80=background ambience  
- **`reverb`**: 40=dungeon echo, 80=magical sparkle, 127=cathedral (MIDI CC 91)
- **`chorus`**: 0=clean retro, 60=lush SNES sound, 100=dreamy (MIDI CC 93)

### **Professional Audio Effects System** 🎛️
The system includes a comprehensive effects processor. Effects are stateful: one instance per synth patch render (shared across every note of that patch in the call, so tails are real), one per R2D2 note, and one per MIDI bus for the call:

#### **Available Effects**
- **Reverb**: Professional Schroeder algorithm with room size, dampening, wet level, and pre-delay
- **Delay**: Analog-style with feedback, damping, and tempo sync options
- **Chorus**: Multi-tap modulation for rich, lush sounds
- **Filter**: State-variable filter with 7 types (LowPass, HighPass, BandPass, Notch, Peak, LowShelf, HighShelf)
- **Compressor**: Professional dynamics with threshold, ratio, attack, and release
- **Distortion**: Musical saturation with pre/post filtering

#### **Effects Configuration**
```json
{
  "notes": [{
    "note": 60,
    "instrument": 1,
    "effects": [{
      "effect": {
        "Reverb": {
          "room_size": 0.7,
          "dampening": 0.5,
          "wet_level": 0.3,
          "pre_delay": 0.02
        }
      },
      "intensity": 0.8,
      "enabled": true
    }]
  }]
}
```

#### **Effects Presets**
Use `effects_preset` on a MIDI or R2D2 note for quick professional-quality
effects. The 14 names (also listed by `list_sounds` section `effects`) are:
`studio`, `concert_hall`, `live_stage`, `tight_mix`, `ambient`, `dreamy`,
`spacious`, `vintage`, `analog_warmth`, `retro_echo`, `psychedelic`,
`distorted`, `filtered`, `lush_chorus`.

#### **Important Effects Notes**
- **Where effects live**: MIDI notes share one bus chain per call (the first MIDI note that specifies `effects` defines it); R2D2 notes render their own chain into their buffer; a synth note takes its chain from its patch, so `effects`/`effects_preset` on a synth note is rejected with `-32602`
- **Stateful, not per-call limited**: no effect-count cap and no automatic gain compensation
- **All effects use the unified `play_notes` tool** - no separate playback methods needed

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

## Technical Architecture ✅ **All Systems Tested & Operational**

### **Universal Audio Engine**
- **🎮 OxiSynth Engine**: Pure Rust SoundFont synthesis for authentic SNES gaming sounds (✅ **Tested**)
- **🤖 ExpressiveSynth Engine**: Ring modulation synthesis for R2D2-style vocalizations (✅ **Tested**)  
- **🎹 Built-in Synth Patches**: vintage-style synthesizer recreations, embedded as JSON and loaded by `PatchLibrary` (✅ **Tested**)
- **🎛️ Agent-Defined Synth Patches**: subtractive, fm, wavetable, granular and percussion engines with band-limited oscillators, a state-variable filter, an optional per-patch LFO and a stateful effects chain, rendered by `render_patch` (✅ **Tested**)
- **🔄 MidiEngine**: One long-lived stereo mixer with stateful effects chains and a soft clipper (✅ **Tested**)
- **💾 FluidR3_GM SoundFont**: 142MB retro gaming instrument collection from [keymusician01.s3.amazonaws.com](https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip)

### **Comprehensive Audio Capabilities**
- **Huge Sound Vocabulary**: 128 GM instruments + 9 R2D2 emotions + 44 built-in synth patches, plus unlimited agent-defined patches
- **Mixed Mode Magic**: All audio systems work together in perfect synchronization  
- **Professional Quality**: Research-driven algorithms for authentic sound reproduction
- **Real-Time Performance**: Zero latency issues, instant musical reactions
- **Production Validated**: Comprehensive 10-scenario test suite confirms all functionality

### **Tools**
- **`play_notes`**: Universal JSON interface supporting all audio systems in single sequences
- **`define_sequence_pattern` / `play_sequence` / `list_patterns`**: reusable bar-based patterns with transposition, repeats and time signature
- **`define_synth`**: store a validated synth patch for the session; notes reference it by name via `synth`
- **`list_sounds`**: catalog of synth patches, GM instruments, drum keys, R2D2 emotions and effects
- **`stop_playback`**: silence everything currently playing
- Both play tools take `"mode": "replace"` (default: stop what is playing first) or `"mode": "layer"` (mix on top). One synthesizer serves the whole session, so overlapping calls do not multiply CPU or memory.

Playback tools return immediately with the expected duration. Failures the agent can act on (unknown synth or pattern name) come back as `isError` results.

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
cargo run -- setup    # Setup with SoundFont download
cargo run             # Run MCP server
```

### Versioning

This project uses [CalVer](https://calver.org/) versioning in the format `YYYY.MM.PATCH` (e.g., `2025.11.0`). Releases are automatically created when changes are merged to the main branch. See [VERSIONING.md](VERSIONING.md) for details.

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
# Re-run setup to re-download SoundFont (adjust command based on installation method)
mcp-muse setup                        # If installed via cargo
./target/release/mcp-muse setup       # If built from source

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
./target/release/mcp-muse setup
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
