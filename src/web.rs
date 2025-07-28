use crate::data::TokenPrice;
use crate::persistence::PriceRepository;
use crate::scraper::Token;
use crate::scraper::Token::Bitcoin;
use axum::extract::ws::Message;
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::routing::get;
use axum::{Json, Router};
use log::{debug, error, info};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_http::services::ServeDir;

struct AppState<P: PriceRepository> {
    repository: Arc<P>,
}

impl<P: PriceRepository> Clone for AppState<P> {
    fn clone(&self) -> Self {
        AppState {
            repository: self.repository.clone(),
        }
    }
}

pub fn run_web_server(
    cancellation_token: CancellationToken,
    price_sender: Sender<TokenPrice>,
    repository: Arc<impl PriceRepository + 'static>,
    host: String,
    port: u32,
) -> JoinHandle<()> {
    let mut router = Router::new()
        .route("/api/v1/prices", get(get_prices_handler))
        .with_state(AppState {
            repository: repository.clone(),
        });

    router = serve_static_dir(router);
    router = configure_ws(router, cancellation_token.clone(), price_sender.clone());

    let _cancellation_token = cancellation_token.clone();
    let url = format!("{host}:{port}");

    tokio::spawn(async move {
        serve(_cancellation_token, router, url).await;
    })
}

fn serve_static_dir(router: Router) -> Router {
    router.nest_service("/", ServeDir::new("static"))
}

async fn get_prices_handler(
    State(state): State<AppState<impl PriceRepository>>,
) -> Json<Vec<TokenPrice>> {
    let timestamp_from = chrono::Utc::now().timestamp() - 60 * 15; //15 minutes ago
    let symbol = Bitcoin.to_string();

    let prices = state
        .repository
        .get_prices(&symbol, timestamp_from)
        .await
        .map(|models| {
            models
                .into_iter()
                .map(|model| TokenPrice {
                    symbol: Token::Bitcoin,
                    timestamp: model
                        .created_at
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    price: model.price.to_string(),
                })
                .collect()
        })
        .unwrap_or_else(|e| {
            error!("Error fetching prices: {e}");
            vec![]
        });

    axum::response::Json(prices)
}

fn configure_ws(
    router: Router,
    cancellation_token: CancellationToken,
    price_sender: Sender<TokenPrice>,
) -> Router {
    router.route(
        "/ws/prices",
        get(
            |ws: WebSocketUpgrade,
             connect_info: ConnectInfo<SocketAddr>| async move {
                debug!("Connected {connect_info:?}");

                ws.on_upgrade(async move |mut socket| {
                    debug!("Connected  upgrade {connect_info:?}");

                    if socket.send(Message::Ping(vec![1, 2, 3])).await.is_ok() {
                        debug!("Pinged ...");
                    } else {
                        error!("Could not send ping !");
                    }

                    let mut price_receiver: Receiver<TokenPrice> = price_sender.subscribe();
                    loop {
                        tokio::select! {
                              data = price_receiver.recv() => {
                                  match data {
                                      Ok(dump) => {
                                          let msg = serde_json::to_string(&dump).unwrap();
                                          if let Err(e) = socket.send(Message::Text(msg)).await {
                                              error!("Error sending message: {e}");
                                              break;
                                          }
                                      }
                                      Err(_) => {
                                          error!("Receiver channel closed");
                                          break;
                                      }
                                  }
                              },
                              _ = cancellation_token.cancelled() => {
                                    debug!("Cancellation requested, closing WebSocket connection...");
                                    if let Err(e) = socket.close().await {
                                        error!("Error closing WebSocket: {e}");
                                    }
                                    break;
                                }
                        }
                    }
                })
            },
        ),
    )
}

async fn serve(cancellation_token: CancellationToken, app: Router, addr: String) {
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("listening on {}", listener.local_addr().unwrap());

    tokio::select! {
        _ = cancellation_token.cancelled() => {
            info!("Cancellation requested, exiting...");
        }
        _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()) => {
            info!("Server stopped");
        }
    }
}
