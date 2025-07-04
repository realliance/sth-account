use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sth-account")]
#[command(about = "Small Turtle House Account API")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run database migrations
    Migration {
        #[command(subcommand)]
        command: MigrationCommand,
    },
    /// Start the REST API service
    Service,
    /// Start the queue worker
    Worker,
    /// Run scheduled maintenance jobs
    Jobs {
        /// The type of job to run
        job_type: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum MigrationCommand {
    /// Apply all pending migrations
    Up,
    /// Rollback the last migration
    Down,
    /// Generate a new migration
    Generate { name: String },
    /// Check migration status
    Status,
    /// Drop all tables and reapply migrations
    Fresh,
}
