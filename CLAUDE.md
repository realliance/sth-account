# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**sth-account** is a Rust-based web API project for "Small Turtle House Account API" using SeaORM for database operations and Tokio for async runtime. The project uses a clean architecture pattern with separate entity and migration modules.

## Development Environment

Use Nix for development environment setup:
```bash
# Enter development shell
nix develop

# Or using legacy Nix
nix-shell
```

This provides Rust toolchain, SeaORM CLI, and required dependencies.

## Build Commands

```bash
# Build the project
cargo build

# Build optimized release
cargo build --release

# Build via Nix (produces stripped binary)
nix build

# Build Docker image
nix build .#dockerImage
```

## Database Operations

The project uses SeaORM with PostgreSQL. All migration commands should be run from the `migration/` directory:

```bash
cd migration

# Apply all pending migrations
cargo run

# Generate new migration
cargo run -- generate MIGRATION_NAME

# Check migration status
cargo run -- status

# Rollback last migration
cargo run -- down

# Fresh install (drop all tables, reapply migrations)
cargo run -- fresh
```

## Architecture

The project follows a Cargo workspace structure:

- **Root crate (`sth-account`)**: Main application binary
- **`entity/`**: Database entity definitions using SeaORM
- **`migration/`**: Database migration management with SeaORM Migration CLI

Key dependencies:
- **SeaORM**: Async ORM with PostgreSQL support
- **Tokio**: Async runtime with full features
- **Rust Edition 2024** with toolchain version 1.88

## Development Status

**Phase 1 Complete**: Foundation with CLI structure, database migrations, and entity generation.

Current state:
- ✅ Multi-mode CLI (Migration, Service, Worker, Jobs)
- ✅ Database migrations for core entities (User, Bot, Match, etc.)
- ✅ Generated SeaORM entities from database schema
- ✅ Health check endpoints
- ✅ Docker Compose development environment

## Development Environment Setup

Use the provided development tools:

```bash
# Using justfile (if available)
just dev-setup

# Or using bash script
./dev-setup.sh dev-setup
```

Available commands:
- `db-up` - Start PostgreSQL database
- `migrate-up` - Apply database migrations  
- `generate` - Generate entities from schema
- `build` - Build the project
- `serve` - Start the service
- `dev-reset` - Reset everything and regenerate
