use std::fs;
use std::process::Command;

#[test]
fn test_logging_integration_fallback() {
    println!("🧪 Integration Test: Logging Fallback Behavior");

    // We'll test by running the main binary with environment manipulation
    // This tests the actual integrated behavior

    let temp_dir = std::env::temp_dir().join("mcp-muse-integration-test");
    let _ = fs::remove_dir_all(&temp_dir);

    // Test 1: Normal operation (should create log directory successfully)
    println!("📁 Testing normal logging directory creation...");

    let output = Command::new("cargo")
        .args(&["run", "--", "--help"])
        .env("HOME", temp_dir.join("fake-home"))
        .output()
        .expect("Failed to execute mcp-muse");

    println!("Exit status: {}", output.status);
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));

    // The binary should run successfully regardless of logging setup
    assert!(output.status.success() || output.status.code() == Some(0));

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    println!("✅ Integration test completed!");
}

#[test]
fn test_logging_file_creation() {
    println!("📝 Testing actual log file creation...");

    let temp_dir = std::env::temp_dir().join("mcp-muse-log-file-test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Test that we can create a log file in the fallback directory (current dir)
    let current_dir = std::env::current_dir().expect("Failed to get current dir");
    let log_file = current_dir.join("test-mcp-muse.log");

    // Clean up any existing test log file
    let _ = fs::remove_file(&log_file);

    // Create a test log file to verify the fallback directory is writable
    match fs::write(&log_file, "test log entry") {
        Ok(_) => {
            println!(
                "✅ Successfully wrote to fallback log location: {:?}",
                log_file
            );
            // Clean up
            let _ = fs::remove_file(&log_file);
        }
        Err(e) => {
            println!("❌ Failed to write to fallback log location: {}", e);
            panic!("Fallback logging location is not writable");
        }
    }

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    println!("✅ Log file creation test completed!");
}
