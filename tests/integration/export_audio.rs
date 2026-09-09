//! export_audio through the real binary: a synth note needs no SoundFont and
//! no audio device, so this runs everywhere CI does.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
#[allow(clippy::zombie_processes)]
fn export_audio_writes_a_wav_file() {
    let dir = std::env::temp_dir().join(format!(
        "mcp-muse-integration-export-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut call = |message: Value| -> Value {
        writeln!(stdin, "{}", message).unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("JSON response")
    };

    call(json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
    }));
    let r = call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "export_audio", "arguments": {
            "notes": [{"synth": {"name": "blip", "subtractive": {"osc1": {"wave": "square"}, "env": {"release": 0.05}}}, "note": 84, "duration": 0.1}],
            "path": dir.to_string_lossy(),
            "name": "blip",
            "bit_depth": 16
        }}
    }));
    assert_eq!(r["id"], 1);
    assert!(r["result"].get("isError").is_none(), "{}", r);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let expected = dir.join("blip.wav");
    assert!(text.contains(&expected.display().to_string()), "{}", text);

    let mut header = [0u8; 12];
    std::fs::File::open(&expected)
        .unwrap()
        .read_exact(&mut header)
        .unwrap();
    assert_eq!(&header[0..4], b"RIFF");
    assert_eq!(&header[8..12], b"WAVE");

    child.kill().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
