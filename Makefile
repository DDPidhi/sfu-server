.PHONY: build up upbg down logs restart clean rebuild shell ipfs-shell ps init help

# Create .env from .env.example if it doesn't exist
init:
	@test -f .env || (cp .env.example .env && echo "Created .env from .env.example")

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

# View logs (follow mode)
logs:
	docker compose logs -f

# View logs for sfu-server only
logs-sfu:
	docker compose logs -f sfu-server

# View logs for ipfs only
logs-ipfs:
	docker compose logs -f ipfs

# Restart all services
restart:
	docker compose restart

# Rebuild and restart
rebuild: init
	docker compose down
	docker compose build --no-cache
	docker compose up -d

# Open shell in sfu-server container
shell:
	docker compose exec sfu-server /bin/bash

# Open shell in ipfs container
ipfs-shell:
	docker compose exec ipfs /bin/sh

# Run CLI commands inside the container
# Usage: make cli CMD="health" or make cli CMD="validate --all"
cli:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 $(CMD)

# CLI shortcuts
cli-health:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 health

cli-config:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 config

cli-validate:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --all

# IPFS-only CLI shortcuts
cli-ipfs-health:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-health

cli-ipfs-validate:
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-health && \
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-upload && \
	docker compose exec sfu-server ./sfu-cli --server localhost:8080 --ipfs http://ipfs:5001 validate --scenario ipfs-mfs

# Show running containers
ps:
	docker compose ps

# Pull latest IPFS image
pull:
	docker compose pull ipfs

# Show help
help:
	@echo "Available commands:"
	@echo ""
	@echo "Docker:"
	@echo "  make init       - Create .env from .env.example (auto-runs with build/up)"
	@echo "  make build      - Build Docker images"
	@echo "  make up         - Run containers with logs (foreground)"
	@echo "  make upbg       - Run containers in background"
	@echo "  make down       - Stop and remove containers"
	@echo "  make clean      - Stop containers and remove volumes/images"
	@echo "  make restart    - Restart all services"
	@echo "  make rebuild    - Rebuild images from scratch and restart"
	@echo "  make ps         - Show running containers"
	@echo "  make pull       - Pull latest IPFS image"
	@echo ""
	@echo "Logs:"
	@echo "  make logs       - Follow logs for all services"
	@echo "  make logs-sfu   - Follow logs for sfu-server only"
	@echo "  make logs-ipfs  - Follow logs for ipfs only"
	@echo ""
	@echo "Shell:"
	@echo "  make shell      - Open shell in sfu-server container"
	@echo "  make ipfs-shell - Open shell in ipfs container"
	@echo ""
	@echo "CLI (run inside container):"
	@echo "  make cli-health      - Check server health"
	@echo "  make cli-config      - Get server configuration"
	@echo "  make cli-validate    - Run all validation tests (SFU + IPFS)"
	@echo "  make cli-ipfs-health - Check IPFS health only"
	@echo "  make cli-ipfs-validate - Run all IPFS validation tests"
	@echo "  make cli CMD=\"<command>\" - Run any CLI command"
	@echo ""
	@echo "Examples:"
	@echo "  make cli CMD=\"create-room --peer-id proctor1 --keep-alive\""
	@echo "  make cli CMD=\"validate --scenario connection\""
	@echo "  make cli CMD=\"validate --scenario ipfs-upload\""
