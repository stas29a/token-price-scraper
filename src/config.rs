use std::fs;
use std::path::Path;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u32,
    pub get_price_duration_sec: u32,
    pub gecko_api_url: String,
    pub gecko_api_key: String,
    pub database_url: String,
}


impl Config {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let toml_str = fs::read_to_string(path)?;
        let config = toml::from_str(&toml_str)?;

        Ok(config)
    }
}