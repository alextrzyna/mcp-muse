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
    assert_eq!(tools.len(), 8);

    // Check that all tools are present
    let tool_names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"play_notes"));
    assert!(tool_names.contains(&"define_sequence_pattern"));
    assert!(tool_names.contains(&"play_sequence"));
    assert!(tool_names.contains(&"list_patterns"));
    assert!(tool_names.contains(&"stop_playback"));
    assert!(tool_names.contains(&"list_sounds"));
    assert!(tool_names.contains(&"export_audio"));

    // Verify the play_notes tool supports all the functionality
    let play_notes_tool = tools
        .iter()
        .find(|tool| tool["name"] == "play_notes")
        .unwrap();
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("quick sounds")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("MIDI")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("R2D2")
    );
    assert!(
        play_notes_tool["description"]
            .as_str()
            .unwrap()
            .contains("synth patches")
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

    // A missing pattern is a tool execution failure: reported in the result
    // with isError so the model can read the message and recover.
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], true);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("not found"), "unexpected text: {text}");

    child.kill().expect("Failed to kill child process");
}

/// Spawn the server and complete the initialize handshake.
struct TestServer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
}

impl TestServer {
    fn start() -> Self {
        let mut child = Command::new("cargo")
            .args(["run", "--"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start MCP server");
        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut server = Self {
            child,
            stdin,
            reader,
        };
        let init = server.call(json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
        }));
        assert_eq!(init["id"], 0);
        server
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{}", message).unwrap();
    }

    fn send_raw(&mut self, line: &str) {
        writeln!(self.stdin, "{}", line).unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON {line:?}: {e}"))
    }

    fn call(&mut self, message: Value) -> Value {
        self.send(&message);
        self.read()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn ping_returns_empty_result() {
    let mut server = TestServer::start();
    let response = server.call(json!({"jsonrpc": "2.0", "id": 7, "method": "ping"}));
    assert_eq!(response["id"], 7);
    assert_eq!(response["result"], json!({}));
}

#[test]
fn notifications_get_no_response() {
    let mut server = TestServer::start();
    server.send(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    // The next line on stdout must be the answer to this request, not an
    // error for either notification.
    let response = server.call(json!({"jsonrpc": "2.0", "id": 8, "method": "tools/list"}));
    assert_eq!(response["id"], 8);
    assert!(response["result"]["tools"].is_array());
}

#[test]
fn parse_error_has_null_id() {
    let mut server = TestServer::start();
    server.send_raw("{this is not json");
    let response = server.read();
    assert_eq!(response["error"]["code"], -32700);
    assert!(
        response["id"].is_null(),
        "id should be null, got {}",
        response["id"]
    );
}

#[test]
fn initialize_reports_crate_version() {
    let mut server = TestServer::start();
    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
    }));
    assert_eq!(
        response["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn unknown_synth_is_a_tool_error_not_a_piano() {
    let mut server = TestServer::start();
    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": "definitely_not_a_patch", "note": 60, "start_time": 0.0, "duration": 0.2}
        ]}}
    }));
    // An unknown synth *name* is an execution failure (it depends on session
    // state, not the request's shape), so it must never be a JSON-RPC error
    // — only a malformed inline patch value is -32602.
    assert!(response["error"].is_null(), "response: {response}");
    let result = &response["result"];
    // Either the audio device is unavailable (CI) or the synth is rejected;
    // both must surface as isError rather than a protocol error.
    assert_eq!(result["isError"], true, "response: {response}");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("definitely_not_a_patch") || text.contains("Audio output unavailable"),
        "unexpected text: {text}"
    );
}

#[test]
fn stop_playback_with_nothing_playing() {
    let mut server = TestServer::start();
    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "stop_playback", "arguments": {}}
    }));
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Stopped 0"), "unexpected text: {text}");
}

#[test]
fn list_sounds_catalog_names_everything() {
    let mut server = TestServer::start();
    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {}}
    }));
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    for needle in [
        "minimoog_bass",
        "tr_808_kick",
        "Acoustic Grand Piano",
        "Closed Hi-Hat",
        "Happy",
        "studio",
        "define_synth",
    ] {
        assert!(text.contains(needle), "catalog missing {needle}");
    }

    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "synths"}}
    }));
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("sub_bass") && !text.contains("Acoustic Grand Piano"));
}

#[test]
fn musical_duration_number_means_bars_and_time_signature_is_honoured() {
    let mut server = TestServer::start();
    // 3/4 at 120 BPM: a note at bar 2 beat 1 starts at 1.5 s. Server-side we
    // only see the summary, so assert the request is accepted with the new
    // fields rather than rejected by the parser.
    let response = server.call(json!({
        "jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": {"name": "define_sequence_pattern", "arguments": {
            "name": "waltz", "beats_per_bar": 3, "pattern_bars": 1, "quantize_grid": "16th",
            "notes": [{"note": 60, "musical_time": {"bar": 1, "beat": 3, "tick": 100}, "musical_duration": 0.5}]
        }}
    }));
    assert!(response["result"]["isError"].is_null(), "{response}");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("1 bars of 3/4"), "unexpected text: {text}");
}

#[test]
fn custom_effects_chains_are_accepted_in_both_forms() {
    let mut server = TestServer::start();
    // Flat form, as documented in the schema. Note-level effects belong to
    // MIDI and R2D2 notes; a synth note takes its chain from its patch.
    let flat = server.call(json!({
        "jsonrpc": "2.0", "id": 10, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [{
            "instrument": 38, "note": 48, "start_time": 0.0, "duration": 0.3,
            "effects": [
                {"type": "filter", "filter_type": "low_pass", "cutoff": 900, "resonance": 2.0, "intensity": 0.8},
                {"type": "delay", "delay_time": 0.25, "feedback": 0.3, "intensity": 0.5},
                {"type": "reverb", "room_size": 0.7, "intensity": 0.4}
            ]}]}}
    }));
    assert!(flat["error"].is_null(), "flat chain rejected: {flat}");

    // Nested PascalCase form from the previous schema.
    let nested = server.call(json!({
        "jsonrpc": "2.0", "id": 11, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [{
            "instrument": 30, "note": 52, "start_time": 0.0, "duration": 0.3,
            "effects": [
                {"effect": {"type": "Distortion", "drive": 3.0, "tone": 0.6}, "intensity": 0.7},
                {"effect": {"type": "Filter", "filter_type": "HighPass", "cutoff": 200.0}, "intensity": 0.5}
            ]}]}}
    }));
    assert!(nested["error"].is_null(), "nested chain rejected: {nested}");

    // A typo in the type is still a validation error the model can read.
    let bad = server.call(json!({
        "jsonrpc": "2.0", "id": 12, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [{
            "note": 60, "start_time": 0.0, "duration": 0.3,
            "effects": [{"type": "flanger", "intensity": 0.5}]}]}}
    }));
    assert_eq!(bad["error"]["code"], -32602, "{bad}");
    assert!(
        bad["error"]["message"]
            .as_str()
            .unwrap()
            .contains("flanger")
    );

    // All three tools expose the same note schema.
    let tools = server.call(json!({"jsonrpc": "2.0", "id": 13, "method": "tools/list"}));
    let schema_for = |name: &str| -> Value {
        tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()["inputSchema"]["properties"]["notes"]["items"]
            .clone()
    };
    assert_eq!(
        schema_for("play_notes"),
        schema_for("define_sequence_pattern")
    );
    assert_eq!(schema_for("play_notes"), schema_for("play_sequence"));
    assert_eq!(
        schema_for("play_notes")["properties"]["effects"]["items"]["required"],
        json!(["type"])
    );
    let _ = server.call(json!({"jsonrpc": "2.0", "id": 14, "method": "tools/call", "params": {"name": "stop_playback", "arguments": {}}}));
}

#[test]
fn play_mode_is_in_both_schemas_and_validated() {
    let mut server = TestServer::start();
    let tools = server.call(json!({"jsonrpc": "2.0", "id": 30, "method": "tools/list"}));
    for name in ["play_notes", "play_sequence"] {
        let tool = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        let mode = &tool["inputSchema"]["properties"]["mode"];
        assert_eq!(mode["type"], "string", "{name}: {mode}");
        assert_eq!(mode["enum"], json!(["replace", "layer"]), "{name}");
        assert_eq!(mode["default"], "replace", "{name}");
    }

    let bad = server.call(json!({
        "jsonrpc": "2.0", "id": 31, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {
            "mode": "queue",
            "notes": [{"note": 60, "duration": 0.1}]
        }}
    }));
    assert_eq!(bad["error"]["code"], -32602, "{bad}");
    assert!(bad["error"]["message"].as_str().unwrap().contains("queue"));
}

#[test]
fn consecutive_plays_layer_or_replace_and_stop_reports_the_count() {
    let mut server = TestServer::start();
    let note = json!([{"synth": "sub_bass", "note": 36, "duration": 3.0}]);
    let first = server.call(json!({
        "jsonrpc": "2.0", "id": 32, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": note.clone()}}
    }));
    if first["result"]["isError"] == true {
        eprintln!("skipping: {}", first["result"]["content"][0]["text"]);
        return; // no audio device (CI)
    }
    let text = first["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("replace"), "{text}");

    let second = server.call(json!({
        "jsonrpc": "2.0", "id": 33, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"mode": "layer", "notes": note.clone()}}
    }));
    let text = second["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("layer"), "{text}");

    let stop = server.call(json!({
        "jsonrpc": "2.0", "id": 34, "method": "tools/call",
        "params": {"name": "stop_playback", "arguments": {}}
    }));
    let text = stop["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Stopped 2"), "{text}");

    let third = server.call(json!({
        "jsonrpc": "2.0", "id": 35, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": note.clone()}}
    }));
    assert!(third["result"]["isError"].is_null(), "{third}");
    let stop = server.call(json!({
        "jsonrpc": "2.0", "id": 36, "method": "tools/call",
        "params": {"name": "stop_playback", "arguments": {}}
    }));
    let text = stop["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Stopped 1"),
        "replace should have dropped the earlier count: {text}"
    );
}

#[test]
fn define_synth_then_play_by_name() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 10, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {
            "name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("blip") && r["result"]["isError"] != true,
        "{text}"
    );

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 11, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": "blip", "note": 84, "start_time": 0.0, "duration": 0.1}]}}
    }));
    // Either playback started, or the CI box has no audio device; never a parse error.
    assert!(r["error"].is_null(), "{r}");
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Playback started") || text.contains("Audio output unavailable"),
        "{text}"
    );
}

#[test]
fn inline_synth_patches_are_accepted_and_invalid_ones_are_invalid_params() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 12, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "k", "percussion": {"kind": "kick", "punch": 0.9}}, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 13, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "k", "percussion": {"kind": "kick", "snap": 0.9}}, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert_eq!(r["error"]["code"], -32602, "{r}");
    assert!(r["error"]["message"].as_str().unwrap().contains("snap"));
}

#[test]
fn effects_on_synth_notes_are_invalid_params() {
    let mut server = TestServer::start();
    for (id, extra) in [
        (
            23,
            json!({"effects": [{"type": "reverb", "intensity": 0.4}]}),
        ),
        (24, json!({"effects_preset": "studio"})),
    ] {
        let mut note = json!({"synth": "saw_bass", "note": 48, "start_time": 0.0, "duration": 0.3});
        for (k, v) in extra.as_object().unwrap() {
            note[k] = v.clone();
        }
        let r = server.call(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": "play_notes", "arguments": {"notes": [note]}}
        }));
        assert_eq!(r["error"]["code"], -32602, "{r}");
        assert!(
            r["error"]["message"]
                .as_str()
                .unwrap()
                .contains("patch's \"effects\" chain"),
            "{r}"
        );
    }
}

#[test]
fn inline_patch_typos_name_the_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 22, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "subtractive": {"cutoff": 500}}, "note": 60,
             "start_time": 0.0, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602, "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("cutoff"),
        "{r}"
    );
}

#[test]
fn removed_synth_fields_are_invalid_params() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 14, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth_type": "sine", "synth_frequency": 440, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    // serde_json's default map is a BTreeMap, so of the two removed fields on
    // this note the alphabetically-first one is the one reported.
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("synth_frequency"),
        "{r}"
    );
}

#[test]
fn define_synth_with_an_unknown_field_is_invalid_params() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 15, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "subtractive": {"cutoff": 500}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("cutoff"));
}

#[test]
fn absurd_durations_are_invalid_params() {
    let mut server = TestServer::start();
    let synth = server.call(json!({
        "jsonrpc": "2.0", "id": 20, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": "sub_bass", "note": 36, "start_time": 0.0, "duration": 100000}]}}
    }));
    assert_eq!(synth["error"]["code"], -32602, "{synth}");
    assert!(
        synth["error"]["message"]
            .as_str()
            .unwrap()
            .contains("duration"),
        "{synth}"
    );

    let r2d2 = server.call(json!({
        "jsonrpc": "2.0", "id": 21, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"note_type": "r2d2", "r2d2_emotion": "Happy", "r2d2_intensity": 0.8,
             "r2d2_complexity": 2, "start_time": 0.0, "duration": 100000}]}}
    }));
    assert_eq!(r2d2["error"]["code"], -32602, "{r2d2}");
    assert!(
        r2d2["error"]["message"]
            .as_str()
            .unwrap()
            .contains("duration"),
        "{r2d2}"
    );
}

#[test]
fn patterns_can_carry_synth_patches() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 16, "method": "tools/call",
        "params": {"name": "define_sequence_pattern", "arguments": {
            "name": "kick4", "pattern_bars": 1, "notes": [
                {"synth": "tr_808_kick", "musical_time": {"bar": 1, "beat": 1, "tick": 0}, "musical_duration": "quarter"},
                {"synth": "tr_808_kick", "musical_time": {"bar": 1, "beat": 3, "tick": 0}, "musical_duration": "quarter"}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 17, "method": "tools/call",
        "params": {"name": "play_sequence", "arguments": {"patterns": [{"pattern_name": "kick4", "start_bar": 1, "repeat_count": 2}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
}

#[test]
fn fm_and_wavetable_patches_play_by_name_and_inline() {
    let mut server = TestServer::start();
    for name in ["dx7_e_piano", "tx81z_lately", "wt_organ"] {
        let r = server.call(json!({
            "jsonrpc": "2.0", "id": 30, "method": "tools/call",
            "params": {"name": "play_notes", "arguments": {"notes": [
                {"synth": name, "note": 60, "start_time": 0.0, "duration": 0.2}]}}
        }));
        assert!(r["error"].is_null(), "{name}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Playback started") || text.contains("Audio output unavailable"),
            "{name}: {text}"
        );
    }
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 31, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "inline_fm", "fm": {"algorithm": "fan_in", "operators": [{"ratio": 1}, {"ratio": 2, "level": 0.3}, {"ratio": 5, "level": 0.2}]}},
             "note": 64, "start_time": 0.0, "duration": 0.2}]}}
    }));
    assert!(r["error"].is_null(), "{r}");

    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 32, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "synths"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    for needle in [
        "dx7_e_piano",
        "dx7_slap_bass",
        "tx81z_lately",
        "fm_bell",
        "wt_organ",
        "wt_pwm_lead",
        "## keys",
    ] {
        assert!(text.contains(needle), "catalog missing {needle}");
    }
}

#[test]
fn fm_schema_errors_name_the_operator_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 33, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "fm": {"operators": [{"ratio": 1, "detune": 5}]}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(r["error"]["message"].as_str().unwrap().contains("detune"));
}

#[test]
fn inline_fm_patch_errors_name_the_operator_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 34, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "fm": {"operators": [{"ratio": 1, "detune": 5}]}},
             "note": 60, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602, "{r}");
    assert!(r["error"]["message"].as_str().unwrap().contains("detune"));
}

#[test]
fn granular_and_lfo_patches_play_by_name_and_inline() {
    let mut server = TestServer::start();
    for name in ["grain_cloud", "drone", "noise_texture"] {
        let r = server.call(json!({
            "jsonrpc": "2.0", "id": 40, "method": "tools/call",
            "params": {"name": "play_notes", "arguments": {"notes": [
                {"synth": name, "note": 57, "start_time": 0.0, "duration": 0.3}]}}
        }));
        assert!(r["error"].is_null(), "{name}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Playback started") || text.contains("Audio output unavailable"),
            "{name}: {text}"
        );
    }
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 41, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "wob", "subtractive": {"filter": {"cutoff": 600}},
                       "lfo": {"rate": 4, "depth": 0.8, "wave": "sine", "target": "cutoff"}},
             "note": 45, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 42, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "synths"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    for needle in [
        "grain_cloud",
        "formant_texture",
        "noise_texture",
        "drone",
        "granular",
        "lfo",
    ] {
        assert!(text.contains(needle), "catalog missing {needle}");
    }
}

#[test]
fn lfo_and_granular_errors_name_the_field() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 43, "method": "tools/call",
        "params": {"name": "define_synth", "arguments": {"name": "x", "subtractive": {}, "lfo": {"target": "resonance"}}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("resonance")
    );
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 44, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "granular": {"grain_size": 0.1}}, "note": 60, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("grain_size")
    );
}

#[test]
fn time_fracture_delay_plays_by_name_and_inline_and_is_validated() {
    let mut server = TestServer::start();
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 50, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"tempo": 90, "notes": [
            {"synth": "shimmer_keys", "note": 72, "start_time": 0.0, "duration": 0.3}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 51, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"note": 60, "instrument": 0, "start_time": 0.0, "duration": 0.3,
             "effects": [{"type": "delay", "delay_time": 0.5, "sync_tempo": true, "feedback": 0.4, "intensity": 0.5}]}]}}
    }));
    assert!(r["error"].is_null(), "{r}");
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 52, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"synth": {"name": "x", "subtractive": {}, "effects": [{"type": "delay", "pitch_intervals": [40]}]},
             "note": 60, "duration": 0.2}]}}
    }));
    assert_eq!(r["error"]["code"], -32602);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("pitch_intervals")
    );
    let r = server.call(json!({
        "jsonrpc": "2.0", "id": 53, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "effects"}}
    }));
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("random_beats") && text.contains("pitch_intervals"),
        "{text}"
    );
}
