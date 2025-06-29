use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub bind_port: u16,
}

impl Config {
    pub fn from_env() -> crate::error::Result<Self> {
        Ok(Config {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/sth_account".to_string()),
            bind_address: env::var("BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            bind_port: env::var("BIND_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .map_err(|_| crate::error::AppError::Config(
                    "Invalid BIND_PORT value".to_string()
                ))?,
        })
    }
}