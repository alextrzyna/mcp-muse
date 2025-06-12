use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod server;
mod setup;
mod midi;

fn init_logging() {
    // macOS Application Support path
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcp-muse");
    let _ = fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(
        &log_dir,
        "mcp-muse.log"
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .init();
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
