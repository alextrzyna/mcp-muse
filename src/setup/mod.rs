// setup module placeholder 

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::env;

const CURSOR_CONFIG_PATH: &str = ".cursor/mcp.json";
const SERVER_NAME: &str = "mcp-muse";

// Use the current executable path instead of a hardcoded path
fn get_server_command() -> String {
    env::current_exe().unwrap_or_else(|_| PathBuf::from("mcp-muse")).to_string_lossy().to_string()
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CursorConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, CursorServerEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct CursorServerEntry {
    transport: String,
    command: String,
    enabled: bool,
}

pub fn run_setup() {
    let config_path = match dirs::home_dir() {
        Some(home) => home.join(CURSOR_CONFIG_PATH),
        None => {
            eprintln!("Could not determine home directory.");
            return;
        }
    };

    let mut config: CursorConfig = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) if !content.trim().is_empty() => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            _ => CursorConfig::default(),
        }
    } else {
        CursorConfig::default()
    };

    let muse_entry = CursorServerEntry {
        transport: "stdio".to_string(),
        command: get_server_command(),
        enabled: true,
    };

    let mut updated = false;
    match config.mcp_servers.get(SERVER_NAME) {
        Some(existing) if existing == &muse_entry => {
            println!("mcp-muse entry already up to date in Cursor config.");
        }
        _ => {
            config.mcp_servers.insert(SERVER_NAME.to_string(), muse_entry);
            updated = true;
            println!("Added or updated mcp-muse entry in Cursor config.");
        }
    }

    if updated {
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match fs::File::create(&config_path).and_then(|mut f| {
            let content = serde_json::to_string_pretty(&config).unwrap();
            f.write_all(content.as_bytes())
        }) {
            Ok(_) => println!("Saved Cursor MCP config to {:?}", config_path),
            Err(e) => eprintln!("Failed to save Cursor MCP config: {}", e),
        }
    }
} 