use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
    #[serde(default = "default_log_db_path")]
    pub log_path: PathBuf,
}

fn default_port() -> u16 {
    std::env::var("GATEWAY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7788)
}

fn default_host() -> String {
    std::env::var("GATEWAY_HOST").unwrap_or_else(|_| "127.0.0.1".into())
}

fn default_db_path() -> PathBuf {
    get_data_dir().join("ccg_gateway.db")
}

fn default_log_db_path() -> PathBuf {
    get_data_dir().join("ccg_logs.db")
}

pub fn get_data_dir() -> PathBuf {
    // Priority 1: Custom environment variable
    if let Ok(dir) = std::env::var("CCG_DATA_DIR") {
        return PathBuf::from(dir);
    }

    // Priority 2: User home directory (cross-platform consistent)
    if let Some(home) = dirs::home_dir() {
        return home.join(".ccg-gateway");
    }

    // Fallback: Current directory
    PathBuf::from(".").join(".ccg-gateway")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                port: default_port(),
                host: default_host(),
            },
            database: DatabaseConfig {
                path: default_db_path(),
                log_path: default_log_db_path(),
            },
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Config::default()
    }
}
