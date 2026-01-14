# SFU Server

A Selective Forwarding Unit (SFU) server built with Rust for real-time WebRTC video conferencing and proctoring applications.

## Features

- WebRTC-based media routing using webrtc-rs
- Room-based peer management with proctor/student roles
- Track forwarding and management
- WebSocket signaling server
- ICE candidate handling
- Peer connection management
- Health check and configuration endpoints
- **Video Recording** - Automatic recording of peer streams using GStreamer
- **IPFS Integration** - Upload recordings to IPFS for distributed storage
- **Docker Support** - Easy deployment with Docker Compose

## Quick Start (Docker - Recommended)

The easiest way to run the SFU server is using Docker. This includes all dependencies (GStreamer, IPFS) pre-configured.

### Prerequisites

- Docker and Docker Compose
- Make (optional, but recommended)

### Running with Docker

```bash
# Clone the repository
git clone <repository-url>
cd sfu-server

# Run with logs in foreground (auto-creates .env from .env.example)
make up

# Or run in background
make upbg
```

> **Note:** On first run, `.env` is automatically created from `.env.example`. Edit `.env` to customize settings.

### Available Make Commands

| Command | Description |
|---------|-------------|
| `make up` | Build and run with logs (foreground) |
| `make upbg` | Build and run in background |
| `make down` | Stop all containers |
| `make logs` | Follow logs for all services |
| `make logs-sfu` | Follow SFU server logs only |
| `make logs-ipfs` | Follow IPFS logs only |
| `make rebuild` | Clean rebuild from scratch |
| `make shell` | Open shell in SFU container |
| `make ipfs-shell` | Open shell in IPFS container |
| `make ps` | Show running containers |
| `make clean` | Stop and remove containers, volumes, images |
| `make init` | Create .env from .env.example (auto-runs with up/build) |
| `make cli-health` | Check server health via CLI |
| `make cli-validate` | Run all CLI validation tests |
| `make cli CMD="..."` | Run any CLI command |
| `make help` | Show all available commands |

### After Code Changes

When you modify the Rust code, rebuild and restart:

```bash
# Quick rebuild and restart
make down && make up

# Or force a clean rebuild (no cache)
make rebuild
```

### Accessing Services

- **SFU WebSocket**: `ws://localhost:8080/sfu`
- **SFU Health Check**: `http://localhost:8080/sfu/health`
- **IPFS Web UI**: `http://localhost:5001/webui`
- **IPFS Gateway**: `http://localhost:8081/ipfs/{CID}`

### Data Storage

All data is stored in the `./data/` directory (gitignored):
- `./data/recordings/` - Video recordings
- `./data/ipfs/` - IPFS data
- `./data/ipfs-staging/` - IPFS staging area

---

## Manual Installation (Without Docker)

If you prefer to run without Docker, you'll need to install dependencies manually.

### Prerequisites

- Rust 1.85 or higher
- Cargo (comes with Rust)
- GStreamer development libraries
- IPFS node (optional)

### Installing GStreamer

**macOS:**
```bash
brew install pkgconf gstreamer gst-plugins-base gst-plugins-good
```

**Debian/Ubuntu:**
```bash
sudo apt install pkg-config libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good
```

**Fedora/RHEL:**
```bash
sudo dnf install pkgconf gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1-plugins-good
```

**Arch Linux:**
```bash
sudo pacman -S pkgconf gstreamer gst-plugins-base gst-plugins-good
```

### Installation

```bash
git clone <repository-url>
cd sfu-server
cargo build --release
```

### Running Manually

```bash
# Development mode
cargo run

# Production mode
./target/release/sfu-server
```

---

## Configuration

Create a `.env` file in the project root (or copy from `.env.example`):

```env
# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
SFU_WEBSOCKET_URL=ws://localhost:8080/sfu
STUN_SERVER_URL=stun:stun.l.google.com:19302
RUST_LOG=info

# Recording
RECORDING_ENABLED=true
RECORDING_OUTPUT_DIR=./recordings

# IPFS (optional)
IPFS_ENABLED=true
IPFS_API_URL=http://127.0.0.1:5001
IPFS_GATEWAY_URL=http://127.0.0.1:8080/ipfs
IPFS_UPLOAD_TIMEOUT_SECS=300
```

### Configuration Options

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVER_HOST` | `0.0.0.0` | Host address to bind the server |
| `SERVER_PORT` | `8080` | Port number for the server |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |
| `RECORDING_ENABLED` | `true` | Enable/disable video recording |
| `RECORDING_OUTPUT_DIR` | `./recordings` | Directory for saved recordings |
| `IPFS_ENABLED` | `false` | Enable IPFS upload for recordings |
| `IPFS_API_URL` | `http://127.0.0.1:5001` | IPFS API endpoint |
| `IPFS_GATEWAY_URL` | `http://127.0.0.1:8080/ipfs` | IPFS gateway URL |

### Logging

Control log levels with `RUST_LOG`:

```bash
# Info level for all modules
RUST_LOG=info cargo run

# Debug level for sfu-server, info for others
RUST_LOG=info,sfu_server=debug cargo run

# Trace level for detailed debugging
RUST_LOG=trace cargo run
```

---

## Recording

The SFU server automatically records video/audio streams for each peer using GStreamer.

### How It Works

- Recording starts automatically when a peer joins a room
- Each session creates a unique file: `{peer_id}_{timestamp}.webm`
- When a peer leaves, the recording is finalized and saved
- If a peer rejoins, a new recording file is created (previous recording is preserved)

### Recording Format

- **Container**: WebM
- **Video Codec**: VP8
- **Audio Codec**: Opus

### File Location

Recordings are saved to:
- **Docker**: `./data/recordings/{room_id}/{peer_id}_{timestamp}.webm`
- **Manual**: Path configured via `RECORDING_OUTPUT_DIR`

### IPFS Upload

When `IPFS_ENABLED=true`, recordings are automatically uploaded to IPFS after saving:

1. File is uploaded via IPFS API
2. CID (Content Identifier) is logged
3. File is copied to IPFS MFS at `/recordings/{room_id}/`
4. Access via gateway: `http://localhost:8081/ipfs/{CID}`

View recordings in IPFS Web UI: `http://localhost:5001/webui` → Files → recordings

---

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
│   ├── sfu/                 # Core SFU logic
│   │   ├── mod.rs
│   │   ├── connection.rs    # Peer connection management
│   │   ├── room.rs          # Room and peer management
│   │   ├── server.rs        # SFU server implementation
│   │   ├── signaling.rs     # Signaling message handling
│   │   ├── track_manager.rs # Media track management
│   │   └── webrtc_utils.rs  # WebRTC utilities
│   ├── recording/           # Video recording with GStreamer
│   │   ├── mod.rs
│   │   ├── pipeline.rs      # GStreamer recording pipeline
│   │   ├── recorder.rs      # Recording manager
│   │   └── state.rs         # Recording state
│   └── ipfs/                # IPFS integration
│       └── mod.rs           # IPFS client for uploading recordings
├── data/                    # Runtime data (gitignored)
│   ├── recordings/          # Saved video recordings
│   ├── ipfs/                # IPFS node data
│   └── ipfs-staging/        # IPFS staging area
├── Dockerfile               # Multi-stage Docker build
├── docker-compose.yml       # Docker Compose services
├── Makefile                 # Convenience commands
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

**Using Docker (recommended):**
```bash
# Check server health
make cli-health

# Get server configuration
make cli-config

# Run all validation tests
make cli-validate

# Run any CLI command
make cli CMD="create-room --peer-id proctor1 --keep-alive"
make cli CMD="validate --scenario connection"
make cli CMD="interactive"
```

**Manual usage (without Docker):**
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
- `gstreamer` (0.22) - Video/audio recording pipeline
- `reqwest` (0.11) - HTTP client for IPFS API
- `clap` (4.4) - CLI argument parsing

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
