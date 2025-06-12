


<div align="center">
  <img src="assets/images/muse-logo.png" alt="MCP Muse Logo" width="400"/>
  
  <br/>
  <br/>
  
  [![Create Issue](https://img.shields.io/badge/Create-GitHub%20Issue-red?style=for-the-badge&logo=github)](https://github.com/alextrzyna/mcp-muse/issues/new)
  
  <br/>
  <br/>
  
  <h1>🎵 mcp-muse 🎵</h1>
  
  <p><strong>Give your AI agent a musical voice with MIDI playback capabilities.</strong></p>
</div>

## Overview

`mcp-muse` is a Rust-based MCP (Model Context Protocol) server that enables AI agents to play MIDI music. It provides a `play_midi` tool that accepts base64-encoded MIDI data and synthesizes audio in real-time using a built-in virtual synthesizer.

## Features

- 🎼 **MIDI Playback**: Full MIDI file parsing and audio synthesis
- 🔌 **MCP Integration**: Standards-compliant MCP server for AI agent integration
- 🎹 **Virtual Synthesizer**: Built-in sine wave synthesis with ADSR envelope
- ⚙️ **Easy Setup**: Automatic configuration for popular MCP hosts (Cursor, Claude)
- 📊 **Real-time Audio**: Low-latency audio playback using Rodio
- 🧪 **Well-Tested**: Comprehensive unit and integration test coverage

## Installation

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Audio output device (speakers/headphones)

### From Source

```bash
git clone https://github.com/alextrzyna/mcp-muse.git
cd mcp-muse
cargo build --release
```

The binary will be available at `target/release/mcp-muse`.

## Quick Start

### 1. Run Setup

For **Cursor**:
```bash
./target/release/mcp-muse --setup
```

This automatically configures `~/.cursor/mcp.json` with the correct binary path.

### 2. Restart Cursor

Close and reopen Cursor for the MCP server to be available.

### 3. Use in AI Chat

Ask your AI agent to play music:

```
"Can you play a simple melody for me?"
```

The agent will use the `play_midi` tool to generate and play MIDI music.

## Usage Examples

### Basic MIDI Playback

The agent can play MIDI from base64 data:

```json
{
  "tool": "play_midi",
  "arguments": {
    "midi_data": "TVRoZAAAAAYAAAABAF9NVHJrAAAAGgAAAAD/UQMBhqAA/1kCAQD/WwIAAAAA/y8A"
  }
}
```

### Creative Scenarios

Ask your agent to:
- "Compose a happy birthday melody"
- "Play a chord progression in C major"
- "Create a simple drum pattern"
- "Play the opening of Für Elise"

## Configuration

### Manual MCP Configuration

If you need to manually configure other MCP hosts, add this to your MCP configuration:

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

Logs are written to:
- **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log`
- **Linux**: `~/.local/share/mcp-muse/mcp-muse.log`
- **Windows**: `%APPDATA%\mcp-muse\mcp-muse.log`

## Technical Details

### Architecture

- **MCP Server**: JSON-RPC 2.0 over stdio transport
- **MIDI Parser**: Uses `midly` crate for robust MIDI parsing
- **Audio Engine**: `rodio` for cross-platform audio playback
- **Synthesizer**: Custom sine wave synthesis with ADSR envelope

### Tool Schema

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
cargo run -- --setup  # Run setup
cargo run             # Run MCP server
```

## Troubleshooting

### Audio Issues

**No sound output:**
1. Check your system audio settings
2. Ensure audio device is not muted
3. Verify other applications can play audio
4. Check logs for audio device errors

**Distorted audio:**
- Reduce MIDI velocity values
- Check for audio buffer underruns in logs

### MCP Connection Issues

**Agent can't find tools:**
1. Restart your MCP host (Cursor/Claude)
2. Check MCP configuration file syntax
3. Verify binary path in configuration
4. Check logs for connection errors

**Permission denied:**
```bash
chmod +x target/release/mcp-muse
```

### Common Error Messages

- `"Invalid MIDI data"`: Ensure MIDI data is properly base64-encoded
- `"Audio device unavailable"`: Check system audio configuration
- `"Failed to parse MIDI"`: MIDI file may be corrupted or unsupported

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Setup

```bash
git clone https://github.com/alextrzyna/mcp-muse.git
cd mcp-muse
cargo build
cargo test
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- MIDI parsing by [midly](https://github.com/kovaxis/midly)
- Audio playback by [rodio](https://github.com/RustAudio/rodio)
- MCP specification by [Anthropic](https://github.com/modelcontextprotocol)
