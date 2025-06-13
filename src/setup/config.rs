use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SetupConfig {
    pub hosts: Vec<HostConfig>,
    pub soundfont_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct HostConfig {
    pub name: String,
    pub endpoint: String,
    pub enabled: bool,
}

impl SetupConfig {
    pub fn config_path() -> PathBuf {
        // Cross-platform data directory:
        // - Linux: $XDG_DATA_HOME or $HOME/.local/share
        // - macOS: $HOME/Library/Application Support
        // - Windows: %APPDATA% (RoamingAppData)
        let config_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp-muse");

        // Ensure the directory exists
        if let Err(e) = fs::create_dir_all(&config_dir) {
            eprintln!(
                "Warning: Could not create config directory {:?}: {}",
                config_dir, e
            );
            eprintln!("Configuration may not be saved properly");
        }

        config_dir.join(CONFIG_FILE)
    }

    pub fn load() -> io::Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(SetupConfig::default());
        }
        let content = fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(SetupConfig::default());
        }
        let config: SetupConfig = serde_json::from_str(&content).unwrap_or_default();
        Ok(config)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self).unwrap();
        let mut file = fs::File::create(path)?;
        file.write_all(content.as_bytes())
    }
}
