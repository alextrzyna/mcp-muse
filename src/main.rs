#![recursion_limit = "512"]

use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing_subscriber::EnvFilter;

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

/// Remove rotated log files (`mcp-muse.log.*`) older than `max_age`.
/// Returns the number of files removed. Never touches non-log files.
fn prune_old_logs(dir: &Path, max_age: Duration) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("mcp-muse.log.") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let is_old = now
            .duration_since(modified)
            .map(|age| age > max_age)
            .unwrap_or(false);
        if is_old && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Default retention for rotated log files.
const LOG_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn init_logging() {
    // Cross-platform data directory (macOS: ~/Library/Application Support, Linux: ~/.local/share, Windows: %APPDATA%)
    let preferred_log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcp-muse");

    let log_dir = determine_log_directory(preferred_log_dir);
    let pruned = prune_old_logs(&log_dir, LOG_RETENTION);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "mcp-muse.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Level is controlled by MCP_MUSE_LOG (EnvFilter syntax, e.g. "debug" or
    // "mcp_muse::midi=trace"). Defaults to INFO so playback never floods the disk.
    let filter = EnvFilter::try_from_env("MCP_MUSE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(filter)
        .init();

    // Log the directory being used for transparency
    tracing::info!(
        "Logging to directory: {:?} (pruned {} old files)",
        log_dir,
        pruned
    );

    // _guard must be kept alive, so we leak it (ok for a server)
    std::mem::forget(_guard);
}

#[derive(Parser, Debug)]
#[command(
    author = "mcp-muse team",
    version,
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
            start_time: Some(0.0),
            duration: Some(3.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(2.0),
            musical_time: None,
            musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(1.0),
                musical_time: None,
                musical_duration: None,
                channel: 0,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("Minimoog Bass".to_string()),
                note: Some(43), // G2
                velocity: Some(90),
                start_time: Some(1.0),
                duration: Some(1.0),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.5),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(2.0),
            musical_time: None,
            musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(2.0),
                musical_time: None,
                musical_duration: None,
                channel: 0,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Pad
            SimpleNote {
                preset_category: Some("pad".to_string()),
                note: Some(60), // C4
                velocity: Some(80),
                start_time: Some(0.5),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
                channel: 1,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_category: Some("pad".to_string()),
                note: Some(64), // E4
                velocity: Some(75),
                start_time: Some(0.5),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
                channel: 1,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Random preset
            SimpleNote {
                preset_random: Some(true),
                note: Some(72), // C5
                velocity: Some(90),
                start_time: Some(1.0),
                duration: Some(1.0),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(2.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(2.0),
            musical_time: None,
            musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(64), // E4
                velocity: Some(75),
                start_time: Some(0.0),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(67), // G4
                velocity: Some(70),
                start_time: Some(0.0),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(2.5),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("OB Brass".to_string()),
                note: Some(62), // D4
                velocity: Some(85),
                start_time: Some(0.0),
                duration: Some(2.5),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(4.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(6.0),
            musical_time: None,
            musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(6.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            // OB Brass mid-range
            SimpleNote {
                preset_name: Some("OB Brass".to_string()),
                note: Some(60), // C4
                velocity: Some(70),
                start_time: Some(1.0),
                duration: Some(4.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            // D-50 Fantasia highlight
            SimpleNote {
                preset_name: Some("D-50 Fantasia".to_string()),
                note: Some(72), // C5
                velocity: Some(80),
                start_time: Some(2.0),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(0.5), // Shorter for zap
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
                start_time: Some(0.0),
                duration: Some(4.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            // Pad layer
            SimpleNote {
                preset_name: Some("JP-8 Strings".to_string()),
                note: Some(60), // C4
                velocity: Some(80),
                start_time: Some(0.5),
                duration: Some(3.0),
                musical_time: None,
                musical_duration: None,
                ..Default::default()
            },
            // Keys melody
            SimpleNote {
                preset_name: Some("DX7 E.Piano".to_string()),
                note: Some(72), // C5
                velocity: Some(90),
                start_time: Some(1.0),
                duration: Some(2.0),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(0.5),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(0.15),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(2.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(0.08),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(1.2),
            musical_time: None,
            musical_duration: None,
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
                start_time: Some(0.0),
                duration: Some(0.3),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Kick".to_string()),
                note: Some(36),
                velocity: Some(110),
                start_time: Some(1.0),
                duration: Some(0.3),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Snare on beats 2 and 4
            SimpleNote {
                preset_name: Some("TR-909 Snare".to_string()),
                note: Some(38),
                velocity: Some(120),
                start_time: Some(0.5),
                duration: Some(0.2),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-909 Snare".to_string()),
                note: Some(38),
                velocity: Some(100),
                start_time: Some(1.5),
                duration: Some(0.2),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            // Hi-hats on eighth notes
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(90),
                start_time: Some(0.25),
                duration: Some(0.08),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(70),
                start_time: Some(0.75),
                duration: Some(0.08),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(85),
                start_time: Some(1.25),
                duration: Some(0.08),
                musical_time: None,
                musical_duration: None,
                channel: 9,
                note_type: "midi".to_string(),
                ..Default::default()
            },
            SimpleNote {
                preset_name: Some("TR-808 Hi-Hat".to_string()),
                note: Some(42),
                velocity: Some(65),
                start_time: Some(1.75),
                duration: Some(0.08),
                musical_time: None,
                musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(1.0), // Shorter note to hear delays clearly
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
            start_time: Some(0.0),
            duration: Some(test_duration),
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
            start_time: Some(0.0),
            duration: Some(3.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(4.0),
            musical_time: None,
            musical_duration: None,
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
            start_time: Some(0.0),
            duration: Some(4.0),
            musical_time: None,
            musical_duration: None,
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
    fn test_prune_old_logs_removes_only_old_rotated_files() {
        let temp_dir = std::env::temp_dir().join("mcp-muse-test-prune");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let old_log = temp_dir.join("mcp-muse.log.2020-01-01");
        let new_log = temp_dir.join("mcp-muse.log.2099-01-01");
        let config = temp_dir.join("config.json");
        for path in [&old_log, &new_log, &config] {
            fs::write(path, "x").unwrap();
        }
        let ancient = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(&old_log)
            .unwrap()
            .set_modified(ancient)
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&config)
            .unwrap()
            .set_modified(ancient)
            .unwrap();

        let removed = prune_old_logs(&temp_dir, std::time::Duration::from_secs(7 * 86_400));

        assert_eq!(removed, 1);
        assert!(!old_log.exists(), "old rotated log should be removed");
        assert!(new_log.exists(), "recent log must be kept");
        assert!(config.exists(), "non-log files must never be touched");

        let _ = fs::remove_dir_all(&temp_dir);
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
