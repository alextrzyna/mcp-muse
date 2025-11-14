# Sequence Patterns Guide

## Overview

Sequence Patterns is a powerful pattern-based composition system that allows you to create reusable musical patterns and use them throughout your compositions with transformations. This feature dramatically simplifies music creation by letting you define musical elements once and reuse them with variations.

## Key Concepts

### Pattern-Based Composition

Instead of specifying every note individually, you can:

1. **Define** reusable patterns (drum beats, bass lines, chord progressions, melodies)
2. **Store** them in the session with metadata (name, category, tags, description)
3. **Reference** them in compositions with transformations (transpose, scale, repeat, etc.)

### Musical Time Notation

Patterns support both legacy seconds-based timing and modern musical time notation:

- **Seconds-based** (legacy): `start_time: 0.5`, `duration: 1.0`
- **Musical time** (recommended): `musical_time: {bar: 1, beat: 1, tick: 0}`
- **Musical duration**: `musical_duration: "quarter"` or `musical_duration: 1.5` (bars)

Musical time ensures perfect synchronization and makes it easy to align patterns to bar/beat boundaries.

## Three New MCP Tools

### 1. `define_sequence_pattern`

Create and store a reusable musical pattern.

**Parameters:**
- `name` (string, required): Unique identifier for the pattern
- `description` (string, optional): Human-readable description
- `category` (string, optional): Pattern category (drums, bass, melody, chords, harmony, effects)
- `notes` (array, required): Array of notes in the pattern
- `tempo` (number, optional): Default tempo (BPM), defaults to 120
- `pattern_bars` (number, optional): Length in bars for proper looping
- `beats_per_bar` (number, optional): Time signature, defaults to 4
- `quantize_grid` (string, optional): Quantization grid (off, bar, beat, 8th, 16th, 32nd, triplet)
- `tags` (array of strings, optional): Tags for organizing/searching patterns

**Example:**

```json
{
  "name": "house_beat",
  "description": "Classic 4/4 house drum pattern",
  "category": "drums",
  "tempo": 128,
  "pattern_bars": 1.0,
  "beats_per_bar": 4,
  "notes": [
    {
      "note": 36,
      "velocity": 120,
      "start_time": 0.0,
      "duration": 0.1,
      "channel": 9
    },
    {
      "note": 42,
      "velocity": 80,
      "start_time": 0.25,
      "duration": 0.05,
      "channel": 9
    },
    {
      "note": 38,
      "velocity": 100,
      "start_time": 0.5,
      "duration": 0.1,
      "channel": 9
    },
    {
      "note": 42,
      "velocity": 80,
      "start_time": 0.75,
      "duration": 0.05,
      "channel": 9
    }
  ],
  "tags": ["house", "electronic", "four-on-floor"]
}
```

### 2. `play_sequence`

Play a sequence combining individual notes and pattern references.

**Parameters:**
- `notes` (array, optional): Individual notes to play
- `patterns` (array, optional): Pattern references with transformations
- `tempo` (number, optional): Overall tempo in BPM, defaults to 120

**Pattern Reference Parameters:**
- `pattern_name` (string, required): Name of the pattern to reference
- `start_time_offset` (number, optional): Start time in seconds (legacy)
- `start_bar` (number, optional): Which bar to start on (1-based)
- `start_beat` (number, optional): Which beat within the bar (1-based), defaults to 1
- `bars` (array of numbers, optional): Specific bars to play pattern on (e.g., [1, 5, 9, 13])
- `transpose` (number, optional): Semitones to transpose (-12 to +12), defaults to 0
- `velocity_scale` (number, optional): Multiply velocities by this factor (0.1-2.0), defaults to 1.0
- `duration_scale` (number, optional): Multiply durations by this factor (0.1-4.0), defaults to 1.0
- `instrument_override` (number, optional): Override instrument for all notes in pattern
- `channel_override` (number, optional): Override MIDI channel for all notes
- `repeat_count` (number, optional): Number of times to repeat the pattern, defaults to 1
- `repeat_spacing_bars` (number, optional): Bars between repeats, defaults to 0
- `align_to_bars` (boolean, optional): Align to bar boundaries, defaults to true

**Example:**

```json
{
  "patterns": [
    {
      "pattern_name": "house_beat",
      "start_bar": 1,
      "repeat_count": 16
    },
    {
      "pattern_name": "bass_line",
      "start_bar": 5,
      "transpose": 0,
      "repeat_count": 12
    },
    {
      "pattern_name": "bass_line",
      "start_bar": 9,
      "transpose": 7,
      "velocity_scale": 1.2,
      "repeat_count": 8
    }
  ],
  "notes": [
    {
      "note": 72,
      "velocity": 90,
      "start_time": 32.0,
      "duration": 2.0,
      "instrument": 80
    }
  ],
  "tempo": 128
}
```

### 3. `list_patterns`

View all defined patterns in the current session.

**Parameters:** None

**Returns:**
- Empty state message if no patterns defined
- Categorized list of patterns with metadata if patterns exist

**Example Response:**

```
📋 Sequence Patterns

🎼 3 patterns available:

## 🥁 drums (2)
• house_beat - 4 notes, 2.0s duration
  Classic 4/4 house drum pattern [house, electronic, four-on-floor]
• techno_beat - 8 notes, 2.0s duration
  Driving techno drum pattern [techno, driving]

## 🎸 bass (1)
• funky_bass - 16 notes, 4.0s duration
  Syncopated funk bass line [funk, groovy]
```

## Common Use Cases

### 1. Drum Patterns

Define drum beats once, use them throughout your track:

```json
// Define the pattern
{
  "name": "verse_drums",
  "category": "drums",
  "notes": [
    {"note": 36, "velocity": 110, "start_time": 0.0, "duration": 0.1, "channel": 9},
    {"note": 42, "velocity": 70, "start_time": 0.25, "duration": 0.05, "channel": 9},
    {"note": 38, "velocity": 100, "start_time": 0.5, "duration": 0.1, "channel": 9},
    {"note": 42, "velocity": 70, "start_time": 0.75, "duration": 0.05, "channel": 9}
  ]
}

// Use it
{
  "patterns": [
    {"pattern_name": "verse_drums", "start_bar": 1, "repeat_count": 32}
  ],
  "tempo": 120
}
```

### 2. Chord Progressions

Define a chord progression and transpose it to different keys:

```json
// Define I-V-vi-IV in C major
{
  "name": "pop_progression",
  "category": "chords",
  "notes": [
    // C major (bar 1)
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 2.0},
    {"note": 64, "velocity": 80, "start_time": 0.0, "duration": 2.0},
    {"note": 67, "velocity": 80, "start_time": 0.0, "duration": 2.0},
    // G major (bar 2)
    {"note": 67, "velocity": 80, "start_time": 2.0, "duration": 2.0},
    {"note": 71, "velocity": 80, "start_time": 2.0, "duration": 2.0},
    {"note": 74, "velocity": 80, "start_time": 2.0, "duration": 2.0},
    // A minor (bar 3)
    {"note": 69, "velocity": 80, "start_time": 4.0, "duration": 2.0},
    {"note": 72, "velocity": 80, "start_time": 4.0, "duration": 2.0},
    {"note": 76, "velocity": 80, "start_time": 4.0, "duration": 2.0},
    // F major (bar 4)
    {"note": 65, "velocity": 80, "start_time": 6.0, "duration": 2.0},
    {"note": 69, "velocity": 80, "start_time": 6.0, "duration": 2.0},
    {"note": 72, "velocity": 80, "start_time": 6.0, "duration": 2.0}
  ],
  "pattern_bars": 4.0
}

// Play in C major (verse)
// Then transpose to F major (+5 semitones) for chorus
{
  "patterns": [
    {"pattern_name": "pop_progression", "start_bar": 1, "transpose": 0, "repeat_count": 2},
    {"pattern_name": "pop_progression", "start_bar": 9, "transpose": 5, "repeat_count": 2}
  ],
  "tempo": 120
}
```

### 3. Bass Line Variations

Create a bass line and vary it:

```json
// Define bass pattern
{
  "name": "bass_groove",
  "category": "bass",
  "notes": [
    {"note": 36, "velocity": 100, "start_time": 0.0, "duration": 0.25},
    {"note": 36, "velocity": 80, "start_time": 0.5, "duration": 0.25},
    {"note": 38, "velocity": 90, "start_time": 1.0, "duration": 0.25},
    {"note": 36, "velocity": 95, "start_time": 1.5, "duration": 0.25}
  ],
  "pattern_bars": 1.0
}

// Play with variations
{
  "patterns": [
    // Normal (bars 1-4)
    {"pattern_name": "bass_groove", "start_bar": 1, "repeat_count": 4},
    // Softer with longer notes (bars 5-8)
    {"pattern_name": "bass_groove", "start_bar": 5, "velocity_scale": 0.7, "duration_scale": 1.5, "repeat_count": 4},
    // Transposed up an octave (bars 9-12)
    {"pattern_name": "bass_groove", "start_bar": 9, "transpose": 12, "repeat_count": 4}
  ],
  "tempo": 110
}
```

### 4. Specific Bar Placement

Place patterns on specific bars for complex arrangements:

```json
{
  "patterns": [
    // Fill pattern only on bars 4, 8, 12, 16 (end of phrases)
    {"pattern_name": "drum_fill", "bars": [4, 8, 12, 16]},
    // Main beat on all other bars
    {"pattern_name": "main_beat", "start_bar": 1, "repeat_count": 16}
  ],
  "tempo": 120
}
```

### 5. Combining Patterns with Individual Notes

Mix patterns and one-off notes:

```json
{
  "patterns": [
    {"pattern_name": "verse_drums", "start_bar": 1, "repeat_count": 8},
    {"pattern_name": "bass_line", "start_bar": 1, "repeat_count": 8}
  ],
  "notes": [
    // Add a cymbal crash at the beginning
    {"note": 49, "velocity": 120, "start_time": 0.0, "duration": 2.0, "channel": 9},
    // Add a synth lead melody
    {"note": 72, "velocity": 90, "start_time": 8.0, "duration": 1.0, "instrument": 80},
    {"note": 74, "velocity": 90, "start_time": 9.0, "duration": 1.0, "instrument": 80},
    {"note": 76, "velocity": 100, "start_time": 10.0, "duration": 2.0, "instrument": 80}
  ],
  "tempo": 120
}
```

## Pattern Organization Best Practices

### Naming Conventions

Use clear, descriptive names:
- `house_beat_basic`
- `funk_bass_syncopated`
- `pop_progression_1564`
- `techno_hats_open`
- `jazz_swing_ride`

### Categories

Organize patterns into categories:
- **drums**: Drum beats and percussion patterns
- **bass**: Bass lines and low-end grooves
- **melody**: Melodic phrases and hooks
- **chords**: Chord progressions and harmonic patterns
- **harmony**: Background harmonic elements
- **effects**: Sound effects and transitional elements

### Tags

Use tags for detailed organization:
- Genre: `["house", "techno", "funk", "jazz"]`
- Feel: `["groovy", "driving", "laid-back", "energetic"]`
- Complexity: `["simple", "complex", "beginner", "advanced"]`
- Elements: `["four-on-floor", "syncopated", "swing", "straight"]`

## Advanced Features

### Musical Time Notation

For precise timing, use musical time:

```json
{
  "notes": [
    {
      "note": 60,
      "velocity": 80,
      "musical_time": {"bar": 1, "beat": 1, "tick": 0},
      "musical_duration": "quarter"
    },
    {
      "note": 62,
      "velocity": 80,
      "musical_time": {"bar": 1, "beat": 2, "tick": 0},
      "musical_duration": "eighth"
    }
  ]
}
```

### Quantization

Align patterns to specific grid positions:

```json
{
  "name": "swing_pattern",
  "quantize_grid": "triplet",
  "notes": [...]
}
```

### Duration Types

Multiple ways to specify duration:

```json
// As seconds (legacy)
"duration": 0.5

// As bars
"musical_duration": 1.5  // 1.5 bars

// As note values
"musical_duration": "quarter"
"musical_duration": "eighth"
"musical_duration": "sixteenth"
"musical_duration": "triplet"
```

## Pattern Storage

Patterns are stored in memory for the duration of your MCP session:
- Patterns persist across multiple `play_sequence` calls
- Patterns are lost when the MCP server restarts
- Use `list_patterns` to view all stored patterns
- Patterns can be overwritten by defining a new pattern with the same name

## Workflow Example

Complete workflow from pattern definition to composition:

```json
// Step 1: Define your patterns
// (using define_sequence_pattern tool)

{
  "name": "house_kick",
  "category": "drums",
  "notes": [
    {"note": 36, "velocity": 120, "start_time": 0.0, "duration": 0.1, "channel": 9},
    {"note": 36, "velocity": 120, "start_time": 0.5, "duration": 0.1, "channel": 9},
    {"note": 36, "velocity": 120, "start_time": 1.0, "duration": 0.1, "channel": 9},
    {"note": 36, "velocity": 120, "start_time": 1.5, "duration": 0.1, "channel": 9}
  ],
  "pattern_bars": 1.0
}

{
  "name": "house_bass",
  "category": "bass",
  "notes": [
    {"note": 36, "velocity": 100, "start_time": 0.0, "duration": 0.25, "instrument": 38},
    {"note": 36, "velocity": 80, "start_time": 0.5, "duration": 0.25, "instrument": 38},
    {"note": 38, "velocity": 90, "start_time": 1.0, "duration": 0.25, "instrument": 38}
  ],
  "pattern_bars": 1.0
}

// Step 2: List patterns to verify
// (using list_patterns tool)

// Step 3: Compose using patterns
// (using play_sequence tool)

{
  "patterns": [
    // Intro: Just kicks (bars 1-4)
    {"pattern_name": "house_kick", "start_bar": 1, "repeat_count": 4},

    // Verse: Add bass (bars 5-20)
    {"pattern_name": "house_kick", "start_bar": 5, "repeat_count": 16},
    {"pattern_name": "house_bass", "start_bar": 5, "repeat_count": 16},

    // Chorus: Bass up an octave, more intense (bars 21-36)
    {"pattern_name": "house_kick", "start_bar": 21, "repeat_count": 16},
    {"pattern_name": "house_bass", "start_bar": 21, "transpose": 12, "velocity_scale": 1.2, "repeat_count": 16}
  ],
  "tempo": 128
}
```

## Tips and Tricks

1. **Start Simple**: Begin with basic patterns and add complexity gradually
2. **Use Descriptive Names**: Make patterns easy to identify later
3. **Leverage Transposition**: Create one pattern, use it in multiple keys
4. **Layer Patterns**: Combine multiple patterns for rich textures
5. **Vary Dynamics**: Use `velocity_scale` for dynamic variation
6. **Time Variations**: Use `duration_scale` for rhythmic variation
7. **Strategic Placement**: Use the `bars` parameter for fills and accents
8. **Mix with Individual Notes**: Combine patterns with one-off melodic elements
9. **Organize with Categories**: Use categories and tags for large pattern libraries
10. **Test Incrementally**: List patterns and test small sections before building full compositions

## Limitations

- Patterns are session-scoped (not persisted to disk)
- Pattern names must be unique within a session
- Redefining a pattern with the same name overwrites the previous version
- Maximum pattern complexity is limited by available memory
- All transformations are applied at pattern resolution time, not during playback

## Future Enhancements

Potential future features:
- Pattern persistence (save/load from files)
- Pattern import/export
- Built-in pattern library
- Pattern preview/audition
- Pattern editing and updating
- Pattern templates
- Advanced quantization options
- Swing/groove timing
- Pattern chaining and sequences
- Pattern randomization and variation generation
