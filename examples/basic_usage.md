# MCP MIDI Usage Examples

This document provides practical examples of using the `mcp-muse` server with various AI agents.

## Example 1: Simple Single Note

The most basic MIDI example - playing a single C4 note for 1 second.

### Base64 MIDI Data
```
TVRoZAAAAAYAAAABAGBNVHJrAAAADgCQPGQwgDwAAP8vAA==
```

### Tool Call
```json
{
  "tool": "play_midi",
  "arguments": {
    "midi_data": "TVRoZAAAAAYAAAABAGBNVHJrAAAADgCQPGQwgDwAAP8vAA=="
  }
}
```

### What You'll Hear
A single C4 note (middle C) played for approximately 1 second.

## Example 2: C Major Scale

A ascending C major scale: C-D-E-F-G-A-B-C.

### Base64 MIDI Data
```
TVRoZAAAAAYAAAABAF9NVHJrAAAATgAAAACQPEBAUAAAAACAP0BAkD5AQFAAAACAPkBAkEBAQFAAAACAQEBAkEJAQFAAAACCQkBAkERAQFAAAACERkBAkERAQFAAAACEREBAlEdAQFAAAACEREBAkEdAQFAAAAAAAP8vAA==
```

### Tool Call
```json
{
  "tool": "play_midi",
  "arguments": {
    "midi_data": "TVRoZAAAAAYAAAABAF9NVHJrAAAATgAAAACQPEBAUAAAAACAP0BAkD5AQFAAAACAPkBAkEBAQFAAAACAQEBAkEJAQFAAAACCQkBAkERAQFAAAACERkBAkERAQFAAAACEREBAlEdAQFAAAACEREBAkEdAQFAAAAAAAP8vAA=="
  }
}
```

### What You'll Hear
Each note of the C major scale played in sequence, with each note lasting about 500ms.

## Example 3: Simple Chord

A C major chord (C-E-G) played simultaneously.

### Base64 MIDI Data
```
TVRoZAAAAAYAAAABAGBNVHJrAAAAIgCQPGQggDwAAJBAZCCAQAAAkENkIIBDAAD/LwA=
```

### Tool Call
```json
{
  "tool": "play_midi",
  "arguments": {
    "midi_data": "TVRoZAAAAAYAAAABAGBNVHJrAAAAIgCQPGQggDwAAJBAZCCAQAAAkENkIIBDAAD/LwA="
  }
}
```

### What You'll Hear
Three notes played simultaneously forming a harmonious C major chord.

## Example 4: Happy Birthday Melody

The opening notes of "Happy Birthday to You".

### Base64 MIDI Data
```
TVRoZAAAAAYAAAABAF9NVHJrAAAAWgAAAACQPEBAUAAAAACAPEBAkDxAQFAAAACAPEBAkD5AQFAAAACAPkBAkEBAQGAAAACAQEBAkD5AQFAAAACAPkBAkDxAQFAAAACAPEBAkDpAQFAAAACAOkBA/y8A
```

### Tool Call
```json
{
  "tool": "play_midi",
  "arguments": {
    "midi_data": "TVRoZAAAAAYAAAABAF9NVHJrAAAAWgAAAACQPEBAUAAAAACAPEBAkDxAQFAAAACAPEBAkD5AQFAAAACAPkBAkEBAQGAAAACAQEBAkD5AQFAAAACAPkBAkDxAQFAAAACAPEBAkDpAQFAAAACAOkBA/y8A"
  }
}
```

### What You'll Hear
The recognizable opening melody of "Happy Birthday" (Hap-py Birth-day to...).

## Using with AI Agents

### In Cursor

1. Make sure mcp-muse is set up: `./target/release/mcp-muse --setup`
2. Restart Cursor
3. In a new chat, ask:

```
"Can you play me a C major scale using the play_midi tool?"
```

The AI will use the tool to generate and play the scale.

### Creative Prompts

Try these prompts with your AI:

- "Play a simple lullaby melody"
- "Create a chord progression in the key of G major"
- "Play the opening of Beethoven's 5th Symphony"
- "Generate a 4-beat drum pattern" (note: will be pitched percussion)
- "Play a blues scale in E minor"
- "Create a simple waltz melody"

## MIDI Technical Notes

### Note Numbers
- C4 (Middle C) = 60
- Each octave adds/subtracts 12
- C3 = 48, C5 = 72

### Velocity (Volume)
- Range: 0-127
- 64 = medium volume
- 127 = maximum volume
- 0 = note off

### Timing
- 480 ticks per quarter note (typical)
- Quarter note at 120 BPM = 500ms
- Adjust delta times between events for rhythm

## Creating Custom MIDI

If you want to create your own MIDI files:

1. Use a DAW (Digital Audio Workstation) like GarageBand, Reaper, or Logic
2. Export as Type 0 MIDI file
3. Convert to base64:

```bash
base64 -i your_file.mid
```

4. Use the base64 output with the `play_midi` tool

## Troubleshooting Examples

### No Sound
If examples don't produce sound:
1. Check system volume
2. Test with Example 1 (single note)
3. Check the logs:
   - **Linux**: `~/.local/share/mcp-muse/mcp-muse.log`
   - **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log`
   - **Windows**: `%APPDATA%\mcp-muse\mcp-muse.log`

### Invalid MIDI Error
- Ensure base64 data is not corrupted
- Try Example 1 to verify the tool works
- Check that the MIDI file is Type 0 format

### Performance Issues
- Keep MIDI files under 1MB for best performance
- Use shorter sequences for real-time interaction
- Consider reducing polyphony (simultaneous notes)

## Sequence Patterns Examples

**New in this release!** The sequence patterns feature allows you to create reusable musical patterns and use them throughout your compositions with transformations.

See [SEQUENCE_PATTERNS.md](../dev-plans/SEQUENCE_PATTERNS.md) for complete documentation.

### Example 1: Creating a Drum Pattern

Define a simple house beat pattern:

```json
{
  "tool": "define_sequence_pattern",
  "arguments": {
    "name": "house_beat",
    "description": "Classic 4/4 house drum pattern",
    "category": "drums",
    "tempo": 128,
    "pattern_bars": 1.0,
    "notes": [
      {"note": 36, "velocity": 120, "start_time": 0.0, "duration": 0.1, "channel": 9},
      {"note": 42, "velocity": 80, "start_time": 0.25, "duration": 0.05, "channel": 9},
      {"note": 38, "velocity": 100, "start_time": 0.5, "duration": 0.1, "channel": 9},
      {"note": 42, "velocity": 80, "start_time": 0.75, "duration": 0.05, "channel": 9}
    ],
    "tags": ["house", "electronic"]
  }
}
```

### Example 2: Using a Pattern in a Sequence

Play the pattern defined above 16 times:

```json
{
  "tool": "play_sequence",
  "arguments": {
    "patterns": [
      {
        "pattern_name": "house_beat",
        "start_bar": 1,
        "repeat_count": 16
      }
    ],
    "tempo": 128
  }
}
```

### Example 3: Transposing a Bass Line

Define a bass pattern, then play it in different keys:

```json
// First, define the pattern
{
  "tool": "define_sequence_pattern",
  "arguments": {
    "name": "funk_bass",
    "category": "bass",
    "notes": [
      {"note": 36, "velocity": 100, "start_time": 0.0, "duration": 0.25, "instrument": 38},
      {"note": 36, "velocity": 80, "start_time": 0.5, "duration": 0.25, "instrument": 38},
      {"note": 38, "velocity": 90, "start_time": 1.0, "duration": 0.25, "instrument": 38}
    ]
  }
}

// Then use it with transposition
{
  "tool": "play_sequence",
  "arguments": {
    "patterns": [
      {"pattern_name": "funk_bass", "start_bar": 1, "transpose": 0, "repeat_count": 4},
      {"pattern_name": "funk_bass", "start_bar": 5, "transpose": 7, "repeat_count": 4},
      {"pattern_name": "funk_bass", "start_bar": 9, "transpose": 5, "repeat_count": 4}
    ],
    "tempo": 110
  }
}
```

### Example 4: Combining Multiple Patterns

Layer drums and bass together:

```json
{
  "tool": "play_sequence",
  "arguments": {
    "patterns": [
      {"pattern_name": "house_beat", "start_bar": 1, "repeat_count": 16},
      {"pattern_name": "funk_bass", "start_bar": 5, "repeat_count": 12}
    ],
    "tempo": 120
  }
}
```

### Example 5: Listing All Patterns

View all defined patterns in the current session:

```json
{
  "tool": "list_patterns",
  "arguments": {}
}
```

### Example 6: Dynamic Variations

Use velocity and duration scaling for variation:

```json
{
  "tool": "play_sequence",
  "arguments": {
    "patterns": [
      // Verse: Normal bass
      {"pattern_name": "funk_bass", "start_bar": 1, "repeat_count": 8},
      // Pre-chorus: Softer, longer notes
      {"pattern_name": "funk_bass", "start_bar": 9, "velocity_scale": 0.7, "duration_scale": 1.5, "repeat_count": 4},
      // Chorus: Louder, octave up
      {"pattern_name": "funk_bass", "start_bar": 13, "transpose": 12, "velocity_scale": 1.3, "repeat_count": 8}
    ],
    "tempo": 120
  }
}
```

### Example 7: Mixing Patterns and Individual Notes

Combine reusable patterns with one-off notes:

```json
{
  "tool": "play_sequence",
  "arguments": {
    "patterns": [
      {"pattern_name": "house_beat", "start_bar": 1, "repeat_count": 16}
    ],
    "notes": [
      // Add cymbal crashes at key moments
      {"note": 49, "velocity": 120, "start_time": 0.0, "duration": 2.0, "channel": 9},
      {"note": 49, "velocity": 120, "start_time": 16.0, "duration": 2.0, "channel": 9},
      // Add a melody
      {"note": 72, "velocity": 90, "start_time": 8.0, "duration": 1.0, "instrument": 80},
      {"note": 74, "velocity": 90, "start_time": 9.0, "duration": 1.0, "instrument": 80}
    ],
    "tempo": 128
  }
}
```

### Creative Prompts with Sequence Patterns

Try these prompts with your AI:

- "Create a house beat pattern and play it for 32 bars"
- "Define a funk bass line and transpose it through different keys"
- "Make a drum pattern for verse and a different one for chorus, then arrange them"
- "Create a chord progression pattern and use it with variations"
- "Build a complete 16-bar arrangement using patterns"
- "Show me all the patterns I've created so far"

### Benefits of Sequence Patterns

1. **Efficiency**: Define once, use multiple times
2. **Consistency**: Same pattern ensures consistent feel
3. **Variation**: Easy to create variations with transformations
4. **Organization**: Categorize and tag patterns for easy retrieval
5. **Flexibility**: Mix patterns with individual notes
6. **Transposition**: Easily change keys
7. **Dynamic Control**: Scale velocity and duration on the fly 