use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const CONFIG_FILE: &str = ".mcp_muse_config.json";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SetupConfig {
    pub hosts: Vec<HostConfig>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct HostConfig {
    pub name: String,
    pub endpoint: String,
    pub enabled: bool,
}

impl SetupConfig {
    pub fn config_path() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_FILE)
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
        let config: SetupConfig = serde_json::from_str(&content)
            .unwrap_or_default();
        Ok(config)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self).unwrap();
        let mut file = fs::File::create(path)?;
        file.write_all(content.as_bytes())
    }

    pub fn upsert_host(&mut self, host: HostConfig) {
        if let Some(existing) = self.hosts.iter_mut().find(|h| h.name == host.name) {
            *existing = host;
        } else {
            self.hosts.push(host);
        }
    }
} 