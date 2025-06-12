# MCP Muse - Simple Note Examples

The `play_notes` tool provides a much simpler way to create music than dealing with raw MIDI bytes!

## Basic Examples

### Single Note (2 seconds)
```json
{
  "notes": [
    {
      "note": 60,        // Middle C
      "velocity": 80,    // Medium volume
      "start_time": 0.0, // Start immediately
      "duration": 2.0    // Play for 2 seconds
    }
  ]
}
```

### Simple Melody (C-D-E-F)
```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 0.5},  // C
    {"note": 62, "velocity": 80, "start_time": 0.5, "duration": 0.5},  // D
    {"note": 64, "velocity": 80, "start_time": 1.0, "duration": 0.5},  // E
    {"note": 65, "velocity": 80, "start_time": 1.5, "duration": 1.0}   // F (longer)
  ]
}
```

### Chord (C Major)
```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 2.0},  // C
    {"note": 64, "velocity": 80, "start_time": 0.0, "duration": 2.0},  // E
    {"note": 67, "velocity": 80, "start_time": 0.0, "duration": 2.0}   // G
  ]
}
```

### Arpeggio (C Major broken chord)
```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 0.5},   // C
    {"note": 64, "velocity": 80, "start_time": 0.25, "duration": 0.5},  // E
    {"note": 67, "velocity": 80, "start_time": 0.5, "duration": 0.5},   // G
    {"note": 72, "velocity": 80, "start_time": 0.75, "duration": 0.5}   // C (octave)
  ]
}
```

### Chord with Different Instruments (C Major)
```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 2.0, "channel": 0, "instrument": 0},
    {"note": 64, "velocity": 80, "start_time": 0.0, "duration": 2.0, "channel": 1, "instrument": 40},
    {"note": 67, "velocity": 80, "start_time": 0.0, "duration": 2.0, "channel": 2, "instrument": 73}
  ]
}
```

This plays a C major chord with Piano (0), Violin (40), and Flute (73).

## MIDI Note Numbers

Common notes and their MIDI numbers:
- C4 (Middle C): 60
- C#4/Db4: 61
- D4: 62
- D#4/Eb4: 63
- E4: 64
- F4: 65
- F#4/Gb4: 66
- G4: 67
- G#4/Ab4: 68
- A4: 69
- A#4/Bb4: 70
- B4: 71
- C5: 72

## Parameters

- **note**: MIDI note number (0-127, where 60 = middle C)
- **velocity**: Volume/intensity (0-127, where 127 = loudest)
- **start_time**: When to start the note (in seconds)
- **duration**: How long to play the note (in seconds)
- **channel**: MIDI channel (0-15, optional, defaults to 0)
- **instrument**: MIDI instrument (0-127, General MIDI program number, optional)

## Comparison with Raw MIDI

### Old way (raw MIDI bytes):
```
TVRoZAAAAAYAAAABAeBNVHJrAAAADQCQPFCDYIA8QAD/LwA=
```
This is a base64-encoded MIDI file that's hard to read and understand.

### New way (simple notes):
```json
{
  "notes": [
    {"note": 60, "velocity": 80, "start_time": 0.0, "duration": 2.0}
  ]
}
```
Much clearer what's happening: play middle C at medium volume for 2 seconds! 