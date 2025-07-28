use async_trait::async_trait;
use bigdecimal::BigDecimal;
use diesel::associations::HasTable;
use diesel::r2d2::ConnectionManager;
use diesel::{
    ExpressionMethods, Insertable, PgConnection, QueryDsl, Queryable, RunQueryDsl, Selectable,
    SelectableHelper,
};
use log::{debug, error, info};
use r2d2::Pool;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::prices)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Debug, Clone)]
pub struct TokenPriceModel {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub created_at: SystemTime,
    pub symbol: String,
    pub price: BigDecimal,
}

#[async_trait]
pub trait PriceRepository: Send + Sync {
    async fn save_price(&self, price: TokenPriceModel) -> anyhow::Result<()>;
    async fn get_prices(
        &self,
        str_symbol: &str,
        timestamp_from: i64,
    ) -> anyhow::Result<Vec<TokenPriceModel>>;
}

pub struct PriceRepositoryImpl {
    actor_sender: Sender<TokenPriceModel>,
    pg_pool: Arc<Pool<ConnectionManager<PgConnection>>>,
}

impl PriceRepositoryImpl {
    pub async fn new(
        cancellation_token: CancellationToken,
        pg_pool: Arc<Pool<ConnectionManager<PgConnection>>>,
    ) -> anyhow::Result<Self> {
        let actor = PriceRepositoryActor::new(cancellation_token, pg_pool.clone());
        let actor_sender = actor.run().await?;

        Ok(Self {
            actor_sender,
            pg_pool,
        })
    }
}

#[async_trait]
impl PriceRepository for PriceRepositoryImpl {
    async fn save_price(&self, price: TokenPriceModel) -> anyhow::Result<()> {
        self.actor_sender.send(price).await.map_err(|e| {
            error!("Failed to send price to actor: {e}");
            anyhow::anyhow!("Failed to send price to actor")
        })?;
        Ok(())
    }

    async fn get_prices(
        &self,
        str_symbol: &str,
        timestamp_from: i64,
    ) -> anyhow::Result<Vec<TokenPriceModel>> {
        use crate::schema::prices::dsl::*;
        let mut connection = self.pg_pool.get()?;

        let fetched_prices = prices::table()
            .filter(symbol.eq(str_symbol))
            .filter(
                created_at
                    .gt(SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(timestamp_from as u64)),
            )
            .select(TokenPriceModel::as_select())
            .load::<TokenPriceModel>(&mut connection)
            .map_err(|e| {
                error!("Failed to load prices: {e}");
                anyhow::anyhow!("Failed to load prices")
            })?
            .into_iter()
            .collect::<Vec<_>>();

        Ok(fetched_prices)
    }
}

struct PriceRepositoryActor {
    cancellation_token: CancellationToken,
    pg_pool: Arc<Pool<ConnectionManager<PgConnection>>>,
}

impl PriceRepositoryActor {
    pub fn new(
        cancellation_token: CancellationToken,
        pg_pool: Arc<Pool<ConnectionManager<PgConnection>>>,
    ) -> Self {
        Self {
            cancellation_token,
            pg_pool,
        }
    }

    pub async fn run(&self) -> anyhow::Result<Sender<TokenPriceModel>> {
        use crate::schema::prices::dsl::*;
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<TokenPriceModel>(100);

        let cancellation_token = self.cancellation_token.clone();
        let pg_pool = self.pg_pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(price_model) = receiver.recv() => {
                        debug!("Saving price: {price:?}");
                        match pg_pool.get() {
                            Ok(mut conn) =>  {
                                diesel::insert_into(prices::table())
                                    .values(&price_model)
                                    .execute(&mut conn)
                                    .map_err(|e| {
                                        error!("Failed to insert price: {e}");
                                        anyhow::anyhow!("Failed to insert price")
                                    })?;
                            },
                            Err(e) => {
                                error!("Failed to get database connection: {e}");
                                continue;
                            }
                        };
                    }
                    _ = cancellation_token.cancelled() => {
                        info!("Cancellation requested, stopping price repository actor...");
                        break;
                    }
                }
            }

            anyhow::Ok(())
        });

        Ok(sender)
    }
}
