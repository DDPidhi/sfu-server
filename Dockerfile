# Build stage
FROM rust:1.85-bookworm AS builder

# Install GStreamer development dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/cli.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src
COPY tests ./tests

# Touch main.rs to ensure it gets rebuilt
RUN touch src/main.rs && touch src/bin/cli.rs

# Build the actual application
RUN cargo build --release

# Test stage - for running unit tests
FROM builder AS tester

# Run tests (this stage is used by docker-compose test service)
CMD ["cargo", "test", "--", "--test-threads=1"]

# Runtime stage
FROM debian:bookworm-slim

# Install runtime GStreamer dependencies
RUN apt-get update && apt-get install -y \
    libgstreamer1.0-0 \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-tools \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built binary from builder stage
COPY --from=builder /app/target/release/sfu-server /app/sfu-server
COPY --from=builder /app/target/release/sfu-cli /app/sfu-cli

# Create recordings directory
RUN mkdir -p /app/recordings

# Default values (can be overridden by docker-compose or .env file)
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080
ENV RUST_LOG=info

EXPOSE 8080

# WebRTC UDP ports
EXPOSE 49152-65535/udp

CMD ["./sfu-server"]
