use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AppConfig {
    pub connections: HashMap<String, ConnectionConfig>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

impl AppConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read config file")?;
        let config: AppConfig = if content.trim_start().starts_with('{') {
            serde_json::from_str(&content).context("Failed to parse JSON config")?
        } else {
            toml::from_str(&content).context("Failed to parse TOML config")?
        };
        Ok(config)
    }
}
