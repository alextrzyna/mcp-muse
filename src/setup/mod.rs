// setup module placeholder

pub mod config;

use config::SetupConfig;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

const CURSOR_CONFIG_PATH: &str = ".cursor/mcp.json";
const SERVER_NAME: &str = "mcp-muse";
const SOUNDFONT_URL: &str = "https://keymusician01.s3.amazonaws.com/FluidR3_GM.zip";
const SOUNDFONT_FILENAME: &str = "FluidR3_GM.sf2";

/// Get the platform-specific data directory path for display purposes
fn get_data_dir_info() -> String {
    if let Some(data_dir) = dirs::data_dir() {
        let mcp_muse_dir = data_dir.join("mcp-muse");
        format!(
            "Platform data directory: {:?}\n   (Linux: ~/.local/share, macOS: ~/Library/Application Support, Windows: %APPDATA%)",
            mcp_muse_dir
        )
    } else {
        "Platform data directory: Current directory (.)".to_string()
    }
}

// Use the current executable path instead of a hardcoded path
fn get_server_command() -> String {
    env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("mcp-muse"))
        .to_string_lossy()
        .to_string()
}

fn get_assets_dir() -> anyhow::Result<PathBuf> {
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot find executable directory"))?;

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

fn ask_permission(message: &str) -> bool {
    print!("{} (y/N): ", message);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

fn get_custom_soundfont_path() -> Option<PathBuf> {
    print!("Enter custom SoundFont path (leave empty to use default): ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    let path = input.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn download_soundfont() -> anyhow::Result<()> {
    let assets_dir = get_assets_dir()?;
    let soundfont_path = assets_dir.join(SOUNDFONT_FILENAME);

    if soundfont_path.exists() {
        println!("✓ SoundFont already exists at {:?}", soundfont_path);
        return Ok(());
    }

    println!("📥 SoundFont Download Information:");
    println!("   Source URL: {}", SOUNDFONT_URL);
    println!("   File size: ~130MB (ZIP archive)");
    println!("   Destination: {:?}", soundfont_path);
    println!("   Description: FluidR3_GM professional quality SoundFont for MIDI synthesis");
    println!();

    if !ask_permission("Do you want to download the SoundFont from the above location?") {
        return Err(anyhow::anyhow!("SoundFont download cancelled by user"));
    }

    println!("📥 Downloading FluidR3_GM SoundFont...");
    println!("   This may take a moment (downloading 130MB ZIP file)...");

    // Create a client with longer timeout for large file
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minutes timeout
        .build()?;

    let response = client.get(SOUNDFONT_URL).send()?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download SoundFont: HTTP {}",
            response.status()
        ));
    }

    let zip_content = response.bytes()?;

    println!(
        "📦 Extracting SoundFont from ZIP ({:.1} MB)...",
        zip_content.len() as f64 / 1024.0 / 1024.0
    );

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
            if sf2_content.len() < 12
                || &sf2_content[0..4] != b"RIFF"
                || &sf2_content[8..12] != b"sfbk"
            {
                return Err(anyhow::anyhow!("Extracted file is not a valid SoundFont"));
            }

            println!(
                "   Writing SoundFont to disk ({:.1} MB)...",
                sf2_content.len() as f64 / 1024.0 / 1024.0
            );
            fs::write(&soundfont_path, sf2_content)?;
            sf2_found = true;
            break;
        }
    }

    if !sf2_found {
        return Err(anyhow::anyhow!("No SF2 file found in the ZIP archive"));
    }

    println!("✓ SoundFont extracted successfully to {:?}", soundfont_path);
    println!(
        "  Size: {:.1} MB",
        soundfont_path.metadata()?.len() as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

pub fn run_setup() {
    println!("🎵 Setting up MCP Muse...\n");

    // Load or create setup configuration
    let mut config = SetupConfig::load().unwrap_or_default();

    // Step 1: Check for custom SoundFont path preference
    println!("Step 1: SoundFont Configuration");

    // Check if user wants to specify a custom soundfont location
    if ask_permission("Would you like to specify a custom SoundFont file location?") {
        if let Some(custom_path) = get_custom_soundfont_path() {
            if custom_path.exists() {
                println!("✓ Custom SoundFont path configured: {:?}", custom_path);
                // Save custom soundfont path to config
                config.soundfont_path = Some(custom_path.to_string_lossy().to_string());
                config
                    .save()
                    .unwrap_or_else(|e| eprintln!("Warning: Could not save config: {}", e));

                // Skip the download step since we have a custom soundfont
                println!("✓ Using custom SoundFont, skipping download step\n");
            } else {
                eprintln!("❌ Custom SoundFont file not found at {:?}", custom_path);
                eprintln!("   Falling back to default download.");
                // Clear any previously saved custom path
                config.soundfont_path = None;
                config
                    .save()
                    .unwrap_or_else(|e| eprintln!("Warning: Could not save config: {}", e));
            }
        }
    } else {
        // If user doesn't want custom path, clear any previously saved custom path
        if config.soundfont_path.is_some() {
            config.soundfont_path = None;
            config
                .save()
                .unwrap_or_else(|e| eprintln!("Warning: Could not save config: {}", e));
        }
    }

    // Step 2: Download SoundFont (if not using custom)
    println!("\nStep 2: SoundFont Setup");

    // Check if we have a valid custom soundfont configured
    let skip_download = if let Some(custom_path) = &config.soundfont_path {
        let path = PathBuf::from(custom_path);
        if path.exists() {
            println!("✓ Using configured custom SoundFont: {:?}", path);
            true
        } else {
            println!("⚠️  Configured custom SoundFont not found: {:?}", path);
            println!("   Proceeding with default download...");
            false
        }
    } else {
        false
    };

    if !skip_download {
        match download_soundfont() {
            Ok(()) => println!("✓ SoundFont setup complete\n"),
            Err(e) => {
                eprintln!("❌ Failed to setup SoundFont: {}", e);
                eprintln!("   Manual setup options:");
                eprintln!("   1. Download: {}", SOUNDFONT_URL);
                eprintln!("   2. Extract FluidR3_GM.sf2 to the assets/ folder");
                eprintln!(
                    "   3. Or run: curl -L {} | funzip > assets/{}",
                    SOUNDFONT_URL, SOUNDFONT_FILENAME
                );
                eprintln!("   4. Or specify a custom SoundFont path in the next run\n");
            }
        }
    }

    // Step 3: Setup Cursor MCP configuration
    println!("Step 3: MCP Host Configuration");
    println!("This will configure Cursor to use the MCP Muse server.");
    println!("Configuration file: ~/.cursor/mcp.json");
    println!();

    if ask_permission("Do you want to configure Cursor MCP integration?") {
        setup_cursor_config();
    } else {
        println!("⏭️  Cursor configuration skipped.");
        println!("   You can run setup again later or manually configure:");
        println!("   Add this to ~/.cursor/mcp.json:");
        println!("   {{");
        println!("     \"mcpServers\": {{");
        println!("       \"{}\": {{", SERVER_NAME);
        println!("         \"transport\": \"stdio\",");
        println!("         \"command\": \"{}\",", get_server_command());
        println!("         \"enabled\": true");
        println!("       }}");
        println!("     }}");
        println!("   }}");
    }

    println!("\n🎉 Setup process complete!");
    println!("   Next steps:");
    println!("   1. Restart Cursor if you configured it");
    println!("   2. Try asking: \"Can you play a simple MIDI melody?\"");
    println!("   3. For help: Run with --help or check the documentation");
    println!();
    println!("   {}", get_data_dir_info());
    println!("   Configuration file: {:?}", SetupConfig::config_path());
    println!("   Log files will be stored in the same directory");
}

fn setup_cursor_config() {
    let config_path = match dirs::home_dir() {
        Some(home) => home.join(CURSOR_CONFIG_PATH),
        None => {
            eprintln!("Could not determine home directory.");
            return;
        }
    };

    // Read existing config as generic JSON to preserve all fields
    let mut config_value: serde_json::Value = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) if !content.trim().is_empty() => {
                match serde_json::from_str(&content) {
                    Ok(value) => value,
                    Err(e) => {
                        eprintln!("⚠️  Warning: Could not parse existing config ({}), creating backup and starting fresh", e);
                        // Create backup of malformed config
                        let backup_path = config_path.with_extension("json.backup");
                        if let Err(backup_err) = fs::copy(&config_path, &backup_path) {
                            eprintln!("   Failed to create backup: {}", backup_err);
                        } else {
                            println!("   Backup saved to: {:?}", backup_path);
                        }
                        serde_json::json!({})
                    }
                }
            }
            _ => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if !config_value.is_object() {
        config_value = serde_json::json!({});
    }

    let config_obj = config_value.as_object_mut().unwrap();
    if !config_obj.contains_key("mcpServers") {
        config_obj.insert("mcpServers".to_string(), serde_json::json!({}));
    }

    // Get the mcpServers object
    let mcp_servers = config_obj.get_mut("mcpServers").unwrap().as_object_mut();
    if mcp_servers.is_none() {
        eprintln!("⚠️  Warning: mcpServers is not an object, replacing it");
        config_obj.insert("mcpServers".to_string(), serde_json::json!({}));
    }
    let mcp_servers = config_obj
        .get_mut("mcpServers")
        .unwrap()
        .as_object_mut()
        .unwrap();

    // Create the new mcp-muse entry
    let mut muse_entry = serde_json::json!({
        "transport": "stdio",
        "command": get_server_command(),
        "enabled": true
    });

    // Add soundfont path to MCP config if custom path is configured
    if let Ok(config) = SetupConfig::load() {
        if let Some(soundfont_path) = config.soundfont_path {
            muse_entry["soundfont_path"] = serde_json::Value::String(soundfont_path);
        }
    }

    // Check if we need to update
    let mut updated = false;
    match mcp_servers.get(SERVER_NAME) {
        Some(existing) if existing == &muse_entry => {
            println!("✓ mcp-muse entry already up to date in Cursor config.");
        }
        _ => {
            mcp_servers.insert(SERVER_NAME.to_string(), muse_entry);
            updated = true;
            println!("✓ Added or updated mcp-muse entry in Cursor config.");
        }
    }

    // Write back the config if updated
    if updated {
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match fs::File::create(&config_path).and_then(|mut f| {
            let content = serde_json::to_string_pretty(&config_value).unwrap();
            f.write_all(content.as_bytes())
        }) {
            Ok(_) => println!("✓ Saved Cursor MCP config to {:?}", config_path),
            Err(e) => eprintln!("❌ Failed to save Cursor MCP config: {}", e),
        }
    }
}
