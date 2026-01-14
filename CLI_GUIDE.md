# SFU CLI Validation Tool Guide

The SFU CLI (`sfu-cli`) is a command-line validation tool for testing and interacting with the SFU server.

## Installation

Build the CLI tool:

```bash
cargo build --release --bin sfu-cli
```

The compiled binary will be at: `./target/release/sfu-cli`

## Quick Reference

**Most Common Commands:**
```bash
# Check server health
sfu-cli health

# Create room (with keep-alive for testing)
sfu-cli create-room -p proctor1 -n "Dr. Smith" --keep-alive

# Join room as student
sfu-cli join-room -r <ROOM_ID> -p student1 -n "John"

# Run all validation tests
sfu-cli validate --all

# Interactive mode
sfu-cli interactive
```

## Usage

```bash
sfu-cli [OPTIONS] <COMMAND>
```

### Global Options

- `-s, --server <SERVER>` - Server address (default: `127.0.0.1:8080`)
- `-i, --ipfs <IPFS_URL>` - IPFS API URL (default: `http://localhost:5001`)
- `-h, --help` - Show help information

### Commands

- `health` - Check server health endpoint
- `config` - Get server configuration
- `connect` - Test WebSocket connection
- `create-room` - Create a room as proctor
- `join-room` - Join a room as student
- `validate` - Run automated validation scenarios
- `interactive` - Interactive mode for sending custom messages

## Important Notes

### Room Lifecycle

When creating a room, the connection must stay open to keep the room active. By default, the CLI closes connections after receiving responses, which causes rooms to be deleted immediately.

**Always use `--keep-alive`** when creating rooms for testing:
```bash
sfu-cli create-room -p proctor1 -n "Dr. Smith" --keep-alive
```

Without `--keep-alive`, you'll see this warning:
```
⚠ Note: Connection closed. Room will be deleted.
Use --keep-alive to keep the room active.
```

### Visual Workflow

**✅ Correct workflow (with --keep-alive):**
```
Terminal 1                    Terminal 2
─────────────────────────────────────────────────────
create-room --keep-alive
  ↓
Room 123456 created
Connection stays open ───────→ join-room -r 123456
[Listening for messages]          ↓
  ↓                            ✓ Join successful
Keep running...
  ↓
[Ctrl+C to exit]
```

**❌ Incorrect workflow (without --keep-alive):**
```
Terminal 1                    Terminal 2
─────────────────────────────────────────────────────
create-room
  ↓
Room 123456 created
  ↓
Connection closes
Room DELETED ────────────────→ join-room -r 123456
                                   ↓
                               ✗ Room not found!
```

## Command Examples

### 1. Health Check

Check if the server is running and healthy:

```bash
sfu-cli health
```

**Example Output:**
```
Checking server health...
✓ Health check passed
  Status: healthy
  Service: SFU Server
  Version: 1.0.0
```

### 2. Get Configuration

Retrieve server configuration:

```bash
sfu-cli config
```

**Example Output:**
```
Fetching server configuration...
✓ Config endpoint accessible

Configuration:
{
  "SFU_WEBSOCKET_URL": null,
  "STUN_SERVER_URL": null,
  "PROCTOR_UI_URL": null,
  "STUDENT_UI_URL": null
}
```

### 3. Test WebSocket Connection

Verify WebSocket connectivity:

```bash
sfu-cli connect
```

**Example Output:**
```
Testing WebSocket connection...
✓ WebSocket connection established
  URL: ws://127.0.0.1:8080/sfu
✓ Connection closed cleanly
```

### 4. Create Room (Proctor)

Create a new room as a proctor:

```bash
sfu-cli create-room --peer-id proctor1 --name "Dr. Smith"
```

**Example Output:**
```
Creating room...
  Proctor ID: proctor1
  Name: Dr. Smith
✓ CreateRoom message sent
Waiting for response...
✓ Room created successfully!

══════════════════════════════════════════════════
Room ID: 123456
══════════════════════════════════════════════════

⚠ Note: Connection closed. Room will be deleted.
Use --keep-alive to keep the room active.
```

**Important:** By default, the connection closes after creating the room, which will delete the room. To keep the room active for students to join, use the `--keep-alive` flag:

```bash
sfu-cli create-room --peer-id proctor1 --name "Dr. Smith" --keep-alive
```

**With --keep-alive:**
```
Creating room...
  Proctor ID: proctor1
  Name: Dr. Smith
✓ CreateRoom message sent
Waiting for response...
✓ Room created successfully!

══════════════════════════════════════════════════
Room ID: 123456
══════════════════════════════════════════════════

Connection is being kept alive...
Students can now join room: 123456
Press Ctrl+C to disconnect and close the room.
```

**Options:**
- `--peer-id, -p <ID>` - Proctor peer ID (required)
- `--name, -n <NAME>` - Proctor name (optional)
- `--keep-alive, -k` - Keep connection alive to maintain room (recommended for testing)

### 5. Join Room (Student)

Join an existing room as a student:

```bash
sfu-cli join-room --room-id 123456 --peer-id student1 --name "John Doe"
```

**Example Output:**
```
Joining room...
  Room ID: 123456
  Student ID: student1
  Name: John Doe
✓ JoinRequest message sent
Waiting for response...
✓ Join request sent to proctor
  Waiting for proctor approval...
```

**Options:**
- `--room-id, -r <ID>` - Room ID to join (required)
- `--peer-id, -p <ID>` - Student peer ID (required)
- `--name, -n <NAME>` - Student name (optional)

### 6. Run Validation Tests

Run automated validation scenarios to test server functionality.

#### Run All Tests

Validation tests automatically manage connections and don't require `--keep-alive` since they handle the full lifecycle internally.

```bash
sfu-cli validate --all
```

**Example Output:**
```
Running All Validation Tests
════════════════════════════════════════════════════════════

▶ Testing: connection
────────────────────────────────────────────────────────────
✓ WebSocket connection successful

▶ Testing: create-room
────────────────────────────────────────────────────────────
✓ Room created: 654321

▶ Testing: join-room
────────────────────────────────────────────────────────────
  Step 1: Creating room...
  ✓ Room created: 789012
  Step 2: Student joining room...
✓ Join request sent successfully

▶ Testing: multi-student
────────────────────────────────────────────────────────────
  Creating room for multi-student test...
  ✓ Room created: 345678
  Connecting student 1...
  Connecting student 2...
  Connecting student 3...
✓ All 3 students connected successfully

▶ Testing: invalid-room
────────────────────────────────────────────────────────────
  Attempting to join non-existent room...
✓ Request sent (server should handle gracefully)

════════════════════════════════════════════════════════════
Validation Summary
════════════════════════════════════════════════════════════
  ✓ Passed: 5
  ✗ Failed: 0
  Total: 5

All validations passed! 🎉
```

#### Run Specific Scenario

```bash
sfu-cli validate --scenario <SCENARIO_NAME>
```

**Available Scenarios:**

*SFU Server:*
- `connection` - Basic WebSocket connection test
- `create-room` - Room creation flow
- `join-room` - Student join flow
- `multi-student` - Multiple students joining
- `invalid-room` - Invalid room join (error handling)

*IPFS:*
- `ipfs-health` - Check IPFS node connectivity
- `ipfs-upload` - Upload test file to IPFS
- `ipfs-mfs` - Verify MFS (Mutable File System)

**Example:**
```bash
sfu-cli validate --scenario connection
sfu-cli validate --scenario ipfs-health
```

### 7. Interactive Mode

Interactive mode allows you to send custom JSON messages to the server:

```bash
sfu-cli interactive
```

**Example Session:**
```
Interactive Mode
════════════════════════════════════════════════════════════
Type help for help, quit to quit

✓ Connected to server
► {"type":"CreateRoom","peer_id":"test1","name":"Test"}
✓ Message sent
◀ {"type":"RoomCreated","room_id":"987654"}

► {"type":"Join","room_id":"987654","peer_id":"student1","name":"John","role":"student"}
✓ Message sent
◀ {"type":"join_success","message":"Successfully connected to SFU"}

► quit
Goodbye!
```

#### Interactive Commands

- `help` - Show example messages
- `quit` or `exit` - Exit interactive mode

#### Example Messages

**Create Room:**
```json
{"type":"CreateRoom","peer_id":"proctor1","name":"Dr. Smith"}
```

**Join Request:**
```json
{"type":"JoinRequest","room_id":"123456","peer_id":"student1","name":"John","role":"student"}
```

**Direct Join:**
```json
{"type":"Join","room_id":"123456","peer_id":"student1","name":"John","role":"student"}
```

**Leave:**
```json
{"type":"Leave","peer_id":"student1"}
```

**ICE Candidate:**
```json
{"type":"IceCandidate","peer_id":"student1","candidate":"candidate:...","sdp_mid":"0","sdp_mline_index":0}
```

## Using with Different Servers

### Remote Server

```bash
sfu-cli --server example.com:8080 health
```

### Different Port

```bash
sfu-cli --server 127.0.0.1:3000 connect
```

### HTTPS/WSS

For secure connections, update the server address accordingly:

```bash
sfu-cli --server wss://secure.example.com:443 connect
```

## Common Use Cases

### 1. Quick Server Check

Before running tests, verify the server is up:

```bash
sfu-cli health && sfu-cli connect
```

### 2. Full System Validation

Run all automated tests (SFU + IPFS):

```bash
sfu-cli validate --all
```

### 3. IPFS-Only Validation

Test IPFS connectivity and functionality:

```bash
# Check IPFS health
sfu-cli validate --scenario ipfs-health

# Test file upload
sfu-cli validate --scenario ipfs-upload

# Verify MFS
sfu-cli validate --scenario ipfs-mfs
```

For Docker environments, specify the IPFS URL:

```bash
sfu-cli --ipfs http://ipfs:5001 validate --scenario ipfs-health
```

### 3. Manual Room Testing

Create a room with keep-alive and let students join:

```bash
# Terminal 1: Create room and keep connection alive
sfu-cli create-room -p proctor_test -n "Test Proctor" --keep-alive
# Note the room ID from output (e.g., 123456)

# Terminal 2: Join with a student
sfu-cli join-room -r 123456 -p student_test -n "Test Student"
```

### 4. Stress Testing

Test multiple students joining the same room:

```bash
# First, create a room with keep-alive and note the ID
sfu-cli create-room -p proctor1 -n "Proctor" --keep-alive &
PROCTOR_PID=$!
sleep 2  # Wait for room creation

# Extract room ID from logs or use a known ID (e.g., 123456)
ROOM_ID=123456

# Then join with multiple students (in separate terminals or script)
for i in {1..10}; do
  sfu-cli join-room -r $ROOM_ID -p "student_$i" -n "Student $i" &
done

# Wait for all students to join
wait

# Kill the proctor process
kill $PROCTOR_PID
```

### 5. Debug Protocol Messages

Use interactive mode to test specific message sequences:

```bash
sfu-cli interactive
```

Then send messages step-by-step and observe server responses.

## Exit Codes

- `0` - Success
- `1` - Error or failure

Use exit codes in scripts:

```bash
if sfu-cli health; then
  echo "Server is healthy"
  sfu-cli validate --all
else
  echo "Server is not responding"
  exit 1
fi
```

## Troubleshooting

### Room Not Found / Proctor Not Found

**Error:** `Failed to send join request: Proctor not found for this room`

**Cause:** The room was created without `--keep-alive`, causing it to be deleted when the connection closed.

**Solution:**
1. Always use `--keep-alive` when creating rooms for manual testing:
   ```bash
   sfu-cli create-room -p proctor1 -n "Dr. Smith" --keep-alive
   ```
2. Keep the proctor terminal open (don't press Ctrl+C) while students join
3. Students must join while the proctor connection is active

**Workflow:**
```bash
# Terminal 1: Create and maintain room
sfu-cli create-room -p proctor1 --keep-alive
# Keep this running!

# Terminal 2: Join as student
sfu-cli join-room -r <ROOM_ID> -p student1
```

### Cannot Connect to Server

**Error:** `Cannot connect to server: Connection refused`

**Solution:**
1. Verify the server is running: `cargo run`
2. Check the correct port in `.env` file
3. Ensure no firewall is blocking the connection

### Timeout Waiting for Response

**Error:** `Timeout waiting for response`

**Solution:**
1. Check server logs for errors
2. Verify WebSocket endpoint is accessible
3. Ensure messages are properly formatted

### Invalid JSON

**Error:** `Invalid JSON. Type 'help' for examples.`

**Solution:**
1. Check JSON syntax (use double quotes, proper escaping)
2. Use the `help` command in interactive mode for examples
3. Validate JSON with online tools before sending

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: SFU Server Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v2

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build CLI
        run: cargo build --release --bin sfu-cli

      - name: Start Server
        run: cargo run &

      - name: Wait for Server
        run: sleep 5

      - name: Run Validations
        run: ./target/release/sfu-cli validate --all
```

### Shell Script Example

```bash
#!/bin/bash

# Start server in background
cargo run &
SERVER_PID=$!

# Wait for server to start
sleep 3

# Run health check
./target/release/sfu-cli health
if [ $? -ne 0 ]; then
  echo "Health check failed"
  kill $SERVER_PID
  exit 1
fi

# Run validations
./target/release/sfu-cli validate --all
VALIDATION_RESULT=$?

# Cleanup
kill $SERVER_PID

exit $VALIDATION_RESULT
```

## Best Practices

### For Manual Testing

1. **Always use `--keep-alive` for room creation:**
   ```bash
   sfu-cli create-room -p proctor1 --keep-alive
   ```

2. **Keep proctor terminal open:**
   - Don't close the proctor terminal until testing is complete
   - Students cannot join if the proctor disconnects
   - Press Ctrl+C only when done testing

3. **Use multiple terminals:**
   - Terminal 1: Proctor with `--keep-alive`
   - Terminal 2+: Students joining the room

4. **Test realistic scenarios:**
   - Create room with proctor
   - Have students join one by one
   - Test join request approvals
   - Test disconnections and rejoin

### For Automated Testing

1. **Use validation scenarios:**
   ```bash
   sfu-cli validate --all
   ```

2. **Validation tests handle lifecycle automatically:**
   - No need for `--keep-alive` in automated tests
   - Connections are managed programmatically

3. **Use in CI/CD pipelines:**
   - Health check first
   - Run full validation suite
   - Check exit codes for pass/fail

## Advanced Features

### Colored Output

The CLI uses colored output for better readability:
- ✓ Green - Success
- ✗ Red - Failure/Error
- ◀ Green - Received messages
- ► Cyan - User input
- Yellow - Warnings

### Verbose Mode

For detailed logging, set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug sfu-cli validate --all
```

## Performance

The CLI is optimized for quick validation:
- Health check: ~10ms
- WebSocket connection: ~50ms
- Full validation suite: ~5-10 seconds

## Security Considerations

- The CLI connects to servers without TLS by default
- For production servers, use WSS (WebSocket Secure)
- Never hardcode sensitive credentials in scripts
- Use environment variables for configuration

## Technical Details

### How --keep-alive Works

When you use `--keep-alive` with `create-room`:

1. **Connection established** - WebSocket connection opens
2. **Room created** - Server creates room and associates proctor with connection
3. **Connection maintained** - CLI keeps listening for messages instead of closing
4. **Room stays active** - Server keeps room and proctor registered
5. **Students can join** - While connection is open, students can join the room
6. **Cleanup on exit** - When you press Ctrl+C or connection closes, server removes proctor and deletes room

### Without --keep-alive

1. **Connection established** - WebSocket connection opens
2. **Room created** - Server creates room and associates proctor
3. **Connection closes** - CLI exits after receiving room ID
4. **Cleanup triggered** - Server detects closed connection
5. **Room deleted** - Proctor and room removed from server
6. **Students cannot join** - Room no longer exists

This behavior mimics real-world scenarios where a proctor disconnecting should end the exam session.

## Contributing

To add new validation scenarios:

1. Edit `src/bin/cli.rs`
2. Add a new validation function
3. Update the `run_scenario` match statement
4. Update `list_scenarios` function
5. Rebuild: `cargo build --release --bin sfu-cli`

## Support

For issues or questions:
- Check the main [README.md](README.md)
- Review [TESTING.md](TESTING.md) for test documentation
- File an issue on the project repository
