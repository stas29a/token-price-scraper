use crate::scraper::Token;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct TokenPrice {
    pub symbol: Token,
    pub timestamp: u64,
    pub price: String,
}
