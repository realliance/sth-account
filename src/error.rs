use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("Service error: {0}")]
    Service(String),
    
    #[error("Worker error: {0}")]
    Worker(String),
    
    #[error("Job error: {0}")]
    Job(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}