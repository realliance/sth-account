use std::env;

#[derive(Debug, Clone)]
pub enum SessionStore {
    Memory,
    Redis { url: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub session_store: SessionStore,
}

impl Config {
    pub fn from_env() -> crate::error::Result<Self> {
        let session_store = match env::var("SESSION_STORE_TYPE").as_deref() {
            Ok("redis") => {
                let redis_url = env::var("SESSION_REDIS_URL")
                    .unwrap_or_else(|_| "redis://localhost:6379".to_string());
                SessionStore::Redis { url: redis_url }
            }
            _ => SessionStore::Memory,
        };

        Ok(Config {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/sth_account".to_string()),
            bind_address: env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0".to_string()),
            bind_port: env::var("BIND_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .map_err(|_| {
                    crate::error::AppError::Config("Invalid BIND_PORT value".to_string())
                })?,
            session_store,
        })
    }
}
