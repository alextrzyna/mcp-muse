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