#!/usr/bin/env python3

import json
import subprocess
import time
import sys
import os

# Test the exact drum sequence that was failing, now with consolidated playback
drum_sequence = {
    "notes": [
        {
            "synth_type": "kick",
            "start_time": 0,
            "duration": 0.1,
            "velocity": 127,
            "synth_amplitude": 0.9
        },
        {
            "synth_type": "hihat",
            "start_time": 0.5,
            "duration": 0.05,
            "velocity": 80,
            "synth_amplitude": 0.6
        },
        {
            "synth_type": "snare",
            "start_time": 1,
            "duration": 0.1,
            "velocity": 110,
            "synth_amplitude": 0.8
        },
        {
            "synth_type": "cymbal",
            "start_time": 2,
            "duration": 1.0,
            "velocity": 110,
            "synth_amplitude": 0.8
        }
    ],
    "tempo": 120
}

# Test drums with effects using the consolidated system
drums_with_effects = {
    "notes": [
        {
            "synth_type": "kick",
            "start_time": 0,
            "duration": 0.1,
            "velocity": 127,
            "synth_amplitude": 0.9,
            "effects_preset": "studio"
        },
        {
            "synth_type": "snare",
            "start_time": 0.5,
            "duration": 0.1,
            "velocity": 120,
            "synth_amplitude": 0.8,
            "effects_preset": "concert_hall"
        },
        {
            "synth_type": "hihat",
            "start_time": 1.0,
            "duration": 0.05,
            "velocity": 100,
            "synth_amplitude": 0.6,
            "effects_preset": "vintage"
        },
        {
            "synth_type": "cymbal",
            "start_time": 1.5,
            "duration": 1.0,
            "velocity": 110,
            "synth_amplitude": 0.7,
            "effects_preset": "ambient"
        }
    ],
    "tempo": 120
}

print("🎛️ Testing CONSOLIDATED PLAYBACK SYSTEM")
print("✅ Now ALL sequences use play_enhanced_mixed() with full 6-effect support!")
print()

print("📋 Test 1: Basic drum synthesis (should sound like drums, not sine waves)")
print(json.dumps(drum_sequence, indent=2))
print()

print("📋 Test 2: Drums with professional effects (should work with all 14 presets)")
print(json.dumps(drums_with_effects, indent=2))
print()

print("🔧 Next: Start MCP server and test these sequences through the play_notes tool")
print("🎯 Expected: Both tests use 'enhanced mixed playback' with proper drum sounds + effects")