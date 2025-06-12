# MCP MIDI Server API Reference

This document provides complete technical documentation for the `mcp-muse` MCP server.

## Overview

The `mcp-muse` server implements the Model Context Protocol (MCP) specification, providing MIDI playback capabilities to AI agents through a standardized JSON-RPC 2.0 interface over stdio transport.

## MCP Protocol Compliance

### Supported Methods

| Method | Description | Status |
|--------|-------------|--------|
| `initialize` | Initialize MCP connection | ✅ Implemented |
| `initialized` | Confirm initialization | ✅ Implemented |
| `tools/list` | List available tools | ✅ Implemented |
| `tools/call` | Execute a tool | ✅ Implemented |
| `resources/list` | List resources (empty) | ✅ Implemented |
| `prompts/list` | List prompts (empty) | ✅ Implemented |

### Protocol Version
- **Supported**: `2024-11-05`
- **Transport**: stdio (stdin/stdout)
- **Format**: JSON-RPC 2.0

## Tools

### play_midi

Plays MIDI music from base64-encoded MIDI data using a built-in synthesizer.

#### Schema
```json
{
  "name": "play_midi",
  "description": "Play MIDI music from base64-encoded MIDI data",
  "inputSchema": {
    "type": "object",
    "properties": {
      "midi_data": {
        "type": "string",
        "description": "Base64-encoded MIDI file data"
      }
    },
    "required": ["midi_data"]
  }
}
```

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `midi_data` | string | Yes | Base64-encoded MIDI file data |

#### Example Call
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "play_midi",
    "arguments": {
      "midi_data": "TVRoZAAAAAYAAAABAGBNVHJrAAAADgCQPGQwgDwAAP8vAA=="
    }
  }
}
```

#### Success Response
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "🎵 MIDI playback completed successfully!\n\nPlayback Details:\n- Notes played: 1\n- Duration: ~1.0 seconds\n- Synthesis: Sine wave with ADSR envelope\n\nThe music has been rendered and played through your system's audio output."
      }
    ]
  }
}
```

#### Error Responses

##### Invalid MIDI Data
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Invalid MIDI data: unable to decode base64"
  }
}
```

##### MIDI Parse Error
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Failed to parse MIDI data: invalid file format"
  }
}
```

##### Audio System Error
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32603,
    "message": "Audio playback failed: no output device available"
  }
}
```

## MIDI Support

### Supported MIDI Features

- ✅ **Note On/Off Events**: MIDI notes with velocity
- ✅ **Channel Support**: All 16 MIDI channels
- ✅ **Timing**: Delta time and tempo handling
- ✅ **File Format**: MIDI Type 0 and Type 1 files
- ✅ **Base64 Encoding**: Automatic decode from base64

### Synthesis Engine

- **Waveform**: Sine wave synthesis
- **Envelope**: ADSR (Attack, Decay, Sustain, Release)
- **Polyphony**: Unlimited simultaneous notes
- **Sample Rate**: 44.1kHz
- **Bit Depth**: 16-bit

### MIDI Limitations

- ❌ **Program Changes**: All notes use sine wave synthesis
- ❌ **Control Changes**: No CC support (sustain, modulation, etc.)
- ❌ **Pitch Bend**: Not implemented
- ❌ **Real-time MIDI**: File playback only

## Server Capabilities

### Initialization Response
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {
        "listChanged": false
      },
      "resources": {
        "subscribe": false,
        "listChanged": false
      },
      "prompts": {
        "listChanged": false
      }
    },
    "serverInfo": {
      "name": "mcp-muse",
      "version": "0.1.0"
    }
  }
}
```

## Configuration

### Command Line Interface

```bash
mcp-muse [OPTIONS]

Options:
  --setup    Run setup for MCP hosts
  -h, --help Print help information
```

### Setup Mode

The `--setup` flag automatically configures popular MCP hosts:

#### Cursor Configuration
Creates/updates `~/.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "mcp-muse": {
      "command": "/path/to/mcp-muse",
      "args": []
    }
  }
}
```

### Logging

#### Log Locations
- **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log`
- **Linux**: `~/.local/share/mcp-muse/mcp-muse.log`
- **Windows**: `%APPDATA%\mcp-muse\mcp-muse.log`

#### Log Levels
- **TRACE**: Detailed debugging information
- **DEBUG**: Development debugging
- **INFO**: General information (default)
- **WARN**: Warning messages
- **ERROR**: Error conditions

#### Log Format
```
2024-01-15T10:30:45.123Z INFO mcp_muse: Starting MCP MIDI Server (stdio mode)...
```

## Integration Examples

### Manual MCP Host Setup

For hosts not supported by `--setup`:

```json
{
  "mcpServers": {
    "mcp-muse": {
      "command": "/usr/local/bin/mcp-muse",
      "args": [],
      "env": {}
    }
  }
}
```

### Custom Client Implementation

```python
import json
import subprocess
import base64

class MCPMuseClient:
    def __init__(self, binary_path):
        self.process = subprocess.Popen(
            [binary_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )
        self.request_id = 0
    
    def send_request(self, method, params=None):
        self.request_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": self.request_id,
            "method": method,
            "params": params
        }
        
        json_request = json.dumps(request) + "\n"
        self.process.stdin.write(json_request)
        self.process.stdin.flush()
        
        response = self.process.stdout.readline()
        return json.loads(response)
    
    def play_midi_file(self, midi_file_path):
        with open(midi_file_path, 'rb') as f:
            midi_data = base64.b64encode(f.read()).decode()
        
        return self.send_request("tools/call", {
            "name": "play_midi",
            "arguments": {"midi_data": midi_data}
        })
```

## Performance Considerations

### Audio Latency
- **Typical**: 10-50ms on modern systems
- **Factors**: Audio driver, buffer size, CPU load

### Memory Usage
- **Base**: ~2MB for server
- **Per MIDI**: ~1KB per note event
- **Audio Buffer**: ~1MB for playback

### CPU Usage
- **Idle**: <1% CPU
- **Playback**: 1-5% CPU (depends on polyphony)

### File Size Limits
- **Recommended**: <1MB MIDI files
- **Maximum**: Limited by available memory
- **Base64 Overhead**: ~33% size increase

## Error Handling

### JSON-RPC Error Codes

| Code | Meaning | Common Causes |
|------|---------|---------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid Request | Missing required fields |
| -32601 | Method not found | Unsupported MCP method |
| -32602 | Invalid params | Bad tool arguments |
| -32603 | Internal error | Audio system failure |

### Common Issues

#### "Audio device not available"
- Check system audio settings
- Ensure no other applications are blocking audio
- Try restarting the audio service

#### "Invalid MIDI data"
- Verify base64 encoding is correct
- Check MIDI file is not corrupted
- Ensure file is a valid MIDI format

#### "Connection refused"
- Verify binary path in MCP configuration
- Check file permissions (`chmod +x`)
- Ensure binary is built for correct architecture

## Development

### Building from Source

```bash
git clone https://github.com/your-username/mcp-muse.git
cd mcp-muse
cargo build --release
```

### Testing

```bash
cargo test           # Run all tests
cargo test unit      # Unit tests only
cargo test integration  # Integration tests only
```

### Contributing

1. Follow Rust coding conventions
2. Add tests for new features
3. Update documentation
4. Ensure `cargo clippy` passes
5. Format code with `cargo fmt`

## Version History

### v0.1.0 (Current)
- Initial release
- Basic MIDI playback
- MCP protocol compliance
- Cursor integration
- Sine wave synthesis 