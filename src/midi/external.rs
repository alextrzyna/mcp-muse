//! MIDI output to ports on this machine: a DAW listening on the server's
//! virtual port, an IAC bus, a hardware interface. Design:
//! docs/superpowers/specs/2026-09-09-external-midi-design.md.
//!
//! `ExternalMidi` lives on the tool thread and owns the list of opened
//! ports; the `midir` connections themselves live on a sender thread so the
//! audio thread only ever pushes a few bytes onto a channel.

use crate::midi::engine::EventKind;
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

/// Name of the virtual port the server publishes for DAWs to select as an input.
pub const VIRTUAL_PORT_NAME: &str = "mcp-muse";
const CLIENT_NAME: &str = "mcp-muse";
const MIDI_CHANNELS: u8 = 16;
const ALL_SOUND_OFF: u8 = 120;
const ALL_NOTES_OFF: u8 = 123;

/// Index of an opened port in `ExternalMidi::opened`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortId(pub u16);

/// MIDI wire bytes for an event, and how many of the three are used.
pub fn encode(kind: &EventKind) -> ([u8; 3], usize) {
    match *kind {
        EventKind::NoteOn {
            channel,
            key,
            velocity,
        } => ([0x90 | (channel & 0x0F), key & 0x7F, velocity & 0x7F], 3),
        EventKind::NoteOff { channel, key } => ([0x80 | (channel & 0x0F), key & 0x7F, 0], 3),
        EventKind::ControlChange {
            channel,
            controller,
            value,
        } => (
            [0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F],
            3,
        ),
        EventKind::ProgramChange { channel, program } => {
            ([0xC0 | (channel & 0x0F), program & 0x7F, 0], 2)
        }
    }
}

/// What a name resolved to: a port already open, or one of the
/// destinations currently on the machine (by index into that list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Opened(PortId),
    Available(usize),
}

/// Case-insensitive lookup over opened ports then available destinations:
/// an exact match first, then a unique substring match. Ambiguity and no
/// match are errors that list the choices.
pub fn resolve_name(
    name: &str,
    opened: &[String],
    available: &[String],
) -> Result<Resolved, String> {
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(
            "midi_out must name an output; call list_sounds with section \"midi_outputs\"".into(),
        );
    }
    // Opened ports first: an opened destination usually still appears in
    // the available list, and the open connection is the one to use.
    let mut candidates: Vec<(&str, Resolved)> = opened
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), Resolved::Opened(PortId(i as u16))))
        .collect();
    for (i, n) in available.iter().enumerate() {
        if !opened.iter().any(|o| o.eq_ignore_ascii_case(n)) {
            candidates.push((n.as_str(), Resolved::Available(i)));
        }
    }
    if let Some((_, found)) = candidates.iter().find(|(n, _)| n.to_lowercase() == wanted) {
        return Ok(*found);
    }
    let partial: Vec<&(&str, Resolved)> = candidates
        .iter()
        .filter(|(n, _)| n.to_lowercase().contains(&wanted))
        .collect();
    match partial.as_slice() {
        [(_, found)] => Ok(*found),
        [] => {
            let names: Vec<&str> = candidates.iter().map(|(n, _)| *n).collect();
            Err(format!(
                "Unknown MIDI output '{}'. Available: {:?}. Call list_sounds with section \"midi_outputs\".",
                name.trim(),
                names
            ))
        }
        many => {
            let names: Vec<&str> = many.iter().map(|(n, _)| *n).collect();
            Err(format!(
                "MIDI output '{}' is ambiguous: {:?}. Give the full name.",
                name.trim(),
                names
            ))
        }
    }
}

/// What the audio thread and the tool thread send to the sender thread.
pub enum ExternalMessage {
    Send {
        port: PortId,
        bytes: [u8; 3],
        len: usize,
    },
    /// CC 123 and CC 120 on every channel of every open port.
    AllNotesOff,
    Open {
        port: PortId,
        connection: MidiOutputConnection,
    },
    /// Answered once every message before it has been handled.
    Sync(Sender<()>),
}

/// Cloneable handle to the sender thread; the engine keeps one.
#[derive(Clone)]
pub struct ExternalSender(Sender<ExternalMessage>);

impl ExternalSender {
    /// A closed channel means the sender thread is gone; external notes are
    /// then dropped silently (everything else keeps playing).
    pub fn send(&self, message: ExternalMessage) {
        if self.0.send(message).is_err() {
            tracing::debug!("External MIDI sender is gone; message dropped");
        }
    }

    #[cfg(test)]
    pub fn for_test() -> (Self, Receiver<ExternalMessage>) {
        let (tx, rx) = channel();
        (Self(tx), rx)
    }
}

/// The destinations on the machine right now, for the catalog.
#[derive(Debug, Clone)]
pub struct Outputs {
    /// `Ok` when the server's virtual port exists.
    pub virtual_port: Result<(), String>,
    pub destinations: Vec<String>,
}

/// Owner of the opened ports, on the tool thread.
pub struct ExternalMidi {
    sender: ExternalSender,
    /// Opened port names; index is the `PortId`.
    opened: Vec<String>,
    virtual_port: Result<(), String>,
}

impl Default for ExternalMidi {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalMidi {
    /// Spawn the sender thread and publish the virtual port. Never fails:
    /// without a MIDI backend, `outputs()` says so and every `resolve`
    /// errors.
    pub fn new() -> Self {
        let (tx, rx) = channel();
        if let Err(e) = thread::Builder::new()
            .name("mcp-muse midi out".into())
            .spawn(move || run_sender(rx))
        {
            tracing::warn!("Could not start the external MIDI sender: {}", e);
        }
        let mut external = Self {
            sender: ExternalSender(tx),
            opened: Vec::new(),
            virtual_port: Err("not created".into()),
        };
        external.virtual_port = external.create_virtual_port();
        match &external.virtual_port {
            Ok(()) => tracing::info!("Published virtual MIDI port '{}'", VIRTUAL_PORT_NAME),
            Err(e) => tracing::warn!("No virtual MIDI port: {}", e),
        }
        external
    }

    #[cfg(unix)]
    fn create_virtual_port(&mut self) -> Result<(), String> {
        use midir::os::unix::VirtualOutput;
        let output = MidiOutput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
        let connection = output
            .create_virtual(VIRTUAL_PORT_NAME)
            .map_err(|e| e.to_string())?;
        self.add(VIRTUAL_PORT_NAME.to_string(), connection);
        Ok(())
    }

    #[cfg(not(unix))]
    fn create_virtual_port(&mut self) -> Result<(), String> {
        Err("virtual MIDI ports need a driver on Windows (for example loopMIDI); send to one of its ports instead".into())
    }

    fn add(&mut self, name: String, connection: MidiOutputConnection) -> PortId {
        let port = PortId(self.opened.len() as u16);
        self.opened.push(name);
        self.sender.send(ExternalMessage::Open { port, connection });
        port
    }

    /// Everything on the machine a note can be sent to, right now.
    pub fn outputs(&self) -> Outputs {
        let destinations = match destinations() {
            Ok(list) => list.into_iter().map(|(name, _)| name).collect(),
            Err(e) => {
                tracing::warn!("Could not list MIDI destinations: {}", e);
                Vec::new()
            }
        };
        Outputs {
            virtual_port: self.virtual_port.clone(),
            destinations,
        }
    }

    /// Find a port by name, opening a destination on first use.
    pub fn resolve(&mut self, name: &str) -> Result<PortId, String> {
        let available = destinations().unwrap_or_default();
        let names: Vec<String> = available.iter().map(|(n, _)| n.clone()).collect();
        match resolve_name(name, &self.opened, &names) {
            Ok(Resolved::Opened(id)) => Ok(id),
            Ok(Resolved::Available(i)) => {
                let (port_name, port) = &available[i];
                let output = MidiOutput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
                let connection = output
                    .connect(port, VIRTUAL_PORT_NAME)
                    .map_err(|e| format!("Could not open MIDI output '{}': {}", port_name, e))?;
                tracing::info!("Opened MIDI output '{}'", port_name);
                Ok(self.add(port_name.clone(), connection))
            }
            Err(e) => {
                if name.trim().eq_ignore_ascii_case(VIRTUAL_PORT_NAME)
                    && let Err(reason) = &self.virtual_port
                {
                    return Err(format!(
                        "The '{}' virtual port is not available: {}",
                        VIRTUAL_PORT_NAME, reason
                    ));
                }
                Err(e)
            }
        }
    }

    pub fn sender(&self) -> ExternalSender {
        self.sender.clone()
    }

    /// Names of the ports opened so far (the virtual port first when it exists).
    pub fn opened(&self) -> &[String] {
        &self.opened
    }
}

impl Drop for ExternalMidi {
    /// Leave no note hanging in a DAW when the server exits: send all
    /// notes off and wait briefly for the sender thread to get it out.
    fn drop(&mut self) {
        self.sender.send(ExternalMessage::AllNotesOff);
        let (tx, rx) = channel();
        self.sender.send(ExternalMessage::Sync(tx));
        let _ = rx.recv_timeout(Duration::from_millis(250));
    }
}

/// Destinations the MIDI backend reports, minus the server's own virtual
/// port (some backends list it as a destination too).
fn destinations() -> Result<Vec<(String, MidiOutputPort)>, String> {
    let output = MidiOutput::new(CLIENT_NAME).map_err(|e| e.to_string())?;
    Ok(output
        .ports()
        .into_iter()
        .filter_map(|port| {
            let name = output.port_name(&port).ok()?;
            (name != VIRTUAL_PORT_NAME).then_some((name, port))
        })
        .collect())
}

/// The sender thread: owns the connections and forwards bytes. A port
/// whose `send` fails (unplugged) is dropped and logged once; `resolve`
/// reopens it if it comes back.
fn run_sender(rx: Receiver<ExternalMessage>) {
    let mut ports: Vec<Option<MidiOutputConnection>> = Vec::new();
    let all_notes_off = |ports: &mut Vec<Option<MidiOutputConnection>>| {
        for slot in ports.iter_mut() {
            let Some(connection) = slot else { continue };
            for channel in 0..MIDI_CHANNELS {
                for controller in [ALL_NOTES_OFF, ALL_SOUND_OFF] {
                    let _ = connection.send(&[0xB0 | channel, controller, 0]);
                }
            }
        }
    };
    loop {
        match rx.recv() {
            Ok(ExternalMessage::Send { port, bytes, len }) => {
                let Some(slot) = ports.get_mut(port.0 as usize) else {
                    continue;
                };
                if let Some(connection) = slot
                    && let Err(e) = connection.send(&bytes[..len])
                {
                    tracing::warn!("MIDI output {} failed, closing it: {}", port.0, e);
                    *slot = None;
                }
            }
            Ok(ExternalMessage::AllNotesOff) => all_notes_off(&mut ports),
            Ok(ExternalMessage::Open { port, connection }) => {
                let index = port.0 as usize;
                if ports.len() <= index {
                    ports.resize_with(index + 1, || None);
                }
                ports[index] = Some(connection);
            }
            Ok(ExternalMessage::Sync(ack)) => {
                let _ = ack.send(());
            }
            Err(_) => {
                all_notes_off(&mut ports);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_encode_to_channel_voice_messages() {
        assert_eq!(
            encode(&EventKind::NoteOn {
                channel: 2,
                key: 60,
                velocity: 100
            }),
            ([0x92, 60, 100], 3)
        );
        assert_eq!(
            encode(&EventKind::NoteOff {
                channel: 15,
                key: 61
            }),
            ([0x8F, 61, 0], 3)
        );
        assert_eq!(
            encode(&EventKind::ControlChange {
                channel: 0,
                controller: 7,
                value: 127
            }),
            ([0xB0, 7, 127], 3)
        );
        assert_eq!(
            encode(&EventKind::ProgramChange {
                channel: 9,
                program: 42
            }),
            ([0xC9, 42, 0], 2)
        );
    }

    #[test]
    fn encode_masks_out_of_range_values_so_a_status_byte_never_leaks_into_data() {
        let (bytes, _) = encode(&EventKind::NoteOn {
            channel: 17,
            key: 200,
            velocity: 255,
        });
        assert_eq!(bytes, [0x91, 200 & 0x7F, 0x7F]);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_names_resolve_case_insensitively_with_opened_ports_first() {
        let opened = names(&["mcp-muse", "IAC Driver Bus 1"]);
        let available = names(&["IAC Driver Bus 1", "Minilogue"]);
        assert_eq!(
            resolve_name("MCP-MUSE", &opened, &available),
            Ok(Resolved::Opened(PortId(0)))
        );
        assert_eq!(
            resolve_name("iac driver bus 1", &opened, &available),
            Ok(Resolved::Opened(PortId(1)))
        );
        assert_eq!(
            resolve_name(" Minilogue ", &opened, &available),
            Ok(Resolved::Available(1))
        );
    }

    #[test]
    fn a_unique_substring_resolves_and_an_ambiguous_one_lists_the_candidates() {
        let opened = names(&["mcp-muse"]);
        let available = names(&["IAC Driver Bus 1", "IAC Driver Bus 2", "Minilogue"]);
        assert_eq!(
            resolve_name("mini", &opened, &available),
            Ok(Resolved::Available(2))
        );
        let err = resolve_name("iac", &opened, &available).unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(err.contains("IAC Driver Bus 1") && err.contains("IAC Driver Bus 2"));
        assert!(!err.contains("Minilogue"));
    }

    #[test]
    fn an_unknown_name_lists_every_output_and_an_empty_name_is_rejected() {
        let opened = names(&["mcp-muse"]);
        let available = names(&["Minilogue"]);
        let err = resolve_name("bitwig", &opened, &available).unwrap_err();
        assert!(err.contains("Unknown MIDI output 'bitwig'"), "{err}");
        assert!(err.contains("mcp-muse") && err.contains("Minilogue"));
        assert!(err.contains("list_sounds"));
        assert!(resolve_name("  ", &opened, &available).is_err());
    }

    #[test]
    fn the_sender_thread_forwards_bytes_and_silences_every_port_on_exit() {
        // No real port is needed to check the thread's bookkeeping: with no
        // connection installed, sends to an unknown port are ignored and
        // the thread still answers a sync and exits cleanly.
        let (tx, rx) = channel();
        let handle = thread::spawn(move || run_sender(rx));
        tx.send(ExternalMessage::Send {
            port: PortId(7),
            bytes: [0x90, 60, 100],
            len: 3,
        })
        .unwrap();
        tx.send(ExternalMessage::AllNotesOff).unwrap();
        let (ack_tx, ack_rx) = channel();
        tx.send(ExternalMessage::Sync(ack_tx)).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(tx);
        handle.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_virtual_port_is_visible_to_other_clients_and_hidden_from_the_catalog() {
        let external = ExternalMidi::new();
        if let Err(e) = &external.virtual_port {
            eprintln!("skipping: no MIDI backend ({e})");
            return;
        }
        let input = midir::MidiInput::new("mcp-muse test").expect("backend up");
        let sources: Vec<String> = input
            .ports()
            .iter()
            .filter_map(|p| input.port_name(p).ok())
            .collect();
        assert!(
            sources.iter().any(|n| n == VIRTUAL_PORT_NAME),
            "virtual port missing from {sources:?}"
        );
        let outputs = external.outputs();
        assert!(outputs.virtual_port.is_ok());
        assert!(!outputs.destinations.iter().any(|n| n == VIRTUAL_PORT_NAME));
        assert_eq!(external.opened(), [VIRTUAL_PORT_NAME.to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn resolving_the_virtual_port_reuses_it_and_unknown_names_fail() {
        let mut external = ExternalMidi::new();
        if external.virtual_port.is_err() {
            return;
        }
        assert_eq!(external.resolve("MCP-Muse"), Ok(PortId(0)));
        assert_eq!(external.opened().len(), 1);
        let err = external
            .resolve("no such synth on this machine")
            .unwrap_err();
        assert!(err.contains("Unknown MIDI output"), "{err}");
    }
}
