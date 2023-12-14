use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_api::storage::StorageS3Args;
use dash_pipe_provider::{
    deltalake::{arrow::array::Int64Array, datafusion::dataframe::DataFrame},
    storage::{
        lakehouse::{GlobalStorageContext, StorageContext},
        MetadataStorage,
    },
    PipeMessage,
};
use futures::Future;
use kube::Client;
use tokio::{
    spawn,
    sync::{mpsc, RwLock},
    task::JoinHandle,
    time::sleep,
};
use tracing::{debug, info, instrument, Level};

use crate::{
    metric::{MetricDuration, MetricSpan, MetricSpanKind},
    plan::Plan,
    world::{Namespace, NodeMetric, World},
};

#[async_trait]
pub trait OptimizerService {
    fn new(ctx: &OptimizerContext) -> Self
    where
        Self: Sized;

    async fn loop_forever(self) -> Result<()>
    where
        Self: Sized;
}

#[derive(Clone)]
pub struct OptimizerContext {
    pub(crate) kube: Arc<Client>,
    plan_tx: Arc<mpsc::Sender<Box<dyn Plan>>>,
    storage: GlobalStorageContext,
    pub(crate) world: Arc<RwLock<World>>,
}

/// Init
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_default() -> Result<(OptimizerContext, mpsc::Receiver<Box<dyn Plan>>)> {
        info!("creating optimizer context");
        let (plan_tx, plan_rx) = mpsc::channel(1024);
        let kube = Client::try_default().await?;
        let namespace = kube.default_namespace().into();

        let flush = Some(Duration::from_secs(10));
        let ctx = Self {
            kube: Arc::new(kube),
            plan_tx: Arc::new(plan_tx),
            storage: GlobalStorageContext::new(StorageS3Args::try_parse()?, flush, namespace),
            world: Arc::default(),
        };

        Ok((ctx, plan_rx))
    }
}

/// Storage
impl OptimizerContext {
    async fn get_table(&self, name: &str) -> Result<StorageContext> {
        self.storage.get_table(name).await
    }

    pub(crate) async fn get_metric(
        &self,
        kind: MetricSpanKind<'_>,
        duration: MetricDuration,
    ) -> Result<NodeMetric> {
        let condition = match kind {
            MetricSpanKind::Messenger { topic, type_ } => {
                format!("kind = 'Messenger' AND topic = '{topic}' AND type = '{type_}'")
            }
            MetricSpanKind::MetadataStorage { type_ } => {
                format!("kind = 'MetadataStorage' AND type = '{type_}'")
            }
            MetricSpanKind::Storage { type_ } => {
                format!("kind = 'Storage' AND type = '{type_}'")
            }
        };

        let MetricDuration { begin_ns, end_ns } = duration;
        let condition = format!("{condition} AND begin_ns >= {begin_ns} AND end_ns < {end_ns}");

        let columns = "sum(end_ns - begin_ns) as elapsed_ns, sum(len) as bytes, count(1) as len";
        let sql = format!("SELECT {columns} FROM optimizer_metric WHERE {condition}");
        debug!("SQL = {sql}");

        let df = self.query_metric(&sql).await?;
        let data = df
            .collect()
            .await?
            .pop()
            .ok_or_else(|| anyhow!("empty row"))?;

        let get_value = |name| {
            data.column_by_name(name)
                .map(|array| {
                    array
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .map(|array| array.iter().next().flatten())
                        .ok_or_else(|| anyhow!("cannot parse column: {name}"))
                })
                .transpose()
                .map(Option::flatten)
                .map(Option::unwrap_or_default)
        };

        Ok(NodeMetric {
            elapsed_ns: get_value("elapsed_ns")?,
            len: get_value("len")?,
            total_bytes: get_value("bytes")?,
        })
    }

    pub(crate) async fn get_metric_with_last(
        &self,
        kind: MetricSpanKind<'_>,
        duration: Duration,
    ) -> Result<NodeMetric> {
        fn parse_ns(duration: Duration) -> Result<u64> {
            duration
                .as_nanos()
                .try_into()
                .map_err(|error| anyhow!("failed to convert duration: {error}"))
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let begin = now
            .checked_sub(duration)
            .ok_or_else(|| anyhow!("too large duration"))?;
        let end = now;

        let duration = MetricDuration {
            begin_ns: parse_ns(begin)?,
            end_ns: parse_ns(end)?,
        };

        self.get_metric(kind, duration).await
    }

    async fn query_metric(&self, sql: &str) -> Result<DataFrame> {
        let table = self.get_table("optimizer-metric").await?;
        let df = table.sql(sql).await?;
        Ok(df)
    }

    pub(crate) async fn write_metric(&self, span: MetricSpan<'_>) -> Result<()> {
        let table = self.get_table("optimizer-metric").await?;
        table.put_metadata(&[&PipeMessage::new(span)]).await
    }
}

/// Tasks
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn add_plan<P>(&self, plan: P) -> Result<()>
    where
        P: Plan,
    {
        self.plan_tx
            .send(Box::new(plan))
            .await
            .map_err(|error| anyhow!("failed to add plan: {error}"))
    }

    pub fn spawn_task<F, Fut>(&self, f: F) -> JoinHandle<()>
    where
        F: FnOnce(Self) -> Fut,
        Fut: 'static + Send + Future<Output = ()>,
    {
        spawn(f(self.clone()))
    }
}

/// World
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub async fn get(
        &self,
        namespace: &str,
        name: &str,
        timeout: Timeout,
    ) -> Option<Arc<RwLock<Namespace>>> {
        const INTERVAL: Duration = Duration::from_millis(100);

        let timeout = match timeout {
            Timeout::Duration(timeout) => Some(timeout),
            Timeout::Unlimited => None,
            Timeout::Instant => {
                if self.exists(namespace, name).await {
                    None
                } else {
                    return None;
                }
            }
        };

        let mut elapsed = Duration::default();
        let is_timeout = || async move {
            elapsed += INTERVAL;
            sleep(INTERVAL).await;

            matches!(timeout, Some(timeout) if timeout <= elapsed)
        };

        let storage = loop {
            let storage_session = self.world.read().await;
            match storage_session.get(namespace) {
                Some(storage) => break storage,
                None => {
                    drop(storage_session);
                    if is_timeout().await {
                        return None;
                    }
                }
            }
        };

        loop {
            let storage_session = storage.read().await;
            if storage_session.is_ready(name) {
                drop(storage_session);
                break Some(storage);
            } else {
                drop(storage_session);
                if is_timeout().await {
                    return None;
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    async fn exists(&self, namespace: &str, name: &str) -> bool {
        let world_session = self.world.read().await;
        match world_session.get(namespace) {
            Some(namespace) => {
                drop(world_session);
                namespace.read().await.exists(name)
            }
            None => false,
        }
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Timeout {
    Duration(Duration),
    Instant,
    Unlimited,
}
