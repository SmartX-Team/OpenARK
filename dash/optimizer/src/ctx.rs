use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_api::{model_claim::ModelClaimBindingPolicy, storage::ModelStorageCrd};
use dash_pipe_api::storage::StorageS3Args;
use dash_pipe_provider::{
    deltalake::{arrow::array::Int64Array, datafusion::dataframe::DataFrame},
    storage::{
        lakehouse::{GlobalStorageContext, StorageContext},
        MetadataStorage,
    },
    PipeMessage,
};
use kube::Client;
use tokio::{
    spawn,
    sync::{mpsc, RwLock},
    task::yield_now,
    time::sleep,
};
use tracing::{debug, error, info, instrument, Level};

use crate::{
    dimension::{Dimension, NamespacedDimension, NodeMetric},
    metric::{MetricDuration, MetricSpan, MetricSpanKind},
    plan::Plan,
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
    pub(crate) dimension: Arc<RwLock<Dimension>>,
    pub(crate) kube: Arc<Client>,
    plan_tx: Arc<mpsc::Sender<Box<dyn Plan>>>,
    storage: GlobalStorageContext,
}

/// Init
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_default() -> Result<OptimizerContext> {
        info!("creating optimizer context");
        let (plan_tx, plan_rx) = mpsc::channel(1024);
        let kube = Client::try_default().await?;
        let namespace = kube.default_namespace().into();

        let flush = Some(Duration::from_secs(10));
        let ctx = Self {
            dimension: Arc::default(),
            kube: Arc::new(kube),
            plan_tx: Arc::new(plan_tx),
            storage: GlobalStorageContext::new(StorageS3Args::try_parse()?, flush, namespace),
        };

        spawn({
            let ctx = ctx.clone();
            async move { ctx.loop_forever_plan(plan_rx).await }
        });

        Ok(ctx)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn loop_forever_plan(self, mut rx: mpsc::Receiver<Box<dyn Plan>>) {
        while let Some(plan) = rx.recv().await {
            // yield per every loop
            yield_now().await;

            match plan.exec(&self).await {
                Ok(()) => continue,
                Err(error) => {
                    error!("failed to spawn plan: {error}");
                }
            }

            loop {
                match self
                    .get_metric_with_last(
                        MetricSpanKind::MetadataStorage {
                            type_: dash_pipe_provider::storage::MetadataStorageType::LakeHouse,
                        },
                        Duration::from_secs(2 * 10),
                    )
                    .await
                {
                    Ok(data) => {
                        dbg!(data);
                    }
                    Err(error) => {
                        error!("{error}")
                    }
                }
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

/// Dimension
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

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub async fn exists_storage(&self, namespace: &str, name: &str) -> bool {
        let storage_session = self.dimension.read().await;
        match storage_session.get(namespace) {
            Some(storage) => {
                drop(storage_session);
                storage.read().await.exists(name)
            }
            None => false,
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub async fn solve_next_model_storage_binding(
        &self,
        namespace: &str,
        name: &str,
        policy: ModelClaimBindingPolicy,
    ) -> Option<String> {
        let storage_session = self.dimension.read().await;
        match storage_session.get(namespace) {
            Some(storage) => {
                drop(storage_session);
                storage
                    .read()
                    .await
                    .solve_next_model_storage_binding(name, policy)
            }
            None => None,
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub async fn solve_next_storage(
        &self,
        namespace: &str,
        name: &str,
        policy: ModelClaimBindingPolicy,
        timeout: Option<Duration>,
    ) -> Option<Arc<ModelStorageCrd>> {
        if !self.exists_storage(namespace, name).await {
            return None;
        }

        match self.wait_storage(namespace, name, timeout).await {
            Some(storage) => storage.read().await.solve_next_storage(name, policy),
            None => None,
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub async fn wait_storage(
        &self,
        namespace: &str,
        name: &str,
        timeout: Option<Duration>,
    ) -> Option<Arc<RwLock<NamespacedDimension>>> {
        const INTERVAL: Duration = Duration::from_millis(100);

        let mut elapsed = Duration::default();
        let is_timeout = || async move {
            elapsed += INTERVAL;
            sleep(INTERVAL).await;

            matches!(timeout, Some(timeout) if timeout <= elapsed)
        };

        let storage = loop {
            let storage_session = self.dimension.read().await;
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

        let columns = "sum(end_ns - begin_ns) as elapsed_ns, sum(len) as payloads, count(1) as len";
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
            payloads: get_value("payloads")?,
            len: get_value("len")?,
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
