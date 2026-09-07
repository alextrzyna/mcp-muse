#![recursion_limit = "512"]

use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing_subscriber::EnvFilter;

mod demos;
mod expressive;
mod midi;
mod server;
mod setup;

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

    // Panics normally go only to stderr, which MCP hosts may hide; mirror them
    // into the log file before the default hook prints them.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {}", info);
        default_hook(info);
    }));

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

    /// Run setup (same as the `setup` subcommand)
    #[arg(long)]
    setup: bool,
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

    /// Play every built-in synth patch (listen-by-ear check)
    #[command(name = "test-synths")]
    TestSynths,

    /// Play the synthesized drum patches
    #[command(name = "test-drums")]
    TestDrums,

    /// Play a MIDI piano dry, then through effect chains
    #[command(name = "test-effects")]
    TestEffects,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    let args = Args::parse();

    let command = if args.setup {
        Some(Commands::Setup)
    } else {
        args.command
    };

    match command {
        Some(Commands::Server { name: _ }) => {
            tracing::info!("Starting MCP MIDI Server (stdio mode)...");
            server::run_stdio_server();
        }
        Some(Commands::Setup) => {
            setup::run_setup();
        }
        Some(Commands::TestSynths) => {
            demos::test_synths()?;
        }
        Some(Commands::TestDrums) => {
            demos::test_drums()?;
        }
        Some(Commands::TestEffects) => {
            demos::test_effects()?;
        }
        None => {
            // Default behavior: start the MCP server
            tracing::info!("Starting MCP MIDI Server (stdio mode)...");
            server::run_stdio_server();
        }
    }

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
