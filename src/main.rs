#![recursion_limit = "512"]

use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod expressive;
mod midi;
mod server;
mod setup;

use crate::midi::{SimpleNote, SimpleSequence};

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
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run setup for MCP hosts
    #[arg(long)]
    setup: bool,

    /// Test preset integration
    #[arg(long)]
    test_presets: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    let args = Args::parse();

    if args.setup {
        setup::run_setup();
        return Ok(());
    }

    if args.test_presets {
        test_preset_integration().await?;
        return Ok(());
    }

    tracing::info!("Starting MCP MIDI Server (stdio mode)...");
    server::run_stdio_server();
    Ok(())
}

/// Test the preset integration with actual audio playback
async fn test_preset_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎹 Testing Classic Synthesizer Preset Integration!");
    println!("This will test the complete audio pipeline with presets...\n");

    let player =
        midi::MidiPlayer::new().map_err(|e| format!("Failed to create MIDI player: {}", e))?;

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
        }
    }
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
