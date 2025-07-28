use std::fmt::{Display, Formatter};
use anyhow::bail;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, FromPrimitive};
use log::error;
use reqwest::Client;
use serde::{Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Token {
    Bitcoin,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Bitcoin => f.collect_str("bitcoin"),
        }
    }
}

#[async_trait]
pub trait Scraper {
    async fn get_price(&self, token: Token) -> anyhow::Result<BigDecimal>;
}

pub struct GeckoScraper {
    client: Client,
    base_api_url: String,
    api_key: String,
}

impl GeckoScraper {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_api_url: base_url,
            api_key,
        }
    }
}

#[async_trait]
impl Scraper for GeckoScraper {
    async fn get_price(&self, token: Token) -> anyhow::Result<BigDecimal> {
        let str_token = token.to_string();
        let base_url = &self.base_api_url;
        let url = format!("{base_url}/api/v3/simple/price?ids={str_token}&vs_currencies=usd");
        let response = self
            .client
            .get(&url)
            .header("x-cg-demo-api-key", self.api_key.clone())
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let opt_price = response
            .get(str_token)
            .and_then(|r| r.get("usd"))
            .and_then(|price| price.as_f64())
            .and_then(BigDecimal::from_f64);

        if let Some(price) = opt_price {
            Ok(price)
        } else {
            error!("Can't get price, response {response}");
            bail!("Can't get price")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[tokio::test]
    async fn test_get_price() {
        use crate::scraper::Scraper;
        use crate::scraper::Token;
        use bigdecimal::BigDecimal;
        use mockito::mock;

        let token = Token::Bitcoin;
        let price = 12345.67_f64;
        let _m = mock("GET", "/api/v3/simple/price")
            .match_query(mockito::Matcher::UrlEncoded("ids".into(), "bitcoin".into()))
            .match_query(mockito::Matcher::UrlEncoded(
                "vs_currencies".into(),
                "usd".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("{{\"bitcoin\":{{\"usd\":{price}}}}}"))
            .create();

        let scraper = GeckoScraper::new(mockito::server_url(), "test_api_key".to_string());
        let result = scraper.get_price(token).await.unwrap();
        let price = BigDecimal::from_f64(price).unwrap();

        assert_eq!(result, price);
    }
}
