// BASE64 imports removed - no longer needed after consolidation
use serde_json::{json, Value};
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
    assert_eq!(tools.len(), 1);

    // Check that the consolidated play_notes tool is present
    let tool_names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"play_notes"));
    
    // Verify the play_notes tool supports all the functionality
    let play_notes_tool = tools.iter().find(|tool| tool["name"] == "play_notes").unwrap();
    assert!(play_notes_tool["description"].as_str().unwrap().contains("UNIVERSAL AUDIO ENGINE"));
    assert!(play_notes_tool["description"].as_str().unwrap().contains("MIDI music"));
    assert!(play_notes_tool["description"].as_str().unwrap().contains("R2D2 expressions"));
    assert!(play_notes_tool["description"].as_str().unwrap().contains("custom synthesis"));

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
