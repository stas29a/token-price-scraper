use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u32,
    pub get_price_duration_sec: u32,
    pub gecko_url: String,
    pub gecko_api_url: String,
    pub database_url: String,
}
