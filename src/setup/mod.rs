// setup module placeholder 

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Write, Read};
use std::path::PathBuf;
use std::env;

const CURSOR_CONFIG_PATH: &str = ".cursor/mcp.json";
const SERVER_NAME: &str = "mcp-muse";
const SOUNDFONT_URL: &str = "https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip";
const SOUNDFONT_FILENAME: &str = "FluidR3_GM.sf2";

// Use the current executable path instead of a hardcoded path
fn get_server_command() -> String {
    env::current_exe().unwrap_or_else(|_| PathBuf::from("mcp-muse")).to_string_lossy().to_string()
}

fn get_assets_dir() -> anyhow::Result<PathBuf> {
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("Cannot find executable directory"))?;
    
    // Try relative to executable first (for development)
    let dev_assets = exe_dir.join("../assets");
    if dev_assets.exists() {
        return Ok(dev_assets);
    }
    
    // Try relative to executable (for installed version)
    let assets = exe_dir.join("assets");
    if assets.exists() {
        return Ok(assets);
    }
    
    // Create assets directory next to executable
    fs::create_dir_all(&assets)?;
    Ok(assets)
}

fn download_soundfont() -> anyhow::Result<()> {
    let assets_dir = get_assets_dir()?;
    let soundfont_path = assets_dir.join(SOUNDFONT_FILENAME);
    
    if soundfont_path.exists() {
        println!("✓ SoundFont already exists at {:?}", soundfont_path);
        return Ok(());
    }
    
    println!("📥 Downloading FluidR3_GM SoundFont...");
    println!("   This may take a moment (downloading 130MB ZIP file)...");
    
    // Create a client with longer timeout for large file
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minutes timeout
        .build()?;
    
    let response = client.get(SOUNDFONT_URL).send()?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to download SoundFont: HTTP {}", response.status()));
    }
    
    let zip_content = response.bytes()?;
    
    println!("📦 Extracting SoundFont from ZIP ({:.1} MB)...", zip_content.len() as f64 / 1024.0 / 1024.0);
    
    // Extract the SF2 file from the ZIP
    let cursor = std::io::Cursor::new(zip_content);
    let mut archive = zip::ZipArchive::new(cursor)?;
    
    // Look for the SF2 file in the archive
    let mut sf2_found = false;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = file.name().to_string();
        
        if file_name.ends_with(".sf2") || file_name.ends_with(".SF2") {
            println!("   Found SoundFont: {}", file_name);
            
            let mut sf2_content = Vec::new();
            file.read_to_end(&mut sf2_content)?;
            
            // Verify it's actually a SoundFont by checking the header
            if sf2_content.len() < 12 || &sf2_content[0..4] != b"RIFF" || &sf2_content[8..12] != b"sfbk" {
                return Err(anyhow::anyhow!("Extracted file is not a valid SoundFont"));
            }
            
            println!("   Writing SoundFont to disk ({:.1} MB)...", sf2_content.len() as f64 / 1024.0 / 1024.0);
            fs::write(&soundfont_path, sf2_content)?;
            sf2_found = true;
            break;
        }
    }
    
    if !sf2_found {
        return Err(anyhow::anyhow!("No SF2 file found in the ZIP archive"));
    }
    
    println!("✓ SoundFont extracted successfully to {:?}", soundfont_path);
    println!("  Size: {:.1} MB", soundfont_path.metadata()?.len() as f64 / 1024.0 / 1024.0);
    
    Ok(())
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
    println!("🎵 Setting up MCP Muse...\n");
    
    // Step 1: Download SoundFont
    println!("Step 1: Setting up SoundFont");
    match download_soundfont() {
        Ok(()) => println!("✓ SoundFont setup complete\n"),
        Err(e) => {
            eprintln!("❌ Failed to download SoundFont: {}", e);
            eprintln!("   Manual setup:");
            eprintln!("   1. Download: https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip");
            eprintln!("   2. Extract FluidR3_GM.sf2 to the assets/ folder");
            eprintln!("   3. Or run: curl -L https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip | funzip > assets/FluidR3_GM.sf2\n");
        }
    }
    
    // Step 2: Setup Cursor MCP configuration
    println!("Step 2: Setting up Cursor MCP configuration");
    setup_cursor_config();
    
    println!("\n🎉 Setup complete! You can now use MCP Muse in Cursor.");
    println!("   Try asking: \"Can you play a simple MIDI melody?\"");
}

fn setup_cursor_config() {
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
            println!("✓ mcp-muse entry already up to date in Cursor config.");
        }
        _ => {
            config.mcp_servers.insert(SERVER_NAME.to_string(), muse_entry);
            updated = true;
            println!("✓ Added or updated mcp-muse entry in Cursor config.");
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
            Ok(_) => println!("✓ Saved Cursor MCP config to {:?}", config_path),
            Err(e) => eprintln!("❌ Failed to save Cursor MCP config: {}", e),
        }
    }
} 