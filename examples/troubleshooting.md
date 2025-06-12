# Troubleshooting Guide

This guide helps resolve common issues with the mcp-muse MIDI server.

## Installation Issues

### Rust Build Errors

**Error**: `cargo build` fails with compilation errors

**Solutions**:
1. Update Rust to latest stable: `rustup update stable`
2. Clear cargo cache: `cargo clean && cargo build`
3. Check minimum Rust version (1.70+): `rustc --version`

**Error**: Missing audio dependencies on Linux

**Solutions**:
```bash
# Ubuntu/Debian
sudo apt-get install libasound2-dev

# Fedora/RHEL
sudo dnf install alsa-lib-devel

# Arch
sudo pacman -S alsa-lib
```

### Permission Issues

**Error**: `Permission denied` when running binary

**Solution**:
```bash
chmod +x target/release/mcp-muse
```

**Error**: Cannot write to config directory

**Solution**:
```bash
# Check and fix home directory permissions
ls -la ~/.cursor/
mkdir -p ~/.cursor
chmod 755 ~/.cursor
```

## Setup Issues

### Cursor Integration

**Problem**: Cursor doesn't show mcp-muse tools after setup

**Solutions**:
1. **Restart Cursor completely** (quit and relaunch)
2. Check config file exists and is valid:
   ```bash
   cat ~/.cursor/mcp.json
   ```
3. Verify JSON syntax:
   ```bash
   python -m json.tool ~/.cursor/mcp.json
   ```
4. Check binary path is correct:
   ```bash
   which mcp-muse
   # or
   ls -la /path/to/mcp-muse
   ```

**Problem**: "Server failed to start" error in Cursor

**Solutions**:
1. Test binary manually:
   ```bash
   ./target/release/mcp-muse
   # Should wait for JSON-RPC input
   ```
2. Check logs in Cursor Developer Tools
3. Verify binary is executable and not corrupted

### Configuration Issues

**Problem**: MCP configuration is ignored

**Solutions**:
1. Check file location: `~/.cursor/mcp.json` (not `.vscode/`)
2. Restart Cursor after config changes
3. Verify config structure:
   ```json
   {
     "mcpServers": {
       "mcp-muse": {
         "command": "/full/path/to/mcp-muse",
         "args": []
       }
     }
   }
   ```

## Audio Issues

### No Sound Output

**Problem**: Tool executes but no audio plays

**Diagnostic Steps**:
1. Test system audio with other applications
2. Check system volume and mute status
3. Verify audio device is working:
   ```bash
   # macOS
   system_profiler SPAudioDataType
   
   # Linux
   aplay -l
   pactl list sinks
   ```

**Solutions**:
1. **macOS**: Check Security & Privacy settings for audio access
2. **Linux**: 
   ```bash
   # PulseAudio
   pulseaudio --check
   pulseaudio --start
   
   # ALSA
   sudo systemctl restart alsa-state
   ```
3. **Windows**: Update audio drivers

### Audio Device Errors

**Error**: "No available output device" or "Audio initialization failed"

**Solutions**:
1. Check other applications can play audio
2. Restart audio service:
   ```bash
   # macOS
   sudo killall coreaudiod
   
   # Linux (PulseAudio)
   systemctl --user restart pulseaudio
   ```
3. Try different audio backend (Linux):
   ```bash
   # Set environment variable
   export ALSA_OUTPUT_DEVICE=default
   ```

### Audio Quality Issues

**Problem**: Distorted or crackling audio

**Solutions**:
1. Reduce MIDI velocities (use values < 100 instead of 127)
2. Lower system audio latency
3. Close other audio-intensive applications
4. Check CPU usage during playback

**Problem**: Delayed audio playback

**Solutions**:
1. Check system audio latency settings
2. Update audio drivers
3. Reduce buffer sizes in audio settings

## MIDI Issues

### Invalid MIDI Data

**Error**: "Invalid MIDI data" or "Failed to parse MIDI"

**Diagnostic**:
1. Verify base64 encoding:
   ```bash
   echo "TVRoZA..." | base64 -d | file -
   # Should show: MIDI sound file
   ```

**Solutions**:
1. Re-encode MIDI file:
   ```bash
   base64 -i your_file.mid
   ```
2. Check MIDI file format (Type 0 or Type 1 supported)
3. Try a known-good MIDI file from examples
4. Verify file isn't corrupted

### MIDI Playback Issues

**Problem**: MIDI plays but sounds wrong

**Common Causes**:
- Tempo too fast/slow
- Wrong note mappings
- Velocity issues

**Solutions**:
1. Check MIDI file in another player first
2. Try simple examples from documentation
3. Verify MIDI file format matches expectations

## Communication Issues

### JSON-RPC Errors

**Error**: "Parse error" or "Invalid request"

**Solutions**:
1. Check MCP client is sending valid JSON
2. Verify protocol version compatibility
3. Test with manual JSON-RPC:
   ```bash
   echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | ./mcp-muse
   ```

**Error**: "Method not found"

**Solutions**:
1. Check method name spelling (`tools/call`, not `tool/call`)
2. Verify MCP server is fully initialized
3. List available tools:
   ```json
   {"jsonrpc":"2.0","id":1,"method":"tools/list"}
   ```

### Connection Issues

**Problem**: MCP client can't connect to server

**Solutions**:
1. Verify stdio transport (not TCP/WebSocket)
2. Check binary path in configuration
3. Test server starts correctly:
   ```bash
   timeout 5s ./mcp-muse
   # Should not exit immediately
   ```

## Logging and Debugging

### Enable Debug Logging

**Temporary**:
```bash
RUST_LOG=debug ./mcp-muse
```

**Check Log Files**:
- **macOS**: `~/Library/Application Support/mcp-muse/mcp-muse.log`
- **Linux**: `~/.local/share/mcp-muse/mcp-muse.log`

### Common Log Messages

**Normal Operation**:
```
Starting MCP MIDI Server (stdio mode)...
Handling initialize request
Handling tools/list request
```

**Errors to Investigate**:
```
Audio playback failed: [details]
Failed to parse MIDI data: [details]
Invalid base64 encoding: [details]
```

## Performance Issues

### High CPU Usage

**Solutions**:
1. Reduce MIDI complexity (fewer simultaneous notes)
2. Lower audio quality settings
3. Close unnecessary applications
4. Check for audio driver issues

### Memory Usage

**Problem**: High memory consumption

**Solutions**:
1. Use smaller MIDI files
2. Restart server periodically for long-running sessions
3. Check for memory leaks in logs

## Getting Help

### Before Reporting Issues

1. **Test with examples**: Try examples from `examples/basic_usage.md`
2. **Check logs**: Review log files for error details
3. **Verify setup**: Re-run `--setup` command
4. **Test environment**: Try on different system if available

### Information to Include

When asking for help, provide:

1. **System Information**:
   ```bash
   uname -a                    # OS version
   rustc --version            # Rust version
   ./mcp-muse --help          # Binary version
   ```

2. **Configuration**:
   ```bash
   cat ~/.cursor/mcp.json     # MCP config
   ```

3. **Logs**:
   ```bash
   tail -50 ~/Library/Application\ Support/mcp-muse/mcp-muse.log
   ```

4. **Error reproduction**: Exact steps and commands that cause the issue

### Community Resources

- **GitHub Issues**: Report bugs and feature requests
- **Documentation**: Check README.md and examples/
- **MCP Specification**: https://github.com/modelcontextprotocol

## Quick Fixes Summary

| Problem | Quick Fix |
|---------|-----------|
| No sound | Check volume, restart audio service |
| Cursor can't find tools | Restart Cursor completely |
| Permission denied | `chmod +x mcp-muse` |
| Invalid MIDI | Try example MIDI data |
| Setup fails | Check home directory permissions |
| High CPU | Reduce MIDI complexity |
| Connection fails | Verify binary path in config | 