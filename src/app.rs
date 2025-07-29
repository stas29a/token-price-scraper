use crate::config::Config;
use crate::data::TokenPrice;
use crate::persistence::{PriceRepository, TokenPriceModel};
use crate::scraper::{Scraper, Token};
use crate::web::run_web_server;
use log::{debug, error, info};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct App<S: Scraper, P: PriceRepository> {
    config: Config,
    scraper: S,
    repository: Arc<P>,
}

impl<S: Scraper, P: PriceRepository + 'static> App<S, P> {
    pub fn new(config: Config, scraper: S, repository: Arc<P>) -> Self {
        Self {
            config,
            scraper,
            repository,
        }
    }
    pub async fn run(&mut self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        let get_price_duration =
            std::time::Duration::from_secs(self.config.get_price_duration_sec as u64);

        let mut get_price_interval = tokio::time::interval(get_price_duration);
        let (price_sender, _price_receiver) = tokio::sync::broadcast::channel::<TokenPrice>(100);

        let server_fut = run_web_server(
            cancellation_token.clone(),
            price_sender.clone(),
            self.repository.clone(),
            self.config.host.clone(),
            self.config.port,
        );

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Cancellation requested, exiting...");
                    break;
                }
                _ = get_price_interval.tick() => {
                    let timestamp = chrono::Utc::now();

                    match self.scraper.get_price(Token::Bitcoin).await {
                        Ok(price) => {
                            debug!("Current BTC price: {price}");
                            let token_price_model = TokenPriceModel {
                                id: None,
                                symbol: Token::Bitcoin.to_string(),
                                created_at: timestamp.into(),
                                price: price.clone()
                            };

                            if let Err(e) = self.repository.save_price(token_price_model).await {
                                error!("Error saving price: {e}");
                            }

                            let token_price = TokenPrice {
                                symbol: Token::Bitcoin,
                                timestamp: timestamp.timestamp() as u64,
                                price: price.to_string(),
                            };

                            if let Err(e) = price_sender.send(token_price) {
                                error!("Error sending token price: {e}");
                            }
                        }
                        Err(e) => {
                            error!("Error fetching price: {e}");
                        }
                    }
                },
            }
        }

        server_fut.await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bigdecimal::{BigDecimal, FromPrimitive};
    use std::sync::Arc;
    use std::time::UNIX_EPOCH;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;
    use tracing_unwrap::{OptionExt, ResultExt};

    struct MockScraper {
        price: BigDecimal,
        fail: bool,
    }

    #[async_trait]
    impl Scraper for MockScraper {
        async fn get_price(&self, _token: Token) -> Result<BigDecimal, anyhow::Error> {
            if self.fail {
                Err(anyhow::anyhow!("Failed to fetch price"))
            } else {
                Ok(self.price.clone())
            }
        }
    }

    struct MockRepo {
        saved: Arc<Mutex<Vec<TokenPriceModel>>>,
        fail: bool,
    }

    #[async_trait]
    impl PriceRepository for MockRepo {
        async fn save_price(&self, model: TokenPriceModel) -> Result<(), anyhow::Error> {
            if self.fail {
                Err(anyhow::anyhow!("Failed to save"))
            } else {
                self.saved.lock().await.push(model);
                Ok(())
            }
        }

        async fn get_prices(
            &self,
            str_symbol: &str,
            timestamp_from: i64,
        ) -> anyhow::Result<Vec<TokenPriceModel>> {
            Ok(self
                .saved
                .lock()
                .await
                .iter()
                .filter(|m| {
                    m.symbol == str_symbol
                        && m.created_at
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_log()
                            .as_secs()
                            > timestamp_from as u64
                })
                .cloned()
                .collect::<Vec<_>>())
        }
    }

    #[tokio::test]
    async fn test_run_price_fetch_and_save() {
        let config = Config {
            get_price_duration_sec: 1,
            gecko_api_url: "".to_string(),
            gecko_api_key: "".to_string(),
            host: "127.0.0.1".to_string(),
            port: 0,
            database_url: "".to_string(),
        };
        let scraper = MockScraper {
            price: BigDecimal::from_f64(100.0f64).unwrap(),
            fail: false,
        };
        let saved = Arc::new(Mutex::new(vec![]));
        let repo = Arc::new(MockRepo {
            saved: saved.clone(),
            fail: false,
        });
        let mut app = App::new(config, scraper, repo);

        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            cancel.cancel();
        });

        let _ = app.run(token).await;
        let saved = saved.lock().await;
        assert!(!saved.is_empty());
        assert_eq!(saved[0].price, BigDecimal::from_f64(100.0).unwrap_or_log());
    }
}
