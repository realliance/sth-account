#!/usr/bin/env just --justfile

# Default recipe - show available commands
default:
    @just --list

# Start PostgreSQL database
db-up:
    docker compose up -d postgres
    @echo "Waiting for database to be ready..."
    @sleep 5
    @docker compose exec postgres pg_isready -U sth_user -d sth_account || (echo "Database not ready, waiting longer..." && sleep 10)

# Stop PostgreSQL database
db-down:
    docker compose down

# Reset database (stop, remove volumes, start fresh)
db-reset:
    docker compose down -v
    docker compose up -d postgres
    @echo "Waiting for database to be ready..."
    @sleep 10

# Show database logs
db-logs:
    docker compose logs -f postgres

# Connect to database with psql
db-shell:
    docker compose exec postgres psql -U sth_user -d sth_account

# Set environment variables for development
[private]
_set-env:
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="debug"

# Run migrations
migrate-up: db-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="info"
    nix develop --command bash -c "cargo run -p migration -- up"

# Rollback last migration
migrate-down: db-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="info"
    nix develop --command bash -c "cargo run -- migration down"

# Check migration status
migrate-status: db-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="info"
    nix develop --command bash -c "cargo run -- migration status"

# Reset database and apply all migrations
migrate-fresh: db-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="info"
    nix develop --command bash -c "cargo run -- migration fresh"

# Generate entities from database schema
generate-entities: migrate-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    cd entity && nix develop --command bash -c "sea-orm-cli generate entity -o src --with-serde both --model-extra-derives 'Default'"
    @echo "Entities generated in entity/src/"

# Build the project
build:
    nix develop --command bash -c "cargo build"

# Run the service
serve: migrate-up build
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="debug"
    export BIND_ADDRESS="127.0.0.1"
    export BIND_PORT="3000"
    nix develop --command bash -c "cargo run -- service"

# Run tests
test: db-up
    #!/usr/bin/env bash
    export DATABASE_URL="postgresql://sth_user:sth_password@localhost:5432/sth_account"
    export RUST_LOG="info"
    nix develop --command bash -c "cargo test"

# Clean up everything
clean: db-down
    nix develop --command bash -c "cargo clean"
    docker system prune -f

# Development setup - start database and run migrations
dev-setup: db-up migrate-up generate-entities
    @echo "Development environment ready!"
    @echo "Database URL: postgresql://sth_user:sth_password@localhost:5432/sth_account"
    @echo "Run 'just serve' to start the service"

# Quick development cycle - reset everything and regenerate
dev-reset: db-reset migrate-fresh generate-entities
    @echo "Development environment reset and ready!"

# Show database connection info
db-info:
    @echo "Database URL: postgresql://sth_user:sth_password@localhost:5432/sth_account"
    @echo "Host: localhost"
    @echo "Port: 5432"
    @echo "Database: sth_account"
    @echo "User: sth_user"
    @echo "Password: sth_password"
