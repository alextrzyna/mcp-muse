//! End to end: a note with `midi_out` leaves the server through its virtual
//! MIDI port and reaches another CoreMIDI/ALSA client, the way a DAW
//! listening on that port would see it.
use super::mcp_protocol::TestServer;
use serde_json::json;

#[cfg(unix)]
#[test]
fn a_midi_out_note_arrives_on_the_virtual_port() {
    use midir::MidiInput;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    let Ok(probe) = MidiInput::new("mcp-muse itest probe") else {
        eprintln!("skipping: no MIDI backend");
        return;
    };
    let mut server = TestServer::start();

    // Every source named mcp-muse: other test servers may be up at the same
    // time, and only ours sends the distinctive note below.
    let (tx, rx) = channel::<Vec<u8>>();
    let mut connections = Vec::new();
    for port in probe.ports() {
        if probe.port_name(&port).ok().as_deref() != Some("mcp-muse") {
            continue;
        }
        let input = MidiInput::new("mcp-muse itest").unwrap();
        let tx = tx.clone();
        let connection = input
            .connect(
                &port,
                "in",
                move |_stamp, message, _| {
                    let _ = tx.send(message.to_vec());
                },
                (),
            )
            .expect("connect to the virtual port");
        connections.push(connection);
    }
    assert!(
        !connections.is_empty(),
        "the server's virtual port is missing from the MIDI sources"
    );

    let catalog = server.call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "list_sounds", "arguments": {"section": "midi_outputs"}}
    }));
    let text = catalog["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("- mcp-muse —"), "{text}");

    let played = server.call(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "play_notes", "arguments": {"notes": [
            {"note": 61, "velocity": 99, "channel": 5, "duration": 0.2, "midi_out": "mcp-muse"}
        ]}}
    }));
    let text = played["result"]["content"][0]["text"].as_str().unwrap();
    if played["result"]["isError"] == true {
        eprintln!("skipping: {text}");
        return; // no audio device (CI)
    }
    assert!(text.contains("external MIDI (mcp-muse)"), "{text}");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        if let Ok(message) = rx.recv_timeout(Duration::from_millis(100)) {
            seen.push(message);
        }
        if seen.contains(&vec![0x85, 61, 0]) {
            break;
        }
    }
    assert!(
        seen.contains(&vec![0x95, 61, 99]),
        "note-on missing from {seen:?}"
    );
    assert!(
        seen.contains(&vec![0x85, 61, 0]),
        "note-off missing from {seen:?}"
    );
}
