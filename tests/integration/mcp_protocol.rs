use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use serde_json::{json, Value};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

#[test]
fn test_mcp_initialize() {
    let mut child = Command::new("cargo")
        .args(&["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Send initialize request
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    writeln!(stdin, "{}", init_request).expect("Failed to write to stdin");

    // Read response
    let mut response_line = String::new();
    reader.read_line(&mut response_line).expect("Failed to read response");

    let response: Value = serde_json::from_str(&response_line)
        .expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response["result"].is_object());
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert!(response["result"]["capabilities"].is_object());

    child.kill().expect("Failed to kill child process");
}

#[test]
fn test_mcp_tools_list() {
    let mut child = Command::new("cargo")
        .args(&["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initialize first
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    writeln!(stdin, "{}", init_request).expect("Failed to write to stdin");
    let mut response_line = String::new();
    reader.read_line(&mut response_line).expect("Failed to read init response");

    // Send tools/list request
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });

    writeln!(stdin, "{}", tools_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader.read_line(&mut response_line).expect("Failed to read tools response");

    let response: Value = serde_json::from_str(&response_line)
        .expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"]["tools"].is_array());
    
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "play_midi");
    assert!(tools[0]["description"].is_string());
    assert!(tools[0]["inputSchema"].is_object());

    child.kill().expect("Failed to kill child process");
}

#[test]
fn test_play_midi_tool_with_invalid_base64() {
    let mut child = Command::new("cargo")
        .args(&["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initialize first
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    writeln!(stdin, "{}", init_request).expect("Failed to write to stdin");
    let mut response_line = String::new();
    reader.read_line(&mut response_line).expect("Failed to read init response");

    // Send play_midi with invalid base64
    let play_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "play_midi",
            "arguments": {
                "midi_data": "invalid_base64!"
            }
        }
    });

    writeln!(stdin, "{}", play_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader.read_line(&mut response_line).expect("Failed to read play response");

    let response: Value = serde_json::from_str(&response_line)
        .expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32602);

    child.kill().expect("Failed to kill child process");
}

#[test]
fn test_play_midi_tool_with_valid_midi() {
    // Create a simple valid MIDI file
    let mut midi_bytes = Vec::new();
    
    // MIDI header
    midi_bytes.extend_from_slice(&[
        0x4D, 0x54, 0x68, 0x64,  // "MThd"
        0x00, 0x00, 0x00, 0x06,  // Header length (6 bytes)
        0x00, 0x00,              // Format type 0
        0x00, 0x01,              // Number of tracks (1)
        0x00, 0x60               // Ticks per quarter note (96)
    ]);
    
    // Track
    midi_bytes.extend_from_slice(&[
        0x4D, 0x54, 0x72, 0x6B,  // "MTrk"
        0x00, 0x00, 0x00, 0x04,  // Track length (4 bytes)
        0x00, 0xFF, 0x2F, 0x00   // End of track
    ]);

    let midi_b64 = BASE64.encode(&midi_bytes);

    let mut child = Command::new("cargo")
        .args(&["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initialize first
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    writeln!(stdin, "{}", init_request).expect("Failed to write to stdin");
    let mut response_line = String::new();
    reader.read_line(&mut response_line).expect("Failed to read init response");

    // Send play_midi with valid MIDI
    let play_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "play_midi",
            "arguments": {
                "midi_data": midi_b64
            }
        }
    });

    writeln!(stdin, "{}", play_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader.read_line(&mut response_line).expect("Failed to read play response");

    let response: Value = serde_json::from_str(&response_line)
        .expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    
    // The response might be an error if audio hardware isn't available in CI,
    // but it should be a proper JSON-RPC response either way
    assert!(response["result"].is_object() || response["error"].is_object());

    child.kill().expect("Failed to kill child process");
} 