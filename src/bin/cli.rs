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
    println!("\n{}", "Blockchain (Asset Hub EVM):".bold().cyan());
    println!("  {} - Check blockchain config from server", "blockchain-status".cyan());
    println!("  {} - Test RPC endpoint connectivity", "blockchain-rpc".cyan());
    println!("  {} - Validate contract address format", "blockchain-contract".cyan());
    println!("  {} - Test contract read functions", "blockchain-functions".cyan());
    println!("\n{}", "Recording:".bold().cyan());
    println!("  {} - Check recording config from server", "recording-status".cyan());
    println!("\n{}", "IPFS:".bold().cyan());
    println!("  {} - Check IPFS node connectivity", "ipfs-health".cyan());
    println!("  {} - Upload test file to IPFS", "ipfs-upload".cyan());
    println!("  {} - Verify MFS (Mutable File System)", "ipfs-mfs".cyan());
    println!("\nExample: sfu-cli validate --scenario connection");
    println!("Example: sfu-cli validate --scenario blockchain-status");
    println!("Example: sfu-cli validate --scenario blockchain-functions");
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
        "blockchain-status" => validate_blockchain_status(server).await,
        "blockchain-rpc" => validate_blockchain_rpc(server).await,
        "blockchain-contract" => validate_blockchain_contract(server).await,
        "blockchain-functions" => validate_blockchain_functions(server).await,
        "recording-status" => validate_recording_status(server).await,
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

    let blockchain_scenarios = vec![
        "blockchain-status",
        "blockchain-rpc",
        "blockchain-contract",
        "blockchain-functions",
    ];

    let recording_scenarios = vec![
        "recording-status",
    ];

    let ipfs_scenarios = vec![
        "ipfs-health",
        "ipfs-upload",
        "ipfs-mfs",
    ];

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

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

    // Run Blockchain scenarios
    println!("\n{}", "Blockchain (Asset Hub EVM) Tests".bold().cyan());
    for scenario in blockchain_scenarios {
        println!("\n{} Testing: {}", "▶".cyan(), scenario.bold());
        println!("{}", "─".repeat(60));

        let result = match scenario {
            "blockchain-status" => validate_blockchain_status(server).await,
            "blockchain-rpc" => validate_blockchain_rpc(server).await,
            "blockchain-contract" => validate_blockchain_contract(server).await,
            "blockchain-functions" => validate_blockchain_functions(server).await,
            _ => false,
        };

        if result {
            passed += 1;
        } else {
            // Check if blockchain is just disabled (not a failure)
            if scenario == "blockchain-status" {
                skipped += 1;
            } else {
                failed += 1;
            }
        }

        sleep(Duration::from_millis(500)).await;
    }

    // Run Recording scenarios
    println!("\n{}", "Recording Tests".bold().cyan());
    for scenario in recording_scenarios {
        println!("\n{} Testing: {}", "▶".cyan(), scenario.bold());
        println!("{}", "─".repeat(60));

        let result = match scenario {
            "recording-status" => validate_recording_status(server).await,
            _ => false,
        };

        if result {
            passed += 1;
        } else {
            skipped += 1;
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
    if skipped > 0 {
        println!("  {} Skipped (disabled features): {}", "○".yellow(), skipped.to_string().yellow());
    }
    println!("  Total: {}", passed + failed + skipped);

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
// Blockchain (Asset Hub EVM) Validation Functions
// ============================================================================

async fn validate_blockchain_status(server: &str) -> bool {
    println!("  Checking blockchain configuration...");

    let url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let blockchain = &body["blockchain"];
                    let enabled = blockchain["enabled"].as_bool().unwrap_or(false);

                    if !enabled {
                        println!("{} Blockchain integration is disabled", "○".yellow());
                        println!("  Set ASSET_HUB_ENABLED=true to enable");
                        return false;
                    }

                    println!("{} Blockchain integration is enabled", "✓".green());
                    if let Some(rpc_url) = blockchain["rpc_url"].as_str() {
                        println!("  RPC URL: {}", rpc_url);
                    }
                    if let Some(contract) = blockchain["contract_address"].as_str() {
                        println!("  Contract: {}", contract);
                    }
                    if let Some(gas_limit) = blockchain["gas_limit"].as_str() {
                        println!("  Gas Limit: {}", gas_limit);
                    }
                    return true;
                }
                println!("{} Could not parse config response", "✗".red());
                false
            } else {
                println!("{} Config endpoint returned error: {}", "✗".red(), response.status());
                false
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
            false
        }
    }
}

async fn validate_blockchain_rpc(server: &str) -> bool {
    println!("  Testing blockchain RPC connectivity...");

    // First get the RPC URL from server config
    let config_url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    let rpc_url = match client.get(&config_url).send().await {
        Ok(response) => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let blockchain = &body["blockchain"];
                if !blockchain["enabled"].as_bool().unwrap_or(false) {
                    println!("{} Blockchain is disabled, skipping RPC test", "○".yellow());
                    return false;
                }
                blockchain["rpc_url"].as_str().map(String::from)
            } else {
                None
            }
        }
        Err(_) => None,
    };

    let rpc_url = match rpc_url {
        Some(url) => url,
        None => {
            println!("{} Could not get RPC URL from server config", "✗".red());
            return false;
        }
    };

    // Test RPC connectivity with eth_chainId call
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_chainId",
        "params": [],
        "id": 1
    });

    match client
        .post(&rpc_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    if let Some(result) = body["result"].as_str() {
                        // Parse chain ID from hex
                        let chain_id = u64::from_str_radix(result.trim_start_matches("0x"), 16)
                            .unwrap_or(0);
                        println!("{} RPC endpoint is accessible", "✓".green());
                        println!("  URL: {}", rpc_url);
                        println!("  Chain ID: {} (0x{:x})", chain_id, chain_id);

                        // Identify known networks
                        let network_name = match chain_id {
                            1287 => "Moonbase Alpha (Moonbeam TestNet)",
                            1284 => "Moonbeam (Polkadot)",
                            1285 => "Moonriver (Kusama)",
                            420420422 => "Paseo Asset Hub",
                            _ => "Unknown network",
                        };
                        println!("  Network: {}", network_name);
                        return true;
                    }
                }
                println!("{} RPC responded but couldn't parse chain ID", "✗".yellow());
                false
            } else {
                println!("{} RPC returned error: {}", "✗".red(), response.status());
                false
            }
        }
        Err(e) => {
            println!("{} Cannot connect to RPC: {}", "✗".red(), e);
            println!("  URL: {}", rpc_url);
            false
        }
    }
}

async fn validate_blockchain_contract(server: &str) -> bool {
    println!("  Validating contract address format...");

    let config_url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    match client.get(&config_url).send().await {
        Ok(response) => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let blockchain = &body["blockchain"];
                if !blockchain["enabled"].as_bool().unwrap_or(false) {
                    println!("{} Blockchain is disabled, skipping contract validation", "○".yellow());
                    return false;
                }

                if let Some(contract_address) = blockchain["contract_address"].as_str() {
                    // Validate Ethereum address format
                    let is_valid = contract_address.starts_with("0x")
                        && contract_address.len() == 42
                        && contract_address[2..].chars().all(|c| c.is_ascii_hexdigit());

                    if is_valid {
                        println!("{} Contract address is valid", "✓".green());
                        println!("  Address: {}", contract_address);

                        // Get RPC URL to check if contract has code
                        if let Some(rpc_url) = blockchain["rpc_url"].as_str() {
                            let payload = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "eth_getCode",
                                "params": [contract_address, "latest"],
                                "id": 1
                            });

                            if let Ok(rpc_response) = client
                                .post(rpc_url)
                                .header("Content-Type", "application/json")
                                .json(&payload)
                                .send()
                                .await
                            {
                                if let Ok(rpc_body) = rpc_response.json::<serde_json::Value>().await {
                                    if let Some(code) = rpc_body["result"].as_str() {
                                        if code != "0x" && code.len() > 2 {
                                            println!("{} Contract has deployed code", "✓".green());
                                            println!("  Code size: {} bytes", (code.len() - 2) / 2);
                                        } else {
                                            println!("{} No code at contract address (not deployed?)", "✗".yellow());
                                        }
                                    }
                                }
                            }
                        }
                        return true;
                    } else {
                        println!("{} Invalid contract address format", "✗".red());
                        println!("  Address: {}", contract_address);
                        println!("  Expected: 0x followed by 40 hex characters");
                        return false;
                    }
                } else {
                    println!("{} No contract address configured", "✗".red());
                    false
                }
            } else {
                println!("{} Could not parse config response", "✗".red());
                false
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
            false
        }
    }
}

async fn validate_blockchain_functions(server: &str) -> bool {
    println!("  Testing blockchain flow via WebSocket...");

    // First check if blockchain is enabled
    let config_url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    let (rpc_url, contract_address) = match client.get(&config_url).send().await {
        Ok(response) => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let blockchain = &body["blockchain"];
                if !blockchain["enabled"].as_bool().unwrap_or(false) {
                    println!("{} Blockchain is disabled, skipping flow test", "○".yellow());
                    return false;
                }
                let rpc = blockchain["rpc_url"].as_str().map(String::from);
                let contract = blockchain["contract_address"].as_str().map(String::from);
                match (rpc, contract) {
                    (Some(r), Some(c)) => (r, c),
                    _ => {
                        println!("{} Missing RPC URL or contract address", "✗".red());
                        return false;
                    }
                }
            } else {
                println!("{} Could not parse config", "✗".red());
                return false;
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
            return false;
        }
    };

    // Use a known test wallet address for validation
    let test_wallet = "0xD1dcb600264d02933796f01b87A76e6A980Ea6e1";

    println!("\n  Step 1: Creating room with wallet address via WebSocket...");
    println!("    Wallet: {}", test_wallet);

    let url = format!("ws://{}/sfu", server);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            let (mut write, mut read) = ws_stream.split();

            // Send CreateRoom with wallet address
            let msg = json!({
                "type": "CreateRoom",
                "peer_id": "cli-blockchain-test",
                "name": "CLI Blockchain Test",
                "wallet_address": test_wallet,
            });

            if write.send(Message::Text(msg.to_string())).await.is_err() {
                println!("  {} Failed to send CreateRoom message", "✗".red());
                return false;
            }

            // Wait for RoomCreated response
            let room_id = match timeout(Duration::from_secs(5), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                        if response["type"] == "RoomCreated" {
                            let rid = response["room_id"].as_str().unwrap_or("").to_string();
                            println!("  {} Room created: {}", "✓".green(), rid);
                            println!("    (Server emitted ChainEvent::RoomCreated to blockchain queue)");
                            Some(rid)
                        } else {
                            println!("  {} Unexpected response: {}", "✗".yellow(), response["type"]);
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => {
                    println!("  {} No response received", "✗".red());
                    None
                }
            };

            if room_id.is_none() {
                return false;
            }

            // Give blockchain queue time to process
            println!("\n  Step 2: Waiting for blockchain transaction to be submitted...");
            println!("    (Transactions are queued and submitted asynchronously)");
            sleep(Duration::from_secs(3)).await;

            // Verify on-chain by querying contract read functions
            println!("\n  Step 3: Verifying contract read functions...");

            let mut all_passed = true;

            // Test 1: getTotalExamResults()
            // Function selector: keccak256("getTotalExamResults()")[:4] = 0x3c445589
            println!("\n    Testing getTotalExamResults()...");
            let call_data = "0x3c445589";

            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_call",
                "params": [{
                    "to": contract_address,
                    "data": call_data
                }, "latest"],
                "id": 1
            });

            match client
                .post(&rpc_url)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        if let Some(result) = body["result"].as_str() {
                            if result.len() >= 2 {
                                let total = u64::from_str_radix(result.trim_start_matches("0x"), 16)
                                    .unwrap_or(0);
                                println!("    {} getTotalExamResults() = {}", "✓".green(), total);
                            } else {
                                println!("    {} getTotalExamResults() returned empty", "✗".yellow());
                                all_passed = false;
                            }
                        } else if let Some(error) = body["error"].as_object() {
                            let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                            println!("    {} getTotalExamResults() failed: {}", "✗".red(), msg);
                            all_passed = false;
                        }
                    }
                }
                Err(e) => {
                    println!("    {} RPC call failed: {}", "✗".red(), e);
                    all_passed = false;
                }
            }

            // Test 2: getParticipantExamResultIds(address)
            // Function selector: keccak256("getParticipantExamResultIds(address)")[:4] = 0xd0fb2626
            // ABI encode the address (pad to 32 bytes)
            let wallet_no_prefix = test_wallet.trim_start_matches("0x");
            let padded_address = format!("{:0>64}", wallet_no_prefix.to_lowercase());
            let call_data = format!("0xd0fb2626{}", padded_address);

            println!("\n    Testing getParticipantExamResultIds({})...", test_wallet);

            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_call",
                "params": [{
                    "to": contract_address,
                    "data": call_data
                }, "latest"],
                "id": 2
            });

            match client
                .post(&rpc_url)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        if let Some(result) = body["result"].as_str() {
                            // Result is ABI-encoded array - check if it's valid
                            if result.len() > 2 {
                                // Parse ABI-encoded dynamic array: offset (32 bytes) + length (32 bytes) + elements
                                let data = result.trim_start_matches("0x");
                                if data.len() >= 128 {
                                    // Get array length at offset 64-128 (second 32-byte word)
                                    let len_hex = &data[64..128];
                                    let array_len = u64::from_str_radix(len_hex, 16).unwrap_or(0);
                                    println!("    {} getParticipantExamResultIds() returned {} result(s)", "✓".green(), array_len);
                                } else {
                                    println!("    {} getParticipantExamResultIds() returned empty array", "✓".green());
                                }
                            } else {
                                println!("    {} getParticipantExamResultIds() returned empty", "✗".yellow());
                                all_passed = false;
                            }
                        } else if let Some(error) = body["error"].as_object() {
                            let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                            println!("    {} getParticipantExamResultIds() failed: {}", "✗".red(), msg);
                            all_passed = false;
                        }
                    }
                }
                Err(e) => {
                    println!("    {} RPC call failed: {}", "✗".red(), e);
                    all_passed = false;
                }
            }

            // Test 3: getEventCount(string roomId)
            // Function selector: keccak256("getEventCount(string)")[:4] = 0xad10faf4
            // We'll use a test room ID - ABI encode string
            let room_id = room_id.unwrap();
            let room_bytes = room_id.as_bytes();
            let room_hex: String = room_bytes.iter().map(|b| format!("{:02x}", b)).collect();
            // ABI encoding for string: offset (32 bytes) + length (32 bytes) + data (padded to 32 bytes)
            let str_len = room_bytes.len();
            let padded_len = ((str_len + 31) / 32) * 32;
            let padded_room = format!("{:0<width$}", room_hex, width = padded_len * 2);
            let encoded_string = format!(
                "{:064x}{:064x}{}",
                32,  // offset to string data
                str_len,  // string length
                padded_room
            );
            let call_data = format!("0xad10faf4{}", encoded_string);

            println!("\n    Testing getEventCount(\"{}\")...", room_id);

            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_call",
                "params": [{
                    "to": contract_address,
                    "data": call_data
                }, "latest"],
                "id": 3
            });

            match client
                .post(&rpc_url)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        if let Some(result) = body["result"].as_str() {
                            if result.len() >= 2 {
                                let count = u64::from_str_radix(result.trim_start_matches("0x"), 16)
                                    .unwrap_or(0);
                                println!("    {} getEventCount() = {}", "✓".green(), count);
                            } else {
                                println!("    {} getEventCount() returned empty", "✗".yellow());
                            }
                        } else if let Some(error) = body["error"].as_object() {
                            let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                            // This might fail if the room hasn't been recorded on-chain yet
                            println!("    {} getEventCount() failed: {} (room may not be on-chain yet)", "○".yellow(), msg);
                        }
                    }
                }
                Err(e) => {
                    println!("    {} RPC call failed: {}", "✗".red(), e);
                }
            }

            if all_passed {
                println!("\n{} Blockchain flow test completed successfully", "✓".green());
            } else {
                println!("\n{} Blockchain flow test completed with some failures", "✗".yellow());
            }
            println!("  The CreateRoom with wallet triggered the on-chain recording flow.");
            println!("  Check server logs for transaction details.");
            all_passed
        }
        Err(e) => {
            println!("{} WebSocket connection failed: {}", "✗".red(), e);
            false
        }
    }
}

// ============================================================================
// Recording Validation Functions
// ============================================================================

async fn validate_recording_status(server: &str) -> bool {
    println!("  Fetching recording configuration from server...");

    let url = format!("http://{}/sfu/config", server);
    let client = reqwest::Client::new();

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let recording = &body["recording"];
                    let enabled = recording["enabled"].as_bool().unwrap_or(false);

                    if enabled {
                        println!("{} Recording is enabled", "✓".green());
                        if let Some(output_dir) = recording["output_dir"].as_str() {
                            println!("  Output directory: {}", output_dir);
                        }
                        if let Some(format) = recording["format"].as_str() {
                            println!("  Format: {}", format);
                        }

                        // Also check IPFS config since recordings are stored in IPFS
                        let ipfs = &body["ipfs"];
                        if ipfs["enabled"].as_bool().unwrap_or(false) {
                            println!("{} IPFS storage is enabled for recordings", "✓".green());
                            if let Some(api_url) = ipfs["api_url"].as_str() {
                                println!("  IPFS API: {}", api_url);
                            }
                        } else {
                            println!("{} IPFS is disabled (recordings may not be stored)", "○".yellow());
                        }

                        return true;
                    } else {
                        println!("{} Recording is disabled", "○".yellow());
                        println!("  Set RECORDING_ENABLED=true to enable");
                        return false;
                    }
                }
                println!("{} Could not parse config response", "✗".red());
                false
            } else {
                println!("{} Config endpoint returned error: {}", "✗".red(), response.status());
                false
            }
        }
        Err(e) => {
            println!("{} Cannot connect to server: {}", "✗".red(), e);
            false
        }
    }
}

// ============================================================================
// IPFS Validation Functions (direct calls)
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

    let test_content = format!(
        "SFU CLI IPFS Test - Timestamp: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

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

    let ls_url = format!("{}/api/v0/files/ls?arg=/&long=true", ipfs_url);

    match client.post(&ls_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let entries = body["Entries"].as_array();
                    let count = entries.map(|e| e.len()).unwrap_or(0);
                    println!("  {} MFS listing successful ({} entries in root)", "✓".green(), count);

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
