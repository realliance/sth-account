use clap::Parser;
use std::process;
use tracing::info;

mod api;
mod auth;
mod cli;
mod config;
mod database;
mod error;
mod health;
mod jobs;
mod openapi;
mod queue;
mod service;
#[cfg(test)]
mod test_utils;
mod worker;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    // Initialize tracing with enhanced formatting
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sth_account=debug,tower_http=debug,axum=debug".into()),
        )
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(true)
        .with_ansi(true)
        .init();

    let cli = Cli::parse();
    info!("STH Account CLI starting with command: {:?}", cli.command);

    let result = match cli.command {
        Commands::Migration { command } => {
            info!("Running migration command: {:?}", command);
            let migration_cmd = match command {
                cli::MigrationCommand::Up => migration::MigrationCommand::Up,
                cli::MigrationCommand::Down => migration::MigrationCommand::Down,
                cli::MigrationCommand::Status => migration::MigrationCommand::Status,
                cli::MigrationCommand::Fresh => migration::MigrationCommand::Fresh,
                cli::MigrationCommand::Generate { name } => {
                    migration::MigrationCommand::Generate { name }
                }
            };
            migration::run_migration(migration_cmd)
                .await
                .map_err(|e| error::AppError::Migration(e.to_string()))
        }
        Commands::Service => {
            info!("Starting service mode");
            service::run_service().await
        }
        Commands::Worker => {
            info!("Starting worker mode");
            worker::run_worker().await
        }
        Commands::Jobs { job_type } => {
            info!("Running job: {}", job_type);
            jobs::run_job(job_type).await
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
