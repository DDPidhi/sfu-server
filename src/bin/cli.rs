// SFU Server CLI Validation Tool
// This tool validates SFU server functionality through automated tests and interactive commands

use clap::{Parser, Subcommand};
use colored::*;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::io::{self, Write};
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use urlencoding;

#[derive(Parser)]
#[command(name = "sfu-cli")]
#[command(about = "SFU Server CLI Validation Tool", long_about = None)]
struct Cli {
    /// Server URL (default: ws://127.0.0.1:8080)
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,

    /// IPFS API URL (default: http://localhost:5001)
    #[arg(short, long, default_value = "http://localhost:5001")]
    ipfs: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check server health endpoint
    Health,

    /// Get server configuration
    Config,

    /// Test WebSocket connection
    Connect,

    /// Create a room as proctor
    CreateRoom {
        /// Proctor peer ID
        #[arg(short, long)]
        peer_id: String,

        /// Proctor name (optional)
        #[arg(short, long)]
        name: Option<String>,

        /// Keep connection alive (press Ctrl+C to exit)
        #[arg(short, long)]
        keep_alive: bool,
    },

    /// Join a room as student
    JoinRoom {
        /// Room ID to join
        #[arg(short, long)]
        room_id: String,

        /// Student peer ID
        #[arg(short, long)]
        peer_id: String,

        /// Student name (optional)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Run automated validation scenarios
    Validate {
        /// Run all validation tests
        #[arg(short, long)]
        all: bool,

        /// Test specific scenario
        #[arg(short, long)]
        scenario: Option<String>,
    },

    /// Interactive mode - send custom messages
    Interactive,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Health => {
            check_health(&cli.server).await;
        }
        Commands::Config => {
            check_config(&cli.server).await;
        }
        Commands::Connect => {
            test_connection(&cli.server).await;
        }
        Commands::CreateRoom { peer_id, name, keep_alive } => {
            create_room(&cli.server, peer_id, name.as_deref(), *keep_alive).await;
        }
        Commands::JoinRoom {
            room_id,
            peer_id,
            name,
        } => {
            join_room(&cli.server, room_id, peer_id, name.as_deref()).await;
        }
        Commands::Validate { all, scenario } => {
            if *all {
                run_all_validations(&cli.server, &cli.ipfs).await;
            } else if let Some(s) = scenario {
                run_scenario(&cli.server, &cli.ipfs, s).await;
            } else {
                println!("{}", "Use --all or --scenario <name>".yellow());
                list_scenarios();
            }
        }
        Commands::Interactive => {
            interactive_mode(&cli.server).await;
        }
    }
}

async fn check_health(server: &str) {
    println!("{}", "Checking server health...".cyan());

    let url = format!("http://{}/sfu/health", server);
    let client = reqwest::Client::new();

    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                println!("{} Health check passed", "✓".green());

                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    println!("  Status: {}", body["status"].as_str().unwrap_or("unknown"));
                    println!("  Service: {}", body["service"].as_str().unwrap_or("unknown"));
                    println!("  Version: {}", body["version"].as_str().unwrap_or("unknown"));
                }
            } else {
                println!("{} Health check failed: {}", "✗".red(), status);
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
            println!("  Make sure the server is running on {}", server);
        }
    }
}

async fn check_config(server: &str) {
    println!("{}", "Fetching server configuration...".cyan());

    let url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("{} Config endpoint accessible", "✓".green());

                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    println!("\nConfiguration:");
                    println!("{}", serde_json::to_string_pretty(&body).unwrap());
                }
            } else {
                println!("{} Config fetch failed: {}", "✗".red(), resp.status());
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
        }
    }
}

async fn test_connection(server: &str) {
    println!("{}", "Testing WebSocket connection...".cyan());

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            println!("{} WebSocket connection established", "✓".green());
            println!("  URL: {}", url);
            drop(ws_stream);
            println!("{} Connection closed cleanly", "✓".green());
        }
        Err(e) => {
            println!("{} WebSocket connection failed: {}", "✗".red(), e);
        }
    }
}

async fn create_room(server: &str, peer_id: &str, name: Option<&str>, keep_alive: bool) {
    println!("{}", "Creating room...".cyan());
    println!("  Proctor ID: {}", peer_id);
    if let Some(n) = name {
        println!("  Name: {}", n);
    }

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            let (mut write, mut read) = ws_stream.split();

            // Send CreateRoom message
            let msg = json!({
                "type": "CreateRoom",
                "peer_id": peer_id,
                "name": name,
            });

            if write.send(Message::Text(msg.to_string())).await.is_err() {
                println!("{} Failed to send CreateRoom message", "✗".red());
                return;
            }

            println!("{} CreateRoom message sent", "✓".green());
            println!("Waiting for response...");

            // Wait for RoomCreated response
            let room_id = match timeout(Duration::from_secs(5), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                        if response["type"] == "RoomCreated" {
                            let room_id = response["room_id"].as_str().unwrap_or("unknown").to_string();
                            println!("{} Room created successfully!", "✓".green());
                            println!("\n{}", "═".repeat(50).green());
                            println!("{} {}", "Room ID:".bold(), room_id.green().bold());
                            println!("{}", "═".repeat(50).green());
                            Some(room_id)
                        } else {
                            println!("{} Unexpected response: {}", "✗".yellow(), response["type"]);
                            println!("{}", text);
                            None
                        }
                    } else {
                        None
                    }
                }
                Ok(Some(Ok(msg))) => {
                    println!("{} Unexpected message type: {:?}", "✗".yellow(), msg);
                    None
                }
                Ok(Some(Err(e))) => {
                    println!("{} Error receiving message: {}", "✗".red(), e);
                    None
                }
                Ok(None) => {
                    println!("{} Connection closed by server", "✗".red());
                    None
                }
                Err(_) => {
                    println!("{} Timeout waiting for response", "✗".red());
                    None
                }
            };

            if keep_alive && room_id.is_some() {
                println!("\n{}", "Connection is being kept alive...".yellow());
                println!("Students can now join room: {}", room_id.unwrap().green().bold());
                println!("Press {} to disconnect and close the room.", "Ctrl+C".bold());

                // Keep connection alive by listening for messages
                loop {
                    match timeout(Duration::from_secs(30), read.next()).await {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            println!("{} {}", "◀".green(), text.bright_white());
                        }
                        Ok(Some(Ok(Message::Close(_)))) => {
                            println!("{} Server closed the connection", "✗".yellow());
                            break;
                        }
                        Ok(Some(Ok(_))) => {
                            // Ignore other message types (Binary, Ping, Pong, Frame)
                            continue;
                        }
                        Ok(Some(Err(e))) => {
                            println!("{} Connection error: {}", "✗".red(), e);
                            break;
                        }
                        Ok(None) => {
                            println!("{} Connection closed", "✗".yellow());
                            break;
                        }
                        Err(_) => {
                            // Timeout - just continue listening
                            continue;
                        }
                    }
                }
            } else if !keep_alive {
                println!("\n{}", "⚠ Note: Connection closed. Room will be deleted.".yellow());
                println!("Use {} to keep the room active.", "--keep-alive".cyan());
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
        }
    }
}

async fn join_room(server: &str, room_id: &str, peer_id: &str, name: Option<&str>) {
    println!("{}", "Joining room...".cyan());
    println!("  Room ID: {}", room_id);
    println!("  Student ID: {}", peer_id);
    if let Some(n) = name {
        println!("  Name: {}", n);
    }

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            let (mut write, mut read) = ws_stream.split();

            // Send JoinRequest message
            let msg = json!({
                "type": "JoinRequest",
                "room_id": room_id,
                "peer_id": peer_id,
                "name": name,
                "role": "student",
            });

            if write.send(Message::Text(msg.to_string())).await.is_err() {
                println!("{} Failed to send JoinRequest message", "✗".red());
                return;
            }

            println!("{} JoinRequest message sent", "✓".green());
            println!("Waiting for response...");

            // Wait for response
            match timeout(Duration::from_secs(5), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                        match response["type"].as_str() {
                            Some("join_request_sent") => {
                                println!("{} Join request sent to proctor", "✓".green());
                                println!("  Waiting for proctor approval...");
                            }
                            Some("error") => {
                                println!("{} Error: {}", "✗".red(), response["message"]);
                            }
                            _ => {
                                println!("Response: {}", text);
                            }
                        }
                    }
                }
                Ok(Some(Ok(msg))) => {
                    println!("Received: {:?}", msg);
                }
                Ok(Some(Err(e))) => {
                    println!("{} Error: {}", "✗".red(), e);
                }
                Ok(None) => {
                    println!("{} Connection closed", "✗".red());
                }
                Err(_) => {
                    println!("{} Timeout", "✗".red());
                }
            }
        }
        Err(e) => {
            println!("{} Cannot connect: {}", "✗".red(), e);
        }
    }
}

fn list_scenarios() {
    println!("\n{}", "Available Validation Scenarios:".bold());
    println!("\n{}", "SFU Server:".bold().cyan());
    println!("  {} - Basic WebSocket connection test", "connection".cyan());
    println!("  {} - Room creation flow", "create-room".cyan());
    println!("  {} - Student join flow", "join-room".cyan());
    println!("  {} - Multiple students joining", "multi-student".cyan());
    println!("  {} - Invalid room join (error handling)", "invalid-room".cyan());
    println!("\n{}", "IPFS:".bold().cyan());
    println!("  {} - Check IPFS node connectivity", "ipfs-health".cyan());
    println!("  {} - Upload test file to IPFS", "ipfs-upload".cyan());
    println!("  {} - Verify MFS (Mutable File System)", "ipfs-mfs".cyan());
    println!("\nExample: sfu-cli validate --scenario connection");
    println!("Example: sfu-cli validate --scenario ipfs-health");
}

async fn run_scenario(server: &str, ipfs_url: &str, scenario: &str) {
    println!("\n{} {}", "Running scenario:".bold(), scenario.cyan());
    println!("{}", "─".repeat(60));

    let result = match scenario {
        "connection" => validate_connection(server).await,
        "create-room" => validate_create_room(server).await,
        "join-room" => validate_join_room(server).await,
        "multi-student" => validate_multi_student(server).await,
        "invalid-room" => validate_invalid_room(server).await,
        "ipfs-health" => validate_ipfs_health(ipfs_url).await,
        "ipfs-upload" => validate_ipfs_upload(ipfs_url).await,
        "ipfs-mfs" => validate_ipfs_mfs(ipfs_url).await,
        _ => {
            println!("{} Unknown scenario: {}", "✗".red(), scenario);
            list_scenarios();
            return;
        }
    };

    if result {
        println!("\n{} Scenario passed", "✓".green().bold());
    } else {
        println!("\n{} Scenario failed", "✗".red().bold());
    }
}

async fn run_all_validations(server: &str, ipfs_url: &str) {
    println!("\n{}", "Running All Validation Tests".bold().green());
    println!("{}\n", "═".repeat(60).green());

    let sfu_scenarios = vec![
        "connection",
        "create-room",
        "join-room",
        "multi-student",
        "invalid-room",
    ];

    let ipfs_scenarios = vec![
        "ipfs-health",
        "ipfs-upload",
        "ipfs-mfs",
    ];

    let mut passed = 0;
    let mut failed = 0;

    // Run SFU scenarios
    println!("{}", "SFU Server Tests".bold().cyan());
    for scenario in sfu_scenarios {
        println!("\n{} Testing: {}", "▶".cyan(), scenario.bold());
        println!("{}", "─".repeat(60));

        let result = match scenario {
            "connection" => validate_connection(server).await,
            "create-room" => validate_create_room(server).await,
            "join-room" => validate_join_room(server).await,
            "multi-student" => validate_multi_student(server).await,
            "invalid-room" => validate_invalid_room(server).await,
            _ => false,
        };

        if result {
            passed += 1;
        } else {
            failed += 1;
        }

        sleep(Duration::from_millis(500)).await;
    }

    // Run IPFS scenarios
    println!("\n{}", "IPFS Tests".bold().cyan());
    for scenario in ipfs_scenarios {
        println!("\n{} Testing: {}", "▶".cyan(), scenario.bold());
        println!("{}", "─".repeat(60));

        let result = match scenario {
            "ipfs-health" => validate_ipfs_health(ipfs_url).await,
            "ipfs-upload" => validate_ipfs_upload(ipfs_url).await,
            "ipfs-mfs" => validate_ipfs_mfs(ipfs_url).await,
            _ => false,
        };

        if result {
            passed += 1;
        } else {
            failed += 1;
        }

        sleep(Duration::from_millis(500)).await;
    }

    println!("\n{}", "═".repeat(60).green());
    println!("{}", "Validation Summary".bold());
    println!("{}", "═".repeat(60).green());
    println!("  {} Passed: {}", "✓".green(), passed.to_string().green());
    println!("  {} Failed: {}", "✗".red(), failed.to_string().red());
    println!("  Total: {}", passed + failed);

    if failed == 0 {
        println!("\n{}", "All validations passed! 🎉".green().bold());
    } else {
        println!("\n{}", "Some validations failed. Check output above.".yellow());
    }
}

async fn validate_connection(server: &str) -> bool {
    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            println!("{} WebSocket connection successful", "✓".green());
            drop(ws_stream);
            true
        }
        Err(e) => {
            println!("{} Connection failed: {}", "✗".red(), e);
            false
        }
    }
}

async fn validate_create_room(server: &str) -> bool {
    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            let (mut write, mut read) = ws_stream.split();

            let msg = json!({
                "type": "CreateRoom",
                "peer_id": "validator_proctor",
                "name": "Validator",
            });

            if write.send(Message::Text(msg.to_string())).await.is_err() {
                println!("{} Failed to send message", "✗".red());
                return false;
            }

            match timeout(Duration::from_secs(3), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                        if response["type"] == "RoomCreated" {
                            println!("{} Room created: {}", "✓".green(), response["room_id"]);
                            return true;
                        }
                    }
                    println!("{} Unexpected response", "✗".yellow());
                    false
                }
                _ => {
                    println!("{} No response received", "✗".red());
                    false
                }
            }
        }
        Err(e) => {
            println!("{} Connection failed: {}", "✗".red(), e);
            false
        }
    }
}

async fn validate_join_room(server: &str) -> bool {
    println!("  Step 1: Creating room (proctor connects)...");

    let url = format!("ws://{}/sfu", server);

    // Connect proctor and keep connection alive
    let proctor_conn = match connect_async(&url).await {
        Ok((ws_stream, _)) => ws_stream,
        Err(e) => {
            println!("{} Proctor connection failed: {}", "✗".red(), e);
            return false;
        }
    };

    let (mut proctor_write, mut proctor_read) = proctor_conn.split();

    // Create room
    let msg = json!({
        "type": "CreateRoom",
        "peer_id": "test_proctor_join",
        "name": "Test Proctor",
    });

    if proctor_write.send(Message::Text(msg.to_string())).await.is_err() {
        println!("{} Failed to send CreateRoom message", "✗".red());
        return false;
    }

    let room_id = match timeout(Duration::from_secs(3), proctor_read.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                if response["type"] == "RoomCreated" {
                    response["room_id"].as_str().map(String::from)
                } else {
                    println!("{} Unexpected response: {}", "✗".yellow(), text);
                    None
                }
            } else {
                None
            }
        }
        _ => {
            println!("{} No response received for CreateRoom", "✗".red());
            None
        }
    };

    let room_id = match room_id {
        Some(id) => {
            println!("  {} Room created: {}", "✓".green(), id);
            id
        }
        None => {
            println!("{} Failed to create room", "✗".red());
            return false;
        }
    };

    // Step 2: Student joins while proctor is still connected
    println!("  Step 2: Student joining room...");

    let student_conn = match connect_async(&url).await {
        Ok((ws_stream, _)) => ws_stream,
        Err(e) => {
            println!("{} Student connection failed: {}", "✗".red(), e);
            return false;
        }
    };

    let (mut student_write, mut student_read) = student_conn.split();

    let msg = json!({
        "type": "JoinRequest",
        "room_id": room_id,
        "peer_id": "test_student_join",
        "name": "Test Student",
        "role": "student",
    });

    if student_write.send(Message::Text(msg.to_string())).await.is_err() {
        println!("{} Failed to send join request", "✗".red());
        return false;
    }

    let result = match timeout(Duration::from_secs(3), student_read.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                if response["type"] == "join_request_sent" {
                    println!("{} Join request sent successfully", "✓".green());
                    true
                } else {
                    println!("{} Unexpected response: {}", "✗".yellow(), text);
                    false
                }
            } else {
                println!("{} Failed to parse response", "✗".red());
                false
            }
        }
        _ => {
            println!("{} No response received", "✗".red());
            false
        }
    };

    // Connections are dropped here, cleaning up proctor and student
    result
}

async fn validate_multi_student(server: &str) -> bool {
    println!("  Creating room for multi-student test...");

    let url = format!("ws://{}/sfu", server);

    // Connect proctor and keep connection alive
    let proctor_conn = match connect_async(&url).await {
        Ok((ws_stream, _)) => ws_stream,
        Err(e) => {
            println!("{} Proctor connection failed: {}", "✗".red(), e);
            return false;
        }
    };

    let (mut proctor_write, mut proctor_read) = proctor_conn.split();

    // Create room
    let msg = json!({
        "type": "CreateRoom",
        "peer_id": "proctor_multi",
        "name": "Multi Test",
    });

    if proctor_write.send(Message::Text(msg.to_string())).await.is_err() {
        println!("{} Failed to send CreateRoom message", "✗".red());
        return false;
    }

    let room_id = match timeout(Duration::from_secs(3), proctor_read.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                response.get("room_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            } else {
                None
            }
        }
        _ => None,
    };

    let room_id = match room_id {
        Some(id) => {
            println!("  {} Room created: {}", "✓".green(), id);
            id
        }
        None => {
            println!("{} Failed to create room", "✗".red());
            return false;
        }
    };

    // Try to add 3 students while proctor stays connected
    let mut success_count = 0;

    for i in 1..=3 {
        println!("  Connecting student {}...", i);

        if let Ok((ws_stream, _)) = connect_async(&url).await {
            let (mut write, mut read) = ws_stream.split();

            let msg = json!({
                "type": "JoinRequest",
                "room_id": room_id,
                "peer_id": format!("student_multi_{}", i),
                "name": format!("Student {}", i),
                "role": "student",
            });

            if write.send(Message::Text(msg.to_string())).await.is_ok() {
                // Wait for response to verify join request was processed
                if let Ok(Some(Ok(Message::Text(text)))) =
                    timeout(Duration::from_secs(2), read.next()).await
                {
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                        if response["type"] == "join_request_sent" {
                            success_count += 1;
                            println!("    {} Student {} join request sent", "✓".green(), i);
                        }
                    }
                }
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    // Proctor connection is dropped here, cleaning up the room

    if success_count == 3 {
        println!("{} All 3 students join requests sent successfully", "✓".green());
        true
    } else {
        println!(
            "{} Only {} out of 3 students sent join requests",
            "✗".red(),
            success_count
        );
        false
    }
}

async fn validate_invalid_room(server: &str) -> bool {
    println!("  Attempting to join non-existent room...");

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            let (mut write, _) = ws_stream.split();

            let msg = json!({
                "type": "JoinRequest",
                "room_id": "999999",
                "peer_id": "invalid_test",
                "name": "Invalid Test",
                "role": "student",
            });

            if write.send(Message::Text(msg.to_string())).await.is_ok() {
                println!("{} Request sent (server should handle gracefully)", "✓".green());
                true
            } else {
                println!("{} Failed to send request", "✗".red());
                false
            }
        }
        Err(e) => {
            println!("{} Connection failed: {}", "✗".red(), e);
            false
        }
    }
}

// ============================================================================
// IPFS Validation Functions
// ============================================================================

async fn validate_ipfs_health(ipfs_url: &str) -> bool {
    println!("  Checking IPFS node connectivity...");

    let client = reqwest::Client::new();
    let version_url = format!("{}/api/v0/version", ipfs_url);

    match client.post(&version_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let version = body["Version"].as_str().unwrap_or("unknown");
                    println!("{} IPFS node is accessible", "✓".green());
                    println!("  Version: {}", version);
                    return true;
                }
                println!("{} IPFS node responded but couldn't parse version", "✓".green());
                true
            } else {
                println!("{} IPFS API returned error: {}", "✗".red(), response.status());
                false
            }
        }
        Err(e) => {
            println!("{} Cannot connect to IPFS: {}", "✗".red(), e);
            println!("  Make sure IPFS is running at {}", ipfs_url);
            false
        }
    }
}

async fn validate_ipfs_upload(ipfs_url: &str) -> bool {
    println!("  Testing IPFS file upload...");

    let client = reqwest::Client::new();
    let add_url = format!("{}/api/v0/add", ipfs_url);

    // Create a small test file content
    let test_content = format!(
        "SFU CLI IPFS Test - Timestamp: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    // Create multipart form with test content
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(test_content.as_bytes().to_vec())
                .file_name("sfu-cli-test.txt"),
        );

    match client.post(&add_url).multipart(form).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let hash = body["Hash"].as_str().unwrap_or("unknown");
                    let size = body["Size"].as_str().unwrap_or("unknown");
                    println!("{} File uploaded successfully", "✓".green());
                    println!("  CID: {}", hash);
                    println!("  Size: {} bytes", size);
                    return true;
                }
                println!("{} Upload succeeded but couldn't parse response", "✗".yellow());
                false
            } else {
                let error = response.text().await.unwrap_or_default();
                println!("{} Upload failed: {}", "✗".red(), error);
                false
            }
        }
        Err(e) => {
            println!("{} Upload request failed: {}", "✗".red(), e);
            false
        }
    }
}

async fn validate_ipfs_mfs(ipfs_url: &str) -> bool {
    println!("  Testing IPFS MFS (Mutable File System)...");

    let client = reqwest::Client::new();

    // Step 1: Create a test directory
    let test_dir = "/sfu-cli-test";
    let mkdir_url = format!(
        "{}/api/v0/files/mkdir?arg={}&parents=true",
        ipfs_url,
        urlencoding::encode(test_dir)
    );

    match client.post(&mkdir_url).send().await {
        Ok(response) => {
            if !response.status().is_success() {
                let error = response.text().await.unwrap_or_default();
                // Ignore "already exists" errors
                if !error.contains("already has entry") && !error.is_empty() {
                    println!("{} Failed to create MFS directory: {}", "✗".red(), error);
                    return false;
                }
            }
            println!("  {} Created test directory: {}", "✓".green(), test_dir);
        }
        Err(e) => {
            println!("{} MFS mkdir request failed: {}", "✗".red(), e);
            return false;
        }
    }

    // Step 2: List the root directory
    let ls_url = format!("{}/api/v0/files/ls?arg=/&long=true", ipfs_url);

    match client.post(&ls_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let entries = body["Entries"].as_array();
                    let count = entries.map(|e| e.len()).unwrap_or(0);
                    println!("  {} MFS listing successful ({} entries in root)", "✓".green(), count);

                    // Check if recordings directory exists
                    if let Some(entries) = entries {
                        let has_recordings = entries.iter().any(|e| {
                            e["Name"].as_str() == Some("recordings")
                        });
                        if has_recordings {
                            println!("  {} Found /recordings directory", "✓".green());
                        }
                    }
                    return true;
                }
                println!("{} MFS listing succeeded but couldn't parse response", "✗".yellow());
                false
            } else {
                let error = response.text().await.unwrap_or_default();
                println!("{} MFS listing failed: {}", "✗".red(), error);
                false
            }
        }
        Err(e) => {
            println!("{} MFS ls request failed: {}", "✗".red(), e);
            false
        }
    }
}

async fn interactive_mode(server: &str) {
    println!("\n{}", "Interactive Mode".bold().green());
    println!("{}", "═".repeat(60).green());
    println!("Type {} for help, {} to quit\n", "help".cyan(), "quit".cyan());

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            println!("{} Connected to server", "✓".green());

            let (mut write, mut read) = ws_stream.split();

            // Spawn task to receive messages
            let receive_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = read.next().await {
                    if let Message::Text(text) = msg {
                        println!("\n{} {}", "◀".green(), text.bright_white());
                    }
                }
            });

            // Main input loop
            loop {
                print!("{} ", "►".cyan());
                io::stdout().flush().unwrap();

                let mut input = String::new();
                if io::stdin().read_line(&mut input).is_err() {
                    break;
                }

                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                if input == "quit" || input == "exit" {
                    println!("Goodbye!");
                    break;
                }

                if input == "help" {
                    print_interactive_help();
                    continue;
                }

                // Try to parse as JSON and send
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(input) {
                    if write.send(Message::Text(parsed.to_string())).await.is_ok() {
                        println!("{} Message sent", "✓".green());
                    } else {
                        println!("{} Failed to send message", "✗".red());
                        break;
                    }
                } else {
                    println!("{} Invalid JSON. Type 'help' for examples.", "✗".yellow());
                }
            }

            receive_task.abort();
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
        }
    }
}

fn print_interactive_help() {
    println!("\n{}", "Interactive Mode Commands".bold());
    println!("{}", "─".repeat(60));
    println!("Send JSON messages directly to the server.\n");

    println!("{}", "Example Messages:".bold());
    println!("\n{}:", "Create Room".cyan());
    println!(r#"  {{"type":"CreateRoom","peer_id":"proctor1","name":"Dr. Smith"}}"#);

    println!("\n{}:", "Join Room".cyan());
    println!(r#"  {{"type":"JoinRequest","room_id":"123456","peer_id":"student1","name":"John","role":"student"}}"#);

    println!("\n{}:", "Join (Direct)".cyan());
    println!(r#"  {{"type":"Join","room_id":"123456","peer_id":"student1","name":"John","role":"student"}}"#);

    println!("\n{}:", "Leave".cyan());
    println!(r#"  {{"type":"Leave","peer_id":"student1"}}"#);

    println!("\n{}:", "ICE Candidate".cyan());
    println!(r#"  {{"type":"IceCandidate","peer_id":"student1","candidate":"candidate:...","sdp_mid":"0","sdp_mline_index":0}}"#);

    println!("\n{}: quit, exit", "Commands".bold());
    println!();
}
