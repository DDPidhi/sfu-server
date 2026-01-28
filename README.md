# SFU Server

A Selective Forwarding Unit (SFU) server built with Rust for real-time WebRTC video conferencing and proctoring applications.

## Features

- WebRTC-based media routing using webrtc-rs
- Room-based peer management with proctor/student roles
- Video recording using GStreamer
- IPFS integration for distributed storage
- Blockchain integration with Polkadot Asset Hub

## Architecture Flow

![Architecture Flow](docs/architecture-flow.png)

### Feature Configuration Matrix

The three main features (Recording, IPFS, Blockchain) can be independently enabled/disabled:

| Recording | IPFS | Blockchain | Result |
|:---------:|:----:|:----------:|--------|
| ON | ON | ON | **Full functionality** - Local recording, IPFS backup, all events on-chain with CID |
| ON | ON | OFF | Local recording + IPFS distributed storage |
| ON | OFF | ON | Local recording + chain events (CID will be empty) |
| ON | OFF | OFF | Local recording only |
| OFF | ON | ON | Chain events only (no recordings, no CID) |
| OFF | OFF | ON | Chain events only (room/participant events) |
| OFF | ON | OFF | No-op - IPFS has nothing to store |
| OFF | OFF | OFF | WebRTC streaming only |

**Key Points:**
- **Recording** is independent - saves `.webm` files locally via GStreamer
- **IPFS** depends on Recording - uploads completed recordings, returns CID
- **Blockchain** is independent - logs all proctoring events, includes CID if available

## Setup

### Option 1: Docker (Recommended)

The easiest way to run the SFU server. All dependencies (GStreamer, IPFS) are pre-configured.

**Prerequisites:**
- Docker and Docker Compose
- Make

**Run:**
```bash
# Clone and enter the repo
git clone <repository-url>
cd sfu-server

# Start with logs in foreground
make up

# Or start in background
make upbg
```

On first run, `.env` is automatically created from `.env.example`.

**Make Commands:**

| Command | Description |
|---------|-------------|
| `make up` | Build and run with logs |
| `make upbg` | Build and run in background |
| `make down` | Stop containers |
| `make logs` | Follow logs |
| `make rebuild` | Clean rebuild |
| `make shell` | Shell into SFU container |
| `make help` | Show all commands |

**Services:**
- WebSocket: `ws://localhost:8080/sfu`
- Health Check: `http://localhost:8080/sfu/health`
- IPFS Web UI: `http://localhost:5001/webui`
- IPFS Gateway: `http://localhost:8081/ipfs/{CID}`

---

### Option 2: Mac (Without Docker)

Tested on macOS only.

**Prerequisites:**
- Rust 1.85+
- GStreamer
- IPFS node (optional)

**Install GStreamer:**
```bash
brew install pkgconf gstreamer gst-plugins-base gst-plugins-good
```

**Build and Run:**
```bash
# Clone and enter the repo
git clone <repository-url>
cd sfu-server

# Copy environment config
cp .env.example .env

# Build
cargo build --release

# Run
cargo run
```

**Services:**
- WebSocket: `ws://localhost:8080/sfu`
- Health Check: `http://localhost:8080/sfu/health`

---

## Configuration

Edit `.env` to customize settings. Copy from `.env.example` if not using Docker.

### Server

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVER_HOST` | `0.0.0.0` | Host address to bind the server |
| `SERVER_PORT` | `8080` | Port number for the server |
| `SFU_WEBSOCKET_URL` | `ws://localhost:8080/sfu` | WebSocket URL for clients to connect |
| `STUN_SERVER_URL` | `stun:stun.l.google.com:19302` | STUN server for ICE candidate gathering |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

### Recording

| Variable | Default | Description |
|----------|---------|-------------|
| `RECORDING_ENABLED` | `true` | Enable/disable video recording |
| `RECORDING_OUTPUT_DIR` | `./recordings` | Directory for saved recordings |

### IPFS

| Variable | Default | Description |
|----------|---------|-------------|
| `IPFS_ENABLED` | `true` | Enable IPFS upload for recordings |
| `IPFS_API_URL` | `http://127.0.0.1:5001` | IPFS API endpoint |
| `IPFS_GATEWAY_URL` | `http://127.0.0.1:8080/ipfs` | IPFS gateway URL for accessing files |
| `IPFS_UPLOAD_TIMEOUT_SECS` | `300` | Timeout for IPFS uploads in seconds |

### Blockchain (Polkadot Asset Hub)

| Variable | Default | Description |
|----------|---------|-------------|
| `ASSET_HUB_ENABLED` | `true` | Enable blockchain integration |
| `ASSET_HUB_RPC_URL` | `https://testnet-passet-hub-eth-rpc.polkadot.io` | RPC URL for Paseo Asset Hub EVM |
| `ASSET_HUB_PRIVATE_KEY` | - | Private key for signer account (hex with 0x prefix) |
| `ASSET_HUB_CONTRACT_ADDRESS` | `0x6b044B2951dAF31F37D1AdB6547EA673AdF56DBB` | Deployed proctoring contract address |
| `ASSET_HUB_SUBMISSION_TIMEOUT_SECS` | `30` | Transaction submission timeout |
| `ASSET_HUB_RETRY_COUNT` | `3` | Number of retries for failed transactions |
| `ASSET_HUB_GAS_LIMIT` | `500000` | Gas limit for transactions |

## WebSocket Protocol

Connect to `ws://localhost:8080/sfu` and exchange JSON messages.

### Room Management

**CreateRoom** - Proctor creates a new room
```json
{
  "type": "CreateRoom",
  "peer_id": "proctor_123",
  "name": "Dr. Smith",
  "wallet_address": "0x1234..."
}
```

**RoomCreated** - Server confirms room creation
```json
{
  "type": "RoomCreated",
  "room_id": "ABC123"
}
```

**JoinRequest** - Student requests to join (requires proctor approval)
```json
{
  "type": "JoinRequest",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "name": "John Doe",
  "role": "student",
  "wallet_address": "0xabcd..."
}
```

**JoinResponse** - Proctor approves/rejects join request
```json
{
  "type": "JoinResponse",
  "room_id": "ABC123",
  "peer_id": "proctor_123",
  "approved": true,
  "requester_peer_id": "student_456"
}
```

**Join** - Peer joins room (after approval or for proctor)
```json
{
  "type": "Join",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "name": "John Doe",
  "role": "student",
  "wallet_address": "0xabcd..."
}
```

**Leave** - Peer leaves room
```json
{
  "type": "Leave",
  "peer_id": "student_456"
}
```

### WebRTC Signaling

**Offer** - Client sends SDP offer
```json
{
  "type": "Offer",
  "sdp": "v=0\r\no=- ..."
}
```

**Answer** - Client sends SDP answer
```json
{
  "type": "Answer",
  "peer_id": "student_456",
  "sdp": "v=0\r\no=- ..."
}
```

**IceCandidate** - Exchange ICE candidates
```json
{
  "type": "IceCandidate",
  "peer_id": "student_456",
  "candidate": "candidate:0 1 UDP ...",
  "sdp_mid": "0",
  "sdp_mline_index": 0
}
```

**Renegotiate** - Renegotiate connection
```json
{
  "type": "Renegotiate",
  "sdp": "v=0\r\no=- ..."
}
```

**MediaReady** - Client media tracks ready
```json
{
  "type": "MediaReady",
  "peer_id": "student_456",
  "has_video": true,
  "has_audio": true
}
```

### Recording

**StartRecording** - Start recording a peer
```json
{
  "type": "StartRecording",
  "room_id": "ABC123",
  "peer_id": "student_456"
}
```

**RecordingStarted** - Server confirms recording started
```json
{
  "type": "RecordingStarted",
  "room_id": "ABC123",
  "peer_id": "student_456"
}
```

**StopRecording** - Stop recording a peer
```json
{
  "type": "StopRecording",
  "room_id": "ABC123",
  "peer_id": "student_456"
}
```

**RecordingStopped** - Server confirms recording stopped
```json
{
  "type": "RecordingStopped",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "file_path": "/recordings/ABC123/student_456_1234567890.webm",
  "cid": "QmXyz...",
  "ipfs_gateway_url": "http://localhost:8081/ipfs/QmXyz..."
}
```

**StopAllRecordings** - Stop all recordings in room
```json
{
  "type": "StopAllRecordings",
  "room_id": "ABC123"
}
```

**AllRecordingsStopped** - Server confirms all recordings stopped
```json
{
  "type": "AllRecordingsStopped",
  "room_id": "ABC123",
  "recordings": [
    {
      "peer_id": "student_456",
      "file_path": "/recordings/...",
      "cid": "QmXyz...",
      "ipfs_gateway_url": "http://..."
    }
  ]
}
```

**GetRecordingStatus** - Query recording status
```json
{
  "type": "GetRecordingStatus",
  "room_id": "ABC123"
}
```

**RecordingStatus** - Server returns recording status
```json
{
  "type": "RecordingStatus",
  "room_id": "ABC123",
  "recording_peers": ["student_456", "student_789"]
}
```

**RecordingError** - Recording error occurred
```json
{
  "type": "RecordingError",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "error": "Failed to initialize pipeline"
}
```

### Proctor Actions

**KickParticipant** - Proctor kicks a participant
```json
{
  "type": "KickParticipant",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "reason": "Violation of exam rules"
}
```

**ParticipantKicked** - Notification sent to kicked participant
```json
{
  "type": "ParticipantKicked",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "reason": "Violation of exam rules"
}
```

**ParticipantLeft** - Notification sent to proctor when participant leaves
```json
{
  "type": "ParticipantLeft",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "name": "John Doe"
}
```

### ID Verification

**StartIdVerification** - Proctor initiates ID verification
```json
{
  "type": "StartIdVerification",
  "room_id": "ABC123",
  "peer_id": "student_456"
}
```

**IdVerificationResult** - Proctor submits verification result
```json
{
  "type": "IdVerificationResult",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "status": "valid",
  "verified_by": "proctor_123"
}
```
Status values: `valid`, `invalid`, `pending`, `skipped`

### Suspicious Activity

**ReportSuspiciousActivity** - Report suspicious behavior
```json
{
  "type": "ReportSuspiciousActivity",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "activity_type": "tab_switch",
  "details": "Switched tabs 3 times"
}
```
Activity types: `multiple_devices`, `tab_switch`, `window_blur`, `screen_share`, `unauthorized_person`, `audio_anomaly`, `other`

**SuspiciousActivityReported** - Server acknowledges report
```json
{
  "type": "SuspiciousActivityReported",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "activity_type": "tab_switch"
}
```

### Exam Results

**SubmitExamResult** - Student submits exam score
```json
{
  "type": "SubmitExamResult",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "score": 85,
  "total": 100,
  "exam_name": "Final Exam"
}
```

**ExamResultSubmitted** - Server confirms exam result recorded
```json
{
  "type": "ExamResultSubmitted",
  "room_id": "ABC123",
  "peer_id": "student_456",
  "grade": 8500
}
```
Note: Grade is in basis points (8500 = 85.00%)
