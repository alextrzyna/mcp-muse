#!/usr/bin/env python3

import json
import subprocess
import sys

# Test the exact drum sequence that was failing
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
            "synth_type": "snare",
            "start_time": 0.5,
            "duration": 0.1,
            "velocity": 110,
            "synth_amplitude": 0.8
        },
        {
            "synth_type": "hihat",
            "start_time": 1.0,
            "duration": 0.05,
            "velocity": 80,
            "synth_amplitude": 0.6
        },
        {
            "synth_type": "cymbal",
            "start_time": 1.5,
            "duration": 1.0,
            "velocity": 110,
            "synth_amplitude": 0.8
        }
    ],
    "tempo": 120
}

print("🧪 Testing direct drum synthesis types (no presets)")
print("This should now produce proper drum sounds, not sine waves!")

# Write test data to a temporary file
with open('/tmp/test_direct_drums.json', 'w') as f:
    json.dump(drum_sequence, f, indent=2)

print("✅ Test configuration created. Test data:")
print(json.dumps(drum_sequence, indent=2))