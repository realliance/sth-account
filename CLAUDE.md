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

The project is in early bootstrap phase with placeholder implementations in main.rs, the migration library, and entity definitions.
