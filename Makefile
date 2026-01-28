.PHONY: build up upbg down logs restart clean rebuild shell ipfs-shell ps init help \
        test test-docker test-local test-unit test-integration \
        cli cli-health cli-config cli-validate cli-validate-sfu cli-validate-blockchain cli-validate-recording cli-validate-ipfs

# ============================================================================
# Initialization
# ============================================================================

# Create .env from .env.example if it doesn't exist
init:
	@test -f .env || (cp .env.example .env && echo "Created .env from .env.example")

# ============================================================================
# Docker Build & Run
# ============================================================================

# Build the Docker images
build: init
	docker compose build

# Run containers in foreground with logs
up: init
	docker compose up --build

# Run containers in background (detached)
upbg: init
	docker compose up --build -d

# Stop and remove containers
down:
	docker compose down

# Stop and remove containers, volumes, and images
clean:
	docker compose down -v --rmi local

# Restart all services
restart:
	docker compose restart

# Rebuild and restart
rebuild: init
	docker compose down
	docker compose build --no-cache
	docker compose up -d

# Show running containers
ps:
	docker compose ps

# Pull latest IPFS image
pull:
	docker compose pull ipfs

# ============================================================================
# Logs
# ============================================================================

# View logs (follow mode)
logs:
	docker compose logs -f

# View logs for sfu-server only
logs-sfu:
	docker compose logs -f sfu-server

# View logs for ipfs only
logs-ipfs:
	docker compose logs -f ipfs

# ============================================================================
# Shell Access
# ============================================================================

# Open shell in sfu-server container
shell:
	docker compose exec sfu-server /bin/bash

# Open shell in ipfs container
ipfs-shell:
	docker compose exec ipfs /bin/sh

# ============================================================================
# Testing
# ============================================================================

# Run all tests (Docker - recommended)
test: test-docker

# Run tests in Docker container (isolated, reproducible)
test-docker: init
	@echo "Running tests in Docker container..."
	docker compose build test
	docker compose run --rm test

# Run tests locally (requires Rust and GStreamer installed)
test-local:
	@echo "Running tests locally..."
	cargo test

# Run only unit tests (fast, no integration)
test-unit:
	@echo "Running unit tests..."
	cargo test --lib

# Run integration tests (requires running server)
test-integration: upbg
	@echo "Waiting for services to start..."
	@sleep 5
	@echo "Running integration tests..."
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --all

# ============================================================================
# CLI Commands (run inside container)
# ============================================================================

# Generic CLI command runner
# Usage: make cli CMD="health" or make cli CMD="validate --all"
cli:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 $(CMD)

# CLI using the dedicated cli service (one-off command)
# Usage: make cli-run CMD="validate --all"
cli-run:
	docker compose run --rm cli --server sfu-server:8080 --ipfs http://ipfs:5001 $(CMD)

# ============================================================================
# CLI Shortcuts - Basic
# ============================================================================

# Check server health
cli-health:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 health

# Get server configuration
cli-config:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 config

# Run ALL validation tests
cli-validate:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --all

# ============================================================================
# CLI Shortcuts - SFU Validation
# ============================================================================

# Run SFU-only validation tests
cli-validate-sfu:
	@echo "Running SFU validation tests..."
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario connection
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario create-room
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario join-room
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario multi-student
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario invalid-room

# ============================================================================
# CLI Shortcuts - Blockchain Validation
# ============================================================================

# Run all blockchain validation tests
cli-validate-blockchain:
	@echo "Running blockchain validation tests..."
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-status
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-rpc
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-contract
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-functions

# Check blockchain status only
cli-blockchain-status:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-status

# Test blockchain RPC connectivity
cli-blockchain-rpc:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-rpc

# Validate blockchain contract
cli-blockchain-contract:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-contract

# Test contract read functions
cli-blockchain-functions:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario blockchain-functions

# ============================================================================
# CLI Shortcuts - Recording Validation
# ============================================================================

# Run recording validation tests
cli-validate-recording:
	@echo "Running recording validation tests..."
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario recording-status

# Check recording status
cli-recording-status:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario recording-status

# ============================================================================
# CLI Shortcuts - IPFS Validation
# ============================================================================

# Run all IPFS validation tests
cli-validate-ipfs:
	@echo "Running IPFS validation tests..."
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-health
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-upload
	@docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-mfs

# Check IPFS health only
cli-ipfs-health:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-health

# Test IPFS upload
cli-ipfs-upload:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-upload

# Test IPFS MFS
cli-ipfs-mfs:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-mfs

# ============================================================================
# CLI Shortcuts - Interactive & Room Management
# ============================================================================

# Start interactive CLI mode
cli-interactive:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 interactive

# Test WebSocket connection
cli-connect:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 connect

# ============================================================================
# Help
# ============================================================================

help:
	@echo "╔══════════════════════════════════════════════════════════════════╗"
	@echo "║                    SFU Server Makefile Commands                  ║"
	@echo "╚══════════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "Docker:"
	@echo "  make init          - Create .env from .env.example"
	@echo "  make build         - Build Docker images"
	@echo "  make up            - Run containers with logs (foreground)"
	@echo "  make upbg          - Run containers in background"
	@echo "  make down          - Stop and remove containers"
	@echo "  make clean         - Stop containers and remove volumes/images"
	@echo "  make restart       - Restart all services"
	@echo "  make rebuild       - Rebuild images from scratch and restart"
	@echo "  make ps            - Show running containers"
	@echo "  make pull          - Pull latest IPFS image"
	@echo ""
	@echo "Logs:"
	@echo "  make logs          - Follow logs for all services"
	@echo "  make logs-sfu      - Follow logs for sfu-server only"
	@echo "  make logs-ipfs     - Follow logs for ipfs only"
	@echo ""
	@echo "Shell:"
	@echo "  make shell         - Open shell in sfu-server container"
	@echo "  make ipfs-shell    - Open shell in ipfs container"
	@echo ""
	@echo "Testing:"
	@echo "  make test          - Run tests in Docker (recommended)"
	@echo "  make test-docker   - Run tests in Docker container"
	@echo "  make test-local    - Run tests locally (requires Rust)"
	@echo "  make test-unit     - Run unit tests only"
	@echo "  make test-integration - Run integration tests (starts services)"
	@echo ""
	@echo "CLI Validation (all):"
	@echo "  make cli-health    - Check server health"
	@echo "  make cli-config    - Get server configuration"
	@echo "  make cli-validate  - Run ALL validation tests"
	@echo ""
	@echo "CLI Validation (by category):"
	@echo "  make cli-validate-sfu        - Run SFU server tests"
	@echo "  make cli-validate-blockchain - Run blockchain tests"
	@echo "  make cli-validate-recording  - Run recording tests"
	@echo "  make cli-validate-ipfs       - Run IPFS tests"
	@echo ""
	@echo "CLI Validation (individual):"
	@echo "  make cli-blockchain-status    - Check blockchain config"
	@echo "  make cli-blockchain-rpc       - Test RPC connectivity"
	@echo "  make cli-blockchain-contract  - Validate contract"
	@echo "  make cli-blockchain-functions - Test contract functions"
	@echo "  make cli-recording-status     - Check recording config"
	@echo "  make cli-ipfs-health          - Check IPFS health"
	@echo "  make cli-ipfs-upload          - Test IPFS upload"
	@echo "  make cli-ipfs-mfs             - Test IPFS MFS"
	@echo ""
	@echo "CLI Interactive:"
	@echo "  make cli-interactive - Start interactive CLI mode"
	@echo "  make cli-connect     - Test WebSocket connection"
	@echo "  make cli CMD=\"<cmd>\" - Run any CLI command"
	@echo ""
	@echo "Examples:"
	@echo "  make cli CMD=\"create-room --peer-id proctor1 --keep-alive\""
	@echo "  make cli CMD=\"validate --scenario connection\""
	@echo "  make test-docker && make upbg && make cli-validate"
