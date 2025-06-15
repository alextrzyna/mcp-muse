#![recursion_limit = "512"]

use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod expressive;
mod midi;
mod server;
mod setup;

fn init_logging() {
    // Cross-platform data directory (macOS: ~/Library/Application Support, Linux: ~/.local/share, Windows: %APPDATA%)
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcp-muse");

    // Create directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!(
            "Warning: Could not create log directory {:?}: {}",
            log_dir, e
        );
        eprintln!("Falling back to current directory for logging");
    }

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
}

fn main() -> anyhow::Result<()> {
    init_logging();
    let args = Args::parse();

    if args.setup {
        setup::run_setup();
        return Ok(());
    }

    tracing::info!("Starting MCP MIDI Server (stdio mode)...");
    server::run_stdio_server();
    Ok(())
}
