# SFU Server

A Selective Forwarding Unit (SFU) server built with Rust for real-time WebRTC video conferencing and proctoring applications.

## Features

- WebRTC-based media routing using webrtc-rs
- Room-based peer management
- Track forwarding and management
- WebSocket signaling server
- Ice candidate handling
- Peer connection management
- Health check and configuration endpoints

## Prerequisites

- Rust 1.70 or higher
- Cargo (comes with Rust)

## Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd sfu-server
```

2. Install dependencies:
```bash
cargo build
```

## Configuration

Create a `.env` file in the project root (or use the provided `.env.example`):

```env
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

### Configuration Options

- `SERVER_HOST`: The host address to bind the server (default: `0.0.0.0`)
- `SERVER_PORT`: The port number for the server (default: `8080`)

## Running the Server

### Development Mode

```bash
cargo run
```

### Production Mode

```bash
cargo build --release
./target/release/sfu-server
```

### With Logging

Set the `RUST_LOG` environment variable to control log levels:

```bash
# Info level for all modules
RUST_LOG=info cargo run

# Debug level for sfu-server, info for others
RUST_LOG=info,sfu_server=debug cargo run

# Trace level for detailed debugging
RUST_LOG=trace cargo run
```

## API Endpoints

### WebSocket Signaling
```
ws://localhost:8080/sfu
```

### Health Check
```
GET http://localhost:8080/sfu/health
```

### Configuration
```
GET http://localhost:8080/sfu/config
```

## Project Structure

```
sfu-server/
├── src/
│   ├── main.rs              # Application entry point
│   ├── config/              # Configuration management
│   │   └── mod.rs
│   ├── error.rs             # Error types and handling
│   ├── api/                 # API routes and WebSocket handling
│   │   ├── mod.rs
│   │   ├── sfu_routes.rs
│   │   └── sfu_websocket.rs
│   └── sfu/                 # Core SFU logic
│       ├── mod.rs
│       ├── connection.rs    # Peer connection management
│       ├── room.rs          # Room and peer management
│       ├── server.rs        # SFU server implementation
│       ├── signaling.rs     # Signaling message handling
│       ├── track_manager.rs # Media track management
│       └── webrtc_utils.rs  # WebRTC utilities
├── Cargo.toml               # Project dependencies
├── .env.example             # Example environment configuration
└── README.md                # This file
```

## WebSocket Signaling Protocol

The server accepts JSON messages over WebSocket for signaling:

### Join Room
```json
{
  "type": "join",
  "room_id": "room123",
  "peer_id": "peer456"
}
```

### Offer
```json
{
  "type": "offer",
  "sdp": "<SDP offer string>"
}
```

### Answer
```json
{
  "type": "answer",
  "sdp": "<SDP answer string>"
}
```

### ICE Candidate
```json
{
  "type": "ice_candidate",
  "candidate": "<ICE candidate string>",
  "sdp_mid": "0",
  "sdp_mline_index": 0
}
```

## Development

### CLI Validation Tool

The project includes a CLI tool for testing and validating server functionality. See [CLI_GUIDE.md](CLI_GUIDE.md) for detailed usage.

```bash
# Build the CLI tool
cargo build --release --bin sfu-cli

# Check server health
./target/release/sfu-cli health

# Run automated validations
./target/release/sfu-cli validate --all

# Interactive mode
./target/release/sfu-cli interactive
```

### Run Tests

The project includes comprehensive unit and integration tests. See [TESTING.md](TESTING.md) for detailed testing guide.

```bash
# Run unit tests
cargo test

# Run integration tests (requires running server)
cargo test --test integration_test -- --ignored --test-threads=1
```

**Test Coverage:**
- 29 unit tests (configuration, room management, signaling)
- 7 integration tests (HTTP endpoints, WebSocket flows)
- CLI validation tool with 5 automated scenarios

### Check Code
```bash
cargo check
```

### Format Code
```bash
cargo fmt
```

### Lint Code
```bash
cargo clippy
```

## Dependencies

- `webrtc` (0.8) - WebRTC implementation
- `tokio` (1.x) - Async runtime
- `warp` (0.3) - Web framework
- `serde` & `serde_json` - Serialization
- `futures` (0.3) - Async utilities
- `dotenv` (0.15) - Environment configuration
- `tracing` & `tracing-subscriber` - Logging

## Troubleshooting

### Port Already in Use
If you see an error about the port being in use, either:
- Change the `SERVER_PORT` in your `.env` file
- Stop the process using the port

### Connection Issues
- Ensure firewall rules allow traffic on the configured port
- Check that `SERVER_HOST` is set correctly for your network setup
- For local testing, use `127.0.0.1` or `localhost`
- For network access, use `0.0.0.0`

## License

[Add your license information here]

## Contributing

[Add contribution guidelines here]
