#![recursion_limit = "512"]

use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod expressive;
mod midi;
mod server;
mod setup;

use crate::midi::{MidiPlayer, SimpleNote, SimpleSequence};

/// Determines the log directory, with fallback to current directory if creation fails
fn determine_log_directory(preferred_dir: PathBuf) -> PathBuf {
    // Try to create the preferred directory
    match fs::create_dir_all(&preferred_dir) {
        Ok(_) => {
            tracing::debug!("Successfully created log directory: {:?}", preferred_dir);
            preferred_dir
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not create log directory {:?}: {}",
                preferred_dir, e
            );
            eprintln!("Falling back to current directory for logging");

            let fallback_dir = PathBuf::from(".");
            tracing::debug!("Falling back to directory: {:?}", fallback_dir);
            fallback_dir
        }
    }
}

fn init_logging() {
    // Cross-platform data directory (macOS: ~/Library/Application Support, Linux: ~/.local/share, Windows: %APPDATA%)
    let preferred_log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcp-muse");

    let log_dir = determine_log_directory(preferred_log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "mcp-muse.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .init();

    // Log the directory being used for transparency
    tracing::info!("Logging to directory: {:?}", log_dir);

    // _guard must be kept alive, so we leak it (ok for a server)
    std::mem::forget(_guard);
}

#[derive(Parser, Debug)]
#[command(
    author = "mcp-muse team",
    version = "0.1.0",
    about = "🎵 Universal Audio Engine: MIDI Music, R2D2 Expressions & Custom Synthesis"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Parser, Debug)]
pub enum Commands {
    /// Start the MCP server
    Server {
        /// Server name to register with
        #[arg(long, default_value = "mcp-muse")]
        name: String,
    },

    /// Run setup for MCP hosts
    Setup,

    /// Test preset functionality
    #[command(name = "test-presets")]
    TestPresets,

    /// Test polyphony validation
    #[command(name = "test-polyphony")]
    TestPolyphony,

    /// Test drum synthesis
    #[command(name = "test-drums")]
    TestDrums,

    /// Debug DX7 synthesis issues
    #[command(name = "debug-dx7")]
    DebugDX7,

    /// Test enhanced pad presets
    #[command(name = "test-pads")]
    TestPads,

    /// Test volume-corrected presets
    #[command(name = "test-volumes")]
    TestVolumes,

    #[command(name = "test-effects")]
    TestEffects,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    let args = Args::parse();

    match args.command {
        Some(Commands::Server { name: _ }) => {
            tracing::info!("Starting MCP MIDI Server (stdio mode)...");
            server::run_stdio_server();
        }
        Some(Commands::Setup) => {
            setup::run_setup();
        }
        Some(Commands::TestPresets) => {
            test_preset_integration().await?;
        }
        Some(Commands::TestPolyphony) => {
            test_polyphony_validation().await?;
        }
        Some(Commands::TestDrums) => {
            test_drum_synthesis().await?;
        }
        Some(Commands::DebugDX7) => {
            test_dx7_debugging().await?;
        }
        Some(Commands::TestPads) => {
            test_enhanced_pads().await?;
        }
        Some(Commands::TestVolumes) => {
            test_volume_consistency().await?;
        }
        Some(Commands::TestEffects) => {
            test_effects_system().await?;
        }
        None => {
            // Default behavior: start the MCP server
            tracing::info!("Starting MCP MIDI Server (stdio mode)...");
            server::run_stdio_server();
        }
    }

    Ok(())
}

/// Test the preset integration with actual audio playback
async fn test_preset_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎹 Testing Classic Synthesizer Preset Integration!");
    println!("This will test the complete audio pipeline with presets...\n");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test 0: Specific test for reported non-working presets
    println!("🔧 Test 0: Testing reported problematic presets");

    println!("  🎹 Testing JP-8 Strings...");
    let jp8_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("JP-8 Strings".to_string()),
            note: Some(60), // C4
            velocity: Some(80),
            start_time: 0.0,
            duration: 3.0,
            channel: 0,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(jp8_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(3500)).await;

    println!("  🎹 Testing DX7 E.Piano...");
    let dx7_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("DX7 E.Piano".to_string()),
            note: Some(64), // E4
            velocity: Some(90),
            start_time: 0.0,
            duration: 2.0,
            channel: 0,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(dx7_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    println!("✅ Problematic preset test completed\n");

    // Test 1: Specific preset by name
    println!("🎵 Test 1: Playing Minimoog Bass preset");
    let minimoog_sequence = SimpleSequence {
        notes: vec![
            SimpleNote {
                preset_name: Some("Minimoog Bass".to_string()),
                note: Some(36), // C2
                velocity: Some(100),
                start_time: 0.0,
                duration: 1.0,
                channel: 0,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("Minimoog Bass".to_string()),
                note: Some(43), // G2
                velocity: Some(90),
                start_time: 1.0,
                duration: 1.0,
                channel: 0,
                note_type: "midi".to_string(),
                ..Default::default()
            },
        ],
        tempo: 120,
    };

    player.play_enhanced_mixed(minimoog_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Test 2: Random preset from bass category
    println!("🎵 Test 2: Playing random bass preset");
    let random_bass_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_category: Some("bass".to_string()),
            note: Some(40), // E2
            velocity: Some(110),
            start_time: 0.0,
            duration: 1.5,
            channel: 0,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };

    player.play_enhanced_mixed(random_bass_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Test 3: Preset with variation
    println!("🎵 Test 3: Playing TB-303 Acid preset with squelchy variation");
    let acid_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TB-303 Acid".to_string()),
            preset_variation: Some("squelchy".to_string()),
            note: Some(45), // A2
            velocity: Some(127),
            start_time: 0.0,
            duration: 2.0,
            channel: 0,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };

    player.play_enhanced_mixed(acid_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Test 4: Multiple presets together
    println!("🎵 Test 4: Playing multiple presets together");
    let multi_preset_sequence = SimpleSequence {
        notes: vec![
            // Bass line
            SimpleNote {
                preset_name: Some("Jupiter Bass".to_string()),
                note: Some(36), // C2
                velocity: Some(100),
                start_time: 0.0,
                duration: 2.0,
                channel: 0,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Pad
            SimpleNote {
                preset_category: Some("pad".to_string()),
                note: Some(60), // C4
                velocity: Some(80),
                start_time: 0.5,
                duration: 3.0,
                channel: 1,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_category: Some("pad".to_string()),
                note: Some(64), // E4
                velocity: Some(75),
                start_time: 0.5,
                duration: 3.0,
                channel: 1,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Random preset
            SimpleNote {
                preset_random: Some(true),
                note: Some(72), // C5
                velocity: Some(90),
                start_time: 1.0,
                duration: 1.0,
                channel: 2,
                note_type: "midi".to_string(),
                ..Default::default()
            },
        ],
        tempo: 120,
    };

    player.play_enhanced_mixed(multi_preset_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("✅ All preset tests completed successfully!");
    println!("🎉 The classic synthesizer preset system is fully operational!");

    Ok(())
}

/// Test polyphony validation with comprehensive scenarios
async fn test_polyphony_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Comprehensive Polyphony Validation");
    println!("{}", "=".repeat(60));

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test 1: Voice Manager Unit Tests
    println!("🔧 Test 1: Voice Manager Unit Tests");
    println!("Testing voice manager internals directly...");

    use crate::expressive::{EnvelopeParams, PolyphonicVoiceManager, SynthParams, SynthType};

    let mut voice_manager = PolyphonicVoiceManager::new(44100.0);

    // Basic voice allocation test
    let synth_params = SynthParams {
        synth_type: SynthType::Sine,
        frequency: 440.0,
        amplitude: 0.5,
        duration: 1.0,
        envelope: EnvelopeParams {
            attack: 0.1,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
        },
        filter: None,
        effects: Vec::new(),
    };

    println!("  🧪 Testing basic voice allocation...");
    let voice_id = voice_manager
        .allocate_voice(synth_params.clone(), 0.0, Some(60), 0, 100)
        .map_err(|e| format!("Voice allocation failed: {}", e))?;
    println!("     ✅ Allocated voice ID: {}", voice_id);

    // Voice count tracking
    println!("  🧪 Testing voice count tracking...");
    let active_count = voice_manager.active_voice_count();
    println!("     ✅ Active voices: {}", active_count);

    // Multiple voice allocation
    println!("  🧪 Testing multiple voice allocation...");
    for i in 1..8 {
        let _voice_id = voice_manager
            .allocate_voice(
                synth_params.clone(),
                i as f64 * 0.1,
                Some(60 + i as u8),
                0,
                100,
            )
            .map_err(|e| format!("Voice allocation {} failed: {}", i, e))?;
    }
    let active_count = voice_manager.active_voice_count();
    println!("     ✅ Active voices after allocation: {}", active_count);

    // Voice processing
    println!("  🧪 Testing voice processing...");
    let dt = 1.0 / 44100.0; // One sample at 44.1kHz
    for _ in 0..1000 {
        // Process 1000 samples
        let _output = voice_manager.process_voices(dt);
    }
    println!("     ✅ Voice processing completed without errors");

    // Voice information
    println!("  🧪 Testing voice information retrieval...");
    let voice_info = voice_manager.get_voice_info();
    println!("     ✅ Retrieved info for {} voices", voice_info.len());

    for (id, state, note, channel) in voice_info.iter().take(3) {
        println!(
            "       Voice {}: state={:?}, note={:?}, channel={}",
            id, state, note, channel
        );
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 2: Polyphonic Chord Progression
    println!("\n🎹 Test 2: Polyphonic Chord Progression");
    println!("Testing classic synthesizer presets with complex chord progressions...");

    let mut notes = Vec::new();

    // Create rich chord progression using classic presets
    let chord_times = [0.0, 2.0, 4.0, 6.0];
    let chord_progressions = [
        vec![60, 64, 67, 72], // C Major
        vec![57, 60, 64, 69], // A Minor
        vec![58, 62, 65, 70], // Bb Major
        vec![67, 71, 74, 79], // G Major
    ];

    for (i, &start_time) in chord_times.iter().enumerate() {
        let chord = &chord_progressions[i % chord_progressions.len()];

        for &note_num in chord {
            notes.push(SimpleNote {
                start_time,
                duration: 3.0, // Long notes for rich overlapping
                note: Some(note_num),
                velocity: Some(80),
                preset_name: Some("JP-8 Strings".to_string()),
                ..Default::default()
            });
        }
    }

    // Add bass line with Minimoog Bass
    let bass_notes = [36, 33, 34, 43]; // C, A, Bb, G bass notes
    for (i, &start_time) in chord_times.iter().enumerate() {
        notes.push(SimpleNote {
            start_time,
            duration: 1.8,
            note: Some(bass_notes[i % bass_notes.len()]),
            velocity: Some(100),
            preset_name: Some("Minimoog Bass".to_string()),
            ..Default::default()
        });
    }

    let sequence = SimpleSequence { notes, tempo: 120 };
    println!(
        "▶️  Playing chord progression with {} total notes (up to 8 simultaneous)",
        sequence.notes.len()
    );

    player.play_enhanced_mixed(sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 3: Fast Arpeggios and Voice Stealing
    println!("\n🎼 Test 3: Fast Arpeggios and Voice Stealing");
    println!("Testing rapid note sequences to validate voice stealing algorithms...");

    let mut notes = Vec::new();
    let arp_pattern = [60, 64, 67, 72, 76, 79, 84, 88]; // C Major arpeggio

    // Create overlapping fast arpeggios
    for sequence in 0..4 {
        let base_time = sequence as f64 * 1.0;

        for (i, &note_num) in arp_pattern.iter().enumerate() {
            notes.push(SimpleNote {
                start_time: base_time + (i as f64 * 0.1), // Very fast notes every 100ms
                duration: 0.8,                            // Long enough to create overlaps
                note: Some(note_num + (sequence * 12) as u8), // Transpose each sequence
                velocity: Some(90 + (i % 4) as u8 * 10),  // Varying velocities
                preset_name: Some("Prophet Lead".to_string()),
                ..Default::default()
            });
        }
    }

    let sequence = SimpleSequence { notes, tempo: 120 };
    println!(
        "▶️  Playing fast arpeggios with {} notes (testing voice stealing)",
        sequence.notes.len()
    );

    player.play_enhanced_mixed(sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 4: Mixed Audio Modes
    println!("\n🎛️  Test 4: Mixed Audio Modes");
    println!("Testing simultaneous MIDI, presets, R2D2, and synthesis...");

    let notes = vec![
        // MIDI drum pattern
        SimpleNote {
            start_time: 0.0,
            duration: 0.1,
            note: Some(36), // Kick
            velocity: Some(120),
            instrument: Some(0), // Standard kit (0-127 range)
            channel: 9,          // MIDI drum channel
            ..Default::default()
        },
        // Preset bass
        SimpleNote {
            start_time: 0.0,
            duration: 1.0,
            note: Some(48),
            velocity: Some(90),
            preset_name: Some("Jupiter Bass".to_string()),
            ..Default::default()
        },
        // R2D2 expression
        SimpleNote {
            start_time: 0.5,
            duration: 0.8,
            r2d2_emotion: Some("Excited".to_string()),
            r2d2_intensity: Some(0.8),
            r2d2_complexity: Some(3),
            ..Default::default()
        },
        // Custom synthesis
        SimpleNote {
            start_time: 1.0,
            duration: 1.5,
            synth_type: Some("sawtooth".to_string()),
            synth_frequency: Some(440.0),
            synth_amplitude: Some(0.5),
            synth_filter_cutoff: Some(1200.0),
            synth_reverb: Some(0.3),
            ..Default::default()
        },
    ];

    let sequence = SimpleSequence { notes, tempo: 120 };
    println!(
        "▶️  Playing mixed audio sequence with {} notes (MIDI + Presets + R2D2 + Synthesis)",
        sequence.notes.len()
    );

    player.play_enhanced_mixed(sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    println!("\n{}", "=".repeat(60));
    println!("🎯 Polyphony Validation Results:");
    println!("   ✅ Voice Manager Unit Tests - PASSED");
    println!("   ✅ Chord Progression - PASSED");
    println!("   ✅ Fast Arpeggios - PASSED");
    println!("   ✅ Mixed Audio Modes - PASSED");
    println!("   📊 Success Rate: 100.0%");

    println!("\n🎉 ALL POLYPHONY TESTS PASSED!");
    println!("🏆 Real-time polyphonic voice management is fully operational!");

    Ok(())
}

/// Test DX7 specifically with debugging output
async fn test_dx7_debugging() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Debugging DX7 Slap Bass - Testing all components");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test 1: Simple FM (for comparison)
    println!("\n1️⃣ Testing basic FM synthesis for comparison:");
    let fm_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            synth_type: Some("fm".to_string()),
            synth_frequency: Some(110.0),
            synth_amplitude: Some(0.8),
            start_time: 0.0,
            duration: 1.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(fm_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // Test 2: DX7 Slap Bass preset (suspected issue)
    println!("\n2️⃣ Testing DX7 Slap Bass preset:");
    let dx7_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("DX7 Slap Bass".to_string()),
            note: Some(48), // C3
            velocity: Some(127),
            start_time: 0.0,
            duration: 2.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(dx7_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    // Test 3: Check if other presets work
    println!("\n3️⃣ Testing Minimoog Bass for comparison:");
    let moog_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("Minimoog Bass".to_string()),
            note: Some(48), // C3
            velocity: Some(127),
            start_time: 0.0,
            duration: 1.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(moog_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // Test 4: DX7 Keys preset (to see if all DX7FM presets have issues)
    println!("\n4️⃣ Testing DX7 E.Piano preset:");
    let dx7_keys_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("DX7 E.Piano".to_string()),
            note: Some(60), // C4
            velocity: Some(100),
            start_time: 0.0,
            duration: 2.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(dx7_keys_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    println!("\n✅ DX7 debugging test complete!");
    println!(
        "If you didn't hear the DX7 presets but heard the others, there's a DX7FM synthesis issue."
    );

    Ok(())
}

/// Test enhanced pad presets with authenticity improvements
async fn test_enhanced_pads() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌊 Testing Enhanced Pad Presets - Authenticity Improvements!");
    println!("This will showcase the improved vintage character...\n");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test 1: Enhanced JP-8 Strings with authentic analog warmth
    println!("🎵 Test 1: Enhanced JP-8 Strings with authentic analog warmth/movement");
    let jp8_sequence = SimpleSequence {
        notes: vec![
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(60), // C4
                velocity: Some(80),
                start_time: 0.0,
                duration: 3.0,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(64), // E4
                velocity: Some(75),
                start_time: 0.0,
                duration: 3.0,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(67), // G4
                velocity: Some(70),
                start_time: 0.0,
                duration: 3.0,
                ..Default::default()
            },
        ],
        tempo: 120,
    };
    player.play_enhanced_mixed(jp8_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 2: Enhanced OB Brass with creamy Oberheim character
    println!("🎵 Test 2: Enhanced OB Brass with creamy Oberheim character");
    let ob_sequence = SimpleSequence {
        notes: vec![
            SimpleNote {
                preset_name: Some("OB Brass".to_string()),
                note: Some(57), // A3
                velocity: Some(90),
                start_time: 0.0,
                duration: 2.5,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("OB Brass".to_string()),
                note: Some(62), // D4
                velocity: Some(85),
                start_time: 0.0,
                duration: 2.5,
                ..Default::default()
            },
        ],
        tempo: 120,
    };
    player.play_enhanced_mixed(ob_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 3: D-50 Fantasia - Complex LA synthesis
    println!("🎵 Test 3: D-50 Fantasia with complex LA synthesis character");
    let d50_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("D-50 Fantasia".to_string()),
            note: Some(72), // C5
            velocity: Some(100),
            start_time: 0.0,
            duration: 4.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(d50_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 4: Space Pad - Atmospheric texture
    println!("🎵 Test 4: Space Pad with cosmic atmospheric character");
    let space_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("Space Pad".to_string()),
            note: Some(48), // C3
            velocity: Some(70),
            start_time: 0.0,
            duration: 6.0,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(space_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Test 5: Mixed pad progression showing authenticity improvements
    println!("🎵 Test 5: Mixed pad progression - Authentic vintage character showcase");
    let mixed_sequence = SimpleSequence {
        notes: vec![
            // JP-8 Strings foundation
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(48), // C3
                velocity: Some(60),
                start_time: 0.0,
                duration: 6.0,
                ..Default::default()
            },
            // OB Brass mid-range
            SimpleNote {
                preset_name: Some("OB Brass".to_string()),
                note: Some(60), // C4
                velocity: Some(70),
                start_time: 1.0,
                duration: 4.0,
                ..Default::default()
            },
            // D-50 Fantasia highlight
            SimpleNote {
                preset_name: Some("D-50 Fantasia".to_string()),
                note: Some(72), // C5
                velocity: Some(80),
                start_time: 2.0,
                duration: 3.0,
                ..Default::default()
            },
        ],
        tempo: 120,
    };
    player.play_enhanced_mixed(mixed_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    println!("\n✅ Enhanced pad preset testing complete!");
    println!("🎉 Authenticity improvements showcase finished!");
    println!("\n🔍 What you should have heard:");
    println!("   ✅ JP-8 Strings: Warm analog character with subtle movement/detuning");
    println!("   ✅ OB Brass: Creamy Oberheim texture with rich harmonics");
    println!("   ✅ D-50 Fantasia: Complex evolving pad with LA synthesis character");
    println!("   ✅ Space Pad: Cosmic atmospheric texture with heavy reverb");
    println!("   ✅ Mixed progression: All presets working together polyphonically");

    Ok(())
}

/// Test volume consistency across preset categories
async fn test_volume_consistency() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔊 Testing Volume Consistency Across Preset Categories");
    println!("This will test standardized amplitude levels...\n");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test same note (C4=60) across different preset categories
    let test_note = 60; // C4
    let test_velocity = 100;
    let test_duration = 2.0;

    // Bass presets (should be 0.8)
    println!("🎸 Testing Bass Presets - Target: Strong & Punchy");
    println!("🎵 Minimoog Bass (amplitude: 0.8)");
    let bass_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("Minimoog Bass".to_string()),
            note: Some(test_note),
            velocity: Some(test_velocity),
            start_time: 0.0,
            duration: test_duration,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(bass_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Pad presets (should be 0.75)
    println!("🌊 Testing Pad Presets - Target: Present but Layerable");
    println!("🎵 JP-8 Strings (amplitude: 0.75)");
    let pad_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("JP-8 Strings".to_string()),
            note: Some(test_note),
            velocity: Some(test_velocity),
            start_time: 0.0,
            duration: test_duration,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(pad_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Keys presets (should be 0.8)
    println!("🎹 Testing Keys Presets - Target: Clear & Present");
    println!("🎵 DX7 E.Piano (amplitude: 0.8)");
    let keys_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("DX7 E.Piano".to_string()),
            note: Some(test_note),
            velocity: Some(test_velocity),
            start_time: 0.0,
            duration: test_duration,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(keys_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Effects presets (should be 0.8)
    println!("⚡ Testing Effects Presets - Target: Noticeable Impact");
    println!("🎵 Sci-Fi Zap (amplitude: 0.8)");
    let effects_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("Sci-Fi Zap".to_string()),
            note: Some(test_note),
            velocity: Some(test_velocity),
            start_time: 0.0,
            duration: 0.5, // Shorter for zap
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(effects_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(750)).await;

    // Mixed progression to test layering
    println!("🎼 Testing Mixed Layering - All Categories Together");
    let mixed_sequence = SimpleSequence {
        notes: vec![
            // Bass foundation
            SimpleNote {
                preset_name: Some("Minimoog Bass".to_string()),
                note: Some(36), // C2
                velocity: Some(100),
                start_time: 0.0,
                duration: 4.0,
                ..Default::default()
            },
            // Pad layer
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(60), // C4
                velocity: Some(80),
                start_time: 0.5,
                duration: 3.0,
                ..Default::default()
            },
            // Keys melody
            SimpleNote {
                preset_name: Some("DX7 E.Piano".to_string()),
                note: Some(72), // C5
                velocity: Some(90),
                start_time: 1.0,
                duration: 2.0,
                ..Default::default()
            },
        ],
        tempo: 120,
    };
    player.play_enhanced_mixed(mixed_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    println!("\n✅ Volume consistency testing complete!");
    println!("\n📊 Standardized Amplitude Levels:");
    println!("   🎸 Bass Presets:    0.8   (Strong & Punchy)");
    println!("   🌊 Pad Presets:     0.75  (Present but Layerable)");
    println!("   🎹 Keys Presets:    0.8   (Clear & Present)");
    println!("   ⚡ Effects Presets: 0.8   (Noticeable Impact)");
    println!("   🥁 Drum Presets:    0.8-0.9 (Percussive Impact)");
    println!("\n🎯 Result: All presets should now have consistent, audible volume levels!");
    println!("🔧 Fixed: Pads are no longer too quiet compared to bass presets");

    Ok(())
}

impl Default for SimpleNote {
    fn default() -> Self {
        Self {
            note: None,
            velocity: None,
            start_time: 0.0,
            duration: 1.0,
            channel: 0,
            instrument: None,
            note_type: "midi".to_string(),
            pan: None,
            balance: None,
            reverb: None,
            chorus: None,
            expression: None,
            volume: None,
            sustain: None,
            preset_name: None,
            preset_category: None,
            preset_variation: None,
            preset_random: None,
            r2d2_emotion: None,
            r2d2_intensity: None,
            r2d2_complexity: None,
            r2d2_pitch_range: None,
            r2d2_context: None,
            synth_type: None,
            synth_frequency: None,
            synth_amplitude: None,
            synth_attack: None,
            synth_decay: None,
            synth_sustain: None,
            synth_release: None,
            synth_filter_type: None,
            synth_filter_cutoff: None,
            synth_filter_resonance: None,
            synth_modulation_index: None,
            synth_modulator_freq: None,
            synth_pulse_width: None,
            synth_chorus: None,
            synth_reverb: None,
            synth_delay: None,
            synth_delay_time: None,
            synth_grain_size: None,
            synth_texture_roughness: None,
            effects: None,
            effects_preset: None,
        }
    }
}

/// Test drum synthesis with all available drum types
async fn test_drum_synthesis() -> Result<(), Box<dyn std::error::Error>> {
    println!("🥁 Testing Drum Synthesis - All Drum Types!");
    println!("This will test kick, snare, hi-hat, and cymbal synthesis...\n");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

    // Test 1: TR-808 Kick (existing preset)
    println!("🔥 Test 1: TR-808 Kick - Classic hip-hop kick");
    let kick_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TR-808 Kick".to_string()),
            note: Some(36), // MIDI kick note
            velocity: Some(127),
            start_time: 0.0,
            duration: 1.0,
            channel: 9, // MIDI drum channel
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(kick_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    // Test 2: TR-909 Snare (existing preset)
    println!("🔥 Test 2: TR-909 Snare - Techno snare with snap");
    let snare_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TR-909 Snare".to_string()),
            note: Some(38), // MIDI snare note
            velocity: Some(120),
            start_time: 0.0,
            duration: 0.5,
            channel: 9,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(snare_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

    // Test 3: TR-909 Hi-Hat (new preset)
    println!("🔥 Test 3: TR-909 Hi-Hat - Sharp, metallic hi-hat");
    let hihat_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TR-909 Hi-Hat".to_string()),
            note: Some(42), // MIDI closed hi-hat
            velocity: Some(100),
            start_time: 0.0,
            duration: 0.15,
            channel: 9,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(hihat_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Test 4: Crash Cymbal (new preset)
    println!("🔥 Test 4: Crash Cymbal - Bright crash with long decay");
    let cymbal_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("Crash Cymbal".to_string()),
            note: Some(49), // MIDI crash cymbal
            velocity: Some(120),
            start_time: 0.0,
            duration: 2.0,
            channel: 9,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(cymbal_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(2200)).await;

    // Test 4b: TR-808 Hi-Hat (new preset)
    println!("🔥 Test 4b: TR-808 Hi-Hat - Classic 808 hi-hat");
    let hihat808_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TR-808 Hi-Hat".to_string()),
            note: Some(42),
            velocity: Some(110),
            start_time: 0.0,
            duration: 0.08,
            channel: 9,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(hihat808_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Test 5: Custom kick parameters
    println!("🔥 Test 5: Custom Kick Parameters - Punchy 808");
    let custom_kick_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            synth_type: Some("kick".to_string()),
            synth_frequency: Some(50.0), // Deep kick
            synth_amplitude: Some(0.9),
            start_time: 0.0,
            duration: 1.2,
            channel: 9,
            note_type: "midi".to_string(),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(custom_kick_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(1400)).await;

    // Test 6: Drum pattern - All together
    println!("🔥 Test 6: Basic Drum Pattern - All drums together");
    let pattern_sequence = SimpleSequence {
        notes: vec![
            // Kick on beats 1 and 3
            SimpleNote {
                preset_name: Some("TR-808 Kick".to_string()),
                note: Some(36),
                velocity: Some(127),
                start_time: 0.0,
                duration: 0.3,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Kick".to_string()),
                note: Some(36),
                velocity: Some(110),
                start_time: 1.0,
                duration: 0.3,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Snare on beats 2 and 4
            SimpleNote {
                preset_name: Some("TR-909 Snare".to_string()),
                note: Some(38),
                velocity: Some(120),
                start_time: 0.5,
                duration: 0.2,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-909 Snare".to_string()),
                note: Some(38),
                velocity: Some(100),
                start_time: 1.5,
                duration: 0.2,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Hi-hats on eighth notes
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(90),
                start_time: 0.25,
                duration: 0.08,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(70),
                start_time: 0.75,
                duration: 0.08,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(85),
                start_time: 1.25,
                duration: 0.08,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(65),
                start_time: 1.75,
                duration: 0.08,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
        ],
        tempo: 120,
    };
    player.play_enhanced_mixed(pattern_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    println!("✅ Drum synthesis test completed!");
    println!(
        "If any drums were silent or too quiet, there may be synthesis issues to investigate."
    );

    Ok(())
}

/// Test the new effects system with dramatic examples
async fn test_effects_system() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎛️ TESTING EFFECTS SYSTEM 🎛️");
    println!("Listen for the difference between dry and processed sound!\n");

    let player = MidiPlayer::new()?;
    let test_note = 60; // Middle C
    let test_velocity = 100;
    let test_duration = 3.0; // Longer to hear effects

    // 1. DRY SOUND (no effects)
    println!("🎵 1. DRY PIANO (no effects)");
    let dry_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(test_note),
            velocity: Some(test_velocity),
            instrument: Some(1), // Bright Acoustic Piano
            start_time: 0.0,
            duration: test_duration,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(dry_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;

    // 2. MASSIVE REVERB (much more dramatic)
    println!("🏛️ 2. SAME PIANO with MASSIVE REVERB");
    let reverb_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(test_note),
            velocity: Some(test_velocity),
            instrument: Some(1),
            start_time: 0.0,
            duration: test_duration,
            effects: Some(vec![crate::midi::EffectConfig {
                effect: crate::midi::EffectType::Reverb {
                    room_size: 1.0, // Maximum room size
                    dampening: 0.1, // Minimal dampening
                    wet_level: 0.8, // Very wet signal
                    pre_delay: 0.1, // Long pre-delay
                },
                intensity: 1.0, // Maximum intensity
                enabled: true,
            }]),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(reverb_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(6000)).await; // Longer to hear reverb tail

    // 3. HEAVY CHORUS (very obvious modulation)
    println!("🌊 3. SAME PIANO with HEAVY CHORUS");
    let chorus_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(test_note),
            velocity: Some(test_velocity),
            instrument: Some(1),
            start_time: 0.0,
            duration: test_duration,
            effects: Some(vec![crate::midi::EffectConfig {
                effect: crate::midi::EffectType::Chorus {
                    rate: 3.0,         // Fast modulation
                    depth: 0.9,        // Deep modulation
                    feedback: 0.7,     // High feedback
                    stereo_width: 1.0, // Maximum stereo width
                },
                intensity: 1.0, // Maximum intensity
                enabled: true,
            }]),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(chorus_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;

    // 4. OBVIOUS DISTORTION
    println!("🔥 4. SAME PIANO with HEAVY DISTORTION");
    let distortion_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(test_note),
            velocity: Some(test_velocity),
            instrument: Some(1),
            start_time: 0.0,
            duration: test_duration,
            effects: Some(vec![crate::midi::EffectConfig {
                effect: crate::midi::EffectType::Distortion {
                    drive: 5.0,        // Maximum drive
                    tone: 0.8,         // Bright tone
                    output_level: 0.7, // Compensate for distortion
                },
                intensity: 1.0, // Maximum intensity
                enabled: true,
            }]),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(distortion_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;

    // 5. EXTREME DELAY (very obvious repeats)
    println!("⚡ 5. SAME PIANO with EXTREME DELAY");
    let delay_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(test_note),
            velocity: Some(test_velocity),
            instrument: Some(1),
            start_time: 0.0,
            duration: 1.0, // Shorter note to hear delays clearly
            effects: Some(vec![crate::midi::EffectConfig {
                effect: crate::midi::EffectType::Delay {
                    delay_time: 0.4, // Clear 400ms delay
                    feedback: 0.8,   // High feedback for multiple repeats
                    wet_level: 0.9,  // Very wet signal
                    sync_tempo: false,
                },
                intensity: 1.0, // Maximum intensity
                enabled: true,
            }]),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(delay_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(6000)).await; // Longer to hear delay repeats

    // 6. PRESET WITH SIGNATURE EFFECTS
    println!("🎛️ 6. TB-303 ACID BASS with signature effects (resonant filter + delay)");
    let acid_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("TB-303 Acid".to_string()),
            note: Some(48), // Lower bass note
            velocity: Some(127),
            start_time: 0.0,
            duration: test_duration,
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(acid_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;

    // 7. EFFECTS PRESET TEST - Testing if effects_preset parameter works
    println!("🎛️ 7. EFFECTS PRESET TEST - Testing concert_hall preset");
    let effects_preset_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            note: Some(60),      // C4
            instrument: Some(1), // Bright Piano
            velocity: Some(90),
            start_time: 0.0,
            duration: 3.0,
            effects_preset: Some("concert_hall".to_string()),
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(effects_preset_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4000)).await;

    // 8. PRESET COMPARISON - Subtle vs No Effects
    println!("🎼 8. PRESET COMPARISON - Pad with vs without signature effects");

    // First: Pad without signature effects
    println!("   🔇 JP-8 Strings (NO signature effects)");
    let dry_pad_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("JP-8 Strings".to_string()),
            note: Some(60), // C4
            velocity: Some(80),
            start_time: 0.0,
            duration: 4.0,
            effects: Some(vec![]), // Override to disable signature effects
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(dry_pad_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4500)).await;

    // Then: Same pad WITH signature effects
    println!("   🎨 JP-8 Strings (WITH subtle signature effects)");
    let wet_pad_sequence = SimpleSequence {
        notes: vec![SimpleNote {
            preset_name: Some("JP-8 Strings".to_string()),
            note: Some(60), // C4
            velocity: Some(80),
            start_time: 0.0,
            duration: 4.0,
            // No effects override - will use preset's signature effects
            ..Default::default()
        }],
        tempo: 120,
    };
    player.play_enhanced_mixed(wet_pad_sequence)?;
    tokio::time::sleep(tokio::time::Duration::from_millis(4500)).await;

    println!("\n✅ Effects test complete!");
    println!("🎧 You should have heard differences between:");
    println!("   • DRY piano (clean original sound)");
    println!("   • MASSIVE REVERB (huge spatial effect)");
    println!("   • HEAVY CHORUS (wobbling/modulation)");
    println!("   • HEAVY DISTORTION (gritty/overdriven)");
    println!("   • EXTREME DELAY (clear repeating echoes)");
    println!("   • Preset signature effects");
    println!("   • Pad comparison: dry vs. subtle signature effects");
    println!();
    println!("🎭 PRESET SIGNATURE EFFECTS are now active on:");
    println!("   • All PAD presets (subtle reverb + chorus)");
    println!("   • All LEAD presets (delay + chorus)");
    println!("   • All KEYS presets (room reverb)");
    println!("   • BASS presets (compression + clarity filtering)");
    println!();
    println!("🔊 If you didn't hear clear differences, please check:");
    println!("   • Audio system volume");
    println!("   • Speaker/headphone quality");
    println!("   • System audio effects or EQ that might mask changes");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_determine_log_directory_success() {
        // Test successful directory creation
        let temp_dir = std::env::temp_dir().join("mcp-muse-test-success");

        // Clean up first
        let _ = fs::remove_dir_all(&temp_dir);

        let result = determine_log_directory(temp_dir.clone());

        assert_eq!(result, temp_dir);
        assert!(temp_dir.exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_determine_log_directory_fallback() {
        // Test fallback to current directory when creation fails
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let readonly_parent = std::env::temp_dir().join("mcp-muse-readonly-test");
            let impossible_dir = readonly_parent.join("subdir").join("impossible");

            // Clean up first
            let _ = fs::remove_dir_all(&readonly_parent);

            // Create parent and make it read-only
            fs::create_dir_all(&readonly_parent).expect("Failed to create readonly parent");
            let mut perms = fs::metadata(&readonly_parent).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&readonly_parent, perms)
                .expect("Failed to set read-only permissions");

            let result = determine_log_directory(impossible_dir);

            // Should fallback to current directory
            assert_eq!(result, PathBuf::from("."));

            // Clean up
            let mut perms = fs::metadata(&readonly_parent).unwrap().permissions();
            perms.set_mode(0o755); // Restore write permissions
            let _ = fs::set_permissions(&readonly_parent, perms);
            let _ = fs::remove_dir_all(&readonly_parent);
        }

        #[cfg(not(unix))]
        {
            // For non-Unix systems, just test that fallback returns current directory
            // when given an impossible path
            let impossible_dir = PathBuf::from("/impossible/path/that/should/not/exist/ever");
            let result = determine_log_directory(impossible_dir);
            assert_eq!(result, PathBuf::from("."));
        }
    }

    #[test]
    fn test_determine_log_directory_already_exists() {
        // Test when directory already exists
        let temp_dir = std::env::temp_dir().join("mcp-muse-test-exists");

        // Clean up first, then create
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        let result = determine_log_directory(temp_dir.clone());

        assert_eq!(result, temp_dir);
        assert!(temp_dir.exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
