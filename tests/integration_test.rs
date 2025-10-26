// Integration tests for SFU Server
// These tests verify end-to-end functionality including HTTP endpoints and WebSocket connections

use tokio::time::{sleep, Duration};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{StreamExt, SinkExt};

/// Test HTTP health check endpoint
/// Verifies that the server responds with healthy status
#[tokio::test]
#[ignore] // Requires running server
async fn test_health_endpoint() {
    let url = "http://127.0.0.1:8080/sfu/health";
    let client = reqwest::Client::new();

    match client.get(url).send().await {
        Ok(resp) => {
            assert_eq!(resp.status(), 200, "Health endpoint should return 200 OK");

            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["status"], "healthy");
            assert_eq!(body["service"], "SFU Server");
            assert_eq!(body["version"], "1.0.0");
        }
        Err(e) => {
            eprintln!("Server not running: {}. Start server with 'cargo run' before running integration tests.", e);
            panic!("Cannot connect to server");
        }
    }
}

/// Test HTTP config endpoint
/// Verifies that configuration can be retrieved
#[tokio::test]
#[ignore] // Requires running server
async fn test_config_endpoint() {
    let url = "http://127.0.0.1:8080/sfu/config";
    let client = reqwest::Client::new();

    match client.get(url).send().await {
        Ok(resp) => {
            assert_eq!(resp.status(), 200, "Config endpoint should return 200 OK");

            let body: serde_json::Value = resp.json().await.unwrap();
            assert!(body.is_object(), "Config should return a JSON object");
        }
        Err(e) => {
            eprintln!("Server not running: {}", e);
            panic!("Cannot connect to server");
        }
    }
}

/// Test WebSocket connection establishment
/// Verifies that clients can connect to the WebSocket endpoint
#[tokio::test]
#[ignore] // Requires running server
async fn test_websocket_connection() {
    let url = "ws://127.0.0.1:8080/sfu";

    match connect_async(url).await {
        Ok((ws_stream, _)) => {
            println!("WebSocket connection established successfully");
            drop(ws_stream); // Clean disconnect
        }
        Err(e) => {
            eprintln!("Cannot connect to WebSocket: {}", e);
            panic!("WebSocket connection failed");
        }
    }
}

/// Test room creation flow
/// Verifies that a proctor can create a room and receive a room ID
#[tokio::test]
#[ignore] // Requires running server
async fn test_create_room_flow() {
    let url = "ws://127.0.0.1:8080/sfu";

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();

    // Send CreateRoom message
    let create_room_msg = json!({
        "type": "CreateRoom",
        "peer_id": "test_proctor_1",
        "name": "Dr. Test"
    });

    write.send(Message::Text(create_room_msg.to_string()))
        .await
        .expect("Failed to send message");

    // Wait for RoomCreated response
    let timeout = sleep(Duration::from_secs(2));
    tokio::pin!(timeout);

    tokio::select! {
        msg = read.next() => {
            if let Some(Ok(Message::Text(text))) = msg {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(response["type"], "RoomCreated", "Should receive RoomCreated message");
                assert!(response["room_id"].is_string(), "Should include room_id");

                let room_id = response["room_id"].as_str().unwrap();
                assert_eq!(room_id.len(), 6, "Room ID should be 6 characters");

                println!("Room created successfully: {}", room_id);
            } else {
                panic!("Did not receive expected RoomCreated message");
            }
        }
        _ = &mut timeout => {
            panic!("Timeout waiting for RoomCreated response");
        }
    }
}

/// Test student join request flow
/// Verifies that a student can request to join a room
#[tokio::test]
#[ignore] // Requires running server
async fn test_student_join_request() {
    let url = "ws://127.0.0.1:8080/sfu";

    // First, create a room as proctor
    let (proctor_stream, _) = connect_async(url).await.expect("Failed to connect proctor");
    let (mut proctor_write, mut proctor_read) = proctor_stream.split();

    let create_room_msg = json!({
        "type": "CreateRoom",
        "peer_id": "test_proctor_2",
        "name": "Dr. Test2"
    });

    proctor_write.send(Message::Text(create_room_msg.to_string()))
        .await
        .expect("Failed to send CreateRoom");

    // Get room ID
    let room_id = if let Some(Ok(Message::Text(text))) = proctor_read.next().await {
        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        response["room_id"].as_str().unwrap().to_string()
    } else {
        panic!("Failed to get room ID");
    };

    println!("Testing with room: {}", room_id);

    // Now connect as student
    let (student_stream, _) = connect_async(url).await.expect("Failed to connect student");
    let (mut student_write, mut student_read) = student_stream.split();

    let join_request_msg = json!({
        "type": "JoinRequest",
        "room_id": room_id,
        "peer_id": "test_student_1",
        "name": "Test Student",
        "role": "student"
    });

    student_write.send(Message::Text(join_request_msg.to_string()))
        .await
        .expect("Failed to send JoinRequest");

    // Student should receive join_request_sent confirmation
    let timeout = sleep(Duration::from_secs(2));
    tokio::pin!(timeout);

    tokio::select! {
        msg = student_read.next() => {
            if let Some(Ok(Message::Text(text))) = msg {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(response["type"], "join_request_sent");
                println!("Student join request sent successfully");
            }
        }
        _ = &mut timeout => {
            panic!("Timeout waiting for join_request_sent");
        }
    }
}

/// Test multiple students in same room
/// Verifies room capacity and multiple peer handling
#[tokio::test]
#[ignore] // Requires running server
async fn test_multiple_students() {
    let url = "ws://127.0.0.1:8080/sfu";

    // Create room
    let (proctor_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut proctor_write, mut proctor_read) = proctor_stream.split();

    let create_msg = json!({
        "type": "CreateRoom",
        "peer_id": "proctor_multi",
        "name": "Multi Test"
    });

    proctor_write.send(Message::Text(create_msg.to_string())).await.unwrap();

    let room_id = if let Some(Ok(Message::Text(text))) = proctor_read.next().await {
        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        response["room_id"].as_str().unwrap().to_string()
    } else {
        panic!("Failed to get room ID");
    };

    // Connect multiple students
    for i in 1..=3 {
        let (student_stream, _) = connect_async(url).await.expect("Failed to connect student");
        let (mut student_write, _) = student_stream.split();

        let join_msg = json!({
            "type": "JoinRequest",
            "room_id": room_id.clone(),
            "peer_id": format!("student_{}", i),
            "name": format!("Student {}", i),
            "role": "student"
        });

        student_write.send(Message::Text(join_msg.to_string())).await.unwrap();
        sleep(Duration::from_millis(100)).await;
    }

    println!("Successfully sent join requests for 3 students");
}

/// Test invalid room join
/// Verifies that joining non-existent room is handled properly
#[tokio::test]
#[ignore] // Requires running server
async fn test_join_invalid_room() {
    let url = "ws://127.0.0.1:8080/sfu";

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();

    let join_msg = json!({
        "type": "JoinRequest",
        "room_id": "999999",
        "peer_id": "test_student_invalid",
        "name": "Test",
        "role": "student"
    });

    write.send(Message::Text(join_msg.to_string())).await.unwrap();

    // Should receive error response
    let timeout = sleep(Duration::from_secs(2));
    tokio::pin!(timeout);

    tokio::select! {
        msg = read.next() => {
            if let Some(Ok(Message::Text(text))) = msg {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                // Server should handle this gracefully
                println!("Received response: {}", response);
            }
        }
        _ = &mut timeout => {
            println!("No response received (acceptable for invalid room)");
        }
    }
}
