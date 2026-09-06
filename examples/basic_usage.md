# mcp-muse Usage Examples

Practical examples of using the `mcp-muse` server from an AI agent. Every
example is a `play_notes` tool call; the JSON shown is the `arguments`
object.

## Example 1: Single Note

Middle C for one second on the default piano.

```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 1.0}
  ]
}
```

## Example 2: C Major Scale

```json
{
  "notes": [
    {"note": 60, "start_time": 0.0, "duration": 0.4},
    {"note": 62, "start_time": 0.4, "duration": 0.4},
    {"note": 64, "start_time": 0.8, "duration": 0.4},
    {"note": 65, "start_time": 1.2, "duration": 0.4},
    {"note": 67, "start_time": 1.6, "duration": 0.4},
    {"note": 69, "start_time": 2.0, "duration": 0.4},
    {"note": 71, "start_time": 2.4, "duration": 0.4},
    {"note": 72, "start_time": 2.8, "duration": 0.8}
  ]
}
```

## Example 3: Chord with an Instrument and Reverb

A C major chord on string ensemble (GM program 48) in a hall.

```json
{
  "notes": [
    {"note": 60, "instrument": 48, "reverb": 70, "start_time": 0.0, "duration": 2.0},
    {"note": 64, "instrument": 48, "reverb": 70, "start_time": 0.0, "duration": 2.0},
    {"note": 67, "instrument": 48, "reverb": 70, "start_time": 0.0, "duration": 2.0}
  ]
}
```

## Example 4: Musical Time and a Drum Beat

Timing in bars and beats at 100 BPM. Channel 9 is the GM drum kit
(36 = kick, 38 = snare, 42 = closed hi-hat).

```json
{
  "tempo": 100,
  "notes": [
    {"channel": 9, "note": 36, "musical_time": {"bar": 1, "beat": 1, "tick": 0}, "musical_duration": "eighth"},
    {"channel": 9, "note": 42, "musical_time": {"bar": 1, "beat": 2, "tick": 0}, "musical_duration": "eighth"},
    {"channel": 9, "note": 38, "musical_time": {"bar": 1, "beat": 3, "tick": 0}, "musical_duration": "eighth"},
    {"channel": 9, "note": 42, "musical_time": {"bar": 1, "beat": 4, "tick": 0}, "musical_duration": "eighth"}
  ]
}
```

## Example 5: Classic Synth Preset

Call `list_sounds` (section `presets`) to see every name. Presets bring
their own effects unless you pass `effects` yourself.

```json
{
  "notes": [
    {"preset_name": "Minimoog Bass", "note": 36, "velocity": 110, "start_time": 0.0, "duration": 0.5},
    {"preset_name": "Minimoog Bass", "note": 43, "velocity": 100, "start_time": 0.5, "duration": 0.5},
    {"preset_name": "JP-8 Strings", "note": 64, "start_time": 0.0, "duration": 3.0},
    {"preset_name": "JP-8 Strings", "note": 67, "start_time": 0.0, "duration": 3.0}
  ]
}
```

## Example 6: R2D2 Reaction plus Synthesis Effect

```json
{
  "notes": [
    {"note_type": "r2d2", "r2d2_emotion": "Excited", "r2d2_intensity": 0.8, "r2d2_complexity": 3, "start_time": 0.0, "duration": 1.2},
    {"synth_type": "zap", "synth_frequency": 900, "start_time": 1.2, "duration": 0.4}
  ]
}
```

## Stopping and Timing

Playback tools return immediately; the reply says how long the audio will
run including effect tails. Call `stop_playback` to cut it short.

## Using with AI Agents

### In Cursor

1. Run `mcp-muse setup` (or `./target/release/mcp-muse setup` from a source build)
2. Restart Cursor
3. In a new chat, ask:

```
"Play me a C major scale on a harpsichord."
```

### Creative Prompts

- "Play a short victory fanfare with brass and a cymbal crash"
- "Give me a moody synth pad and an R2D2 that sounds worried"
- "Make a four-bar house beat with a TB-303 bassline"
- "Play a blues scale in E minor on overdriven guitar"

## MIDI Technical Notes

- C4 (middle C) = 60; each octave adds or subtracts 12 (C3 = 48, C5 = 72)
- `velocity` 0-127: 64 medium, 127 maximum
- `instrument` is the General MIDI program number; `list_sounds` lists all 128 by family
- Channel 9 is always the drum kit; `note` selects the drum
- With `musical_time`, ticks run 0-479 per beat (480 PPQ)

## Troubleshooting Examples

### No Sound
1. Check system volume and the selected output device
2. Test with Example 1
3. Look at the newest `mcp-muse.log.*` file (see `api_reference.md` for locations)

### Tool result says isError
The text explains why: usually a preset or pattern name that does not
exist. `list_sounds` and `list_patterns` show the valid names.

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