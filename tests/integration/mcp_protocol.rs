// BASE64 imports removed - no longer needed after consolidation
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
#[allow(clippy::zombie_processes)]
fn test_mcp_initialize() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response["result"].is_object());
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert!(response["result"]["capabilities"].is_object());

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_mcp_tools_list() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Send tools/list request
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });

    writeln!(stdin, "{}", tools_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read tools response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"]["tools"].is_array());

    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);

    // Check that all tools are present
    let tool_names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"play_notes"));
    assert!(tool_names.contains(&"define_sequence_pattern"));
    assert!(tool_names.contains(&"play_sequence"));
    assert!(tool_names.contains(&"list_patterns"));

    // Verify the play_notes tool supports all the functionality
    let play_notes_tool = tools
        .iter()
        .find(|tool| tool["name"] == "play_notes")
        .unwrap();
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("UNIVERSAL AUDIO ENGINE")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("MIDI music")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("R2D2 expressions")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("custom synthesis")
    );

    // Verify structure of both tools
    for tool in tools {
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
    }

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_play_notes_tool_with_invalid_arguments() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Send play_notes with invalid arguments (empty notes array)
    let play_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "play_notes",
            "arguments": {
                "notes": []
            }
        }
    });

    writeln!(stdin, "{}", play_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read play response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32602);

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_play_notes_tool_with_valid_notes() {
    // Create a simple valid note sequence
    let notes = json!([
        {
            "note": 60,
            "velocity": 80,
            "start_time": 0.0,
            "duration": 1.0,
            "channel": 0,
            "instrument": 0
        }
    ]);

    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Send play_notes with valid notes
    let play_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "play_notes",
            "arguments": {
                "notes": notes
            }
        }
    });

    writeln!(stdin, "{}", play_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read play response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);

    // The response might be an error if audio hardware isn't available in CI,
    // but it should be a proper JSON-RPC response either way
    assert!(response["result"].is_object() || response["error"].is_object());

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_define_sequence_pattern_valid() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Define a drum pattern
    let define_pattern_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "define_sequence_pattern",
            "arguments": {
                "name": "test_beat",
                "description": "A simple test drum pattern",
                "category": "drums",
                "tempo": 120,
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
                        "start_time": 0.5,
                        "duration": 0.05,
                        "channel": 9
                    }
                ],
                "tags": ["house", "basic"]
            }
        }
    });

    writeln!(stdin, "{}", define_pattern_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read define pattern response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object());
    assert!(response["result"]["content"].is_array());

    // Check that the response contains success message
    let content_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content_text.contains("test_beat"));
    assert!(content_text.contains("2 notes"));

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_define_sequence_pattern_invalid_empty_notes() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Try to define a pattern with empty notes
    let define_pattern_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "define_sequence_pattern",
            "arguments": {
                "name": "empty_pattern",
                "notes": []
            }
        }
    });

    writeln!(stdin, "{}", define_pattern_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read define pattern response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("empty")
    );

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_list_patterns_empty() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // List patterns (should be empty)
    let list_patterns_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "list_patterns",
            "arguments": {}
        }
    });

    writeln!(stdin, "{}", list_patterns_request).expect("Failed to write to stdin");

    // Read response
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read list patterns response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object());

    let content_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content_text.contains("No patterns defined"));

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_list_patterns_with_patterns() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Define a drum pattern
    let define_pattern_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "define_sequence_pattern",
            "arguments": {
                "name": "house_beat",
                "description": "Classic 4/4 house drum pattern",
                "category": "drums",
                "notes": [
                    {"note": 36, "velocity": 120, "start_time": 0.0, "duration": 0.1, "channel": 9}
                ],
                "tags": ["house", "electronic"]
            }
        }
    });

    writeln!(stdin, "{}", define_pattern_request).expect("Failed to write to stdin");
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read define pattern response");

    // List patterns
    let list_patterns_request = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "list_patterns",
            "arguments": {}
        }
    });

    writeln!(stdin, "{}", list_patterns_request).expect("Failed to write to stdin");
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read list patterns response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);
    assert!(response["result"].is_object());

    let content_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content_text.contains("house_beat"));
    assert!(content_text.contains("drums"));
    assert!(content_text.contains("1 notes"));

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_play_sequence_with_pattern_reference() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initialize
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Define a pattern
    let define_pattern_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "define_sequence_pattern",
            "arguments": {
                "name": "bass_line",
                "category": "bass",
                "notes": [
                    {"note": 48, "velocity": 90, "start_time": 0.0, "duration": 0.5, "channel": 0}
                ]
            }
        }
    });

    writeln!(stdin, "{}", define_pattern_request).expect("Failed to write to stdin");
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read define pattern response");

    // Play sequence using the pattern
    let play_sequence_request = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "play_sequence",
            "arguments": {
                "patterns": [
                    {
                        "pattern_name": "bass_line",
                        "start_time_offset": 0.0,
                        "transpose": 0,
                        "velocity_scale": 1.0
                    }
                ],
                "tempo": 120
            }
        }
    });

    writeln!(stdin, "{}", play_sequence_request).expect("Failed to write to stdin");
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read play sequence response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);

    // Should succeed (or fail with audio error in CI, but not with pattern error)
    if response["error"].is_object() {
        let error_msg = response["error"]["message"].as_str().unwrap();
        // Should not be a pattern resolution error - specifically check for pattern-specific errors
        assert!(
            !error_msg.contains("Pattern '")
                && !error_msg.contains("pattern")
                && !error_msg.contains("Failed to resolve patterns"),
            "Got pattern resolution error: {}",
            error_msg
        );
        // Audio/SoundFont errors are OK in test environment
    } else {
        assert!(response["result"].is_object());
    }

    child.kill().expect("Failed to kill child process");
}

#[test]
#[allow(clippy::zombie_processes)]
fn test_play_sequence_pattern_not_found() {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Initialize
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
    reader
        .read_line(&mut response_line)
        .expect("Failed to read init response");

    // Try to play sequence with non-existent pattern
    let play_sequence_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "play_sequence",
            "arguments": {
                "patterns": [
                    {
                        "pattern_name": "nonexistent_pattern",
                        "start_time_offset": 0.0
                    }
                ],
                "tempo": 120
            }
        }
    });

    writeln!(stdin, "{}", play_sequence_request).expect("Failed to write to stdin");
    response_line.clear();
    reader
        .read_line(&mut response_line)
        .expect("Failed to read play sequence response");

    let response: Value =
        serde_json::from_str(&response_line).expect("Failed to parse JSON response");

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found")
    );

    child.kill().expect("Failed to kill child process");
}
