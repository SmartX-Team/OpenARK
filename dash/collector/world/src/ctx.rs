use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use dash_collector_api::metrics::{MetricDuration, MetricRow};
use dash_pipe_api::storage::StorageS3Args;
use dash_pipe_provider::{
    deltalake::{arrow::json::ArrayWriter, datafusion::dataframe::DataFrame},
    storage::lakehouse::{GlobalStorageContext, StorageContext},
    Name,
};
use inflector::Inflector;
use kube::Client;
use tokio::{
    sync::{mpsc, RwLock},
    time::sleep,
};
use tracing::{debug, info, instrument, Level};

use crate::{
    data::{Namespace, World},
    plan::Plan,
};

#[derive(Clone)]
pub struct WorldContext {
    pub(crate) data: Arc<RwLock<World>>,
    pub(crate) kube: Arc<Client>,
    model: Name,
    plan_tx: Arc<mpsc::Sender<Box<dyn Plan>>>,
    storage: GlobalStorageContext,
    storage_model: String,
}

/// Init
impl WorldContext {
    pub(crate) const INTERVAL_FLUSH: Duration = Duration::from_secs(10);

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_new(model: Name) -> Result<(Self, mpsc::Receiver<Box<dyn Plan>>)> {
        info!("creating world context");
        let (plan_tx, plan_rx) = mpsc::channel(1024);
        let kube = Client::try_default().await?;
        let storage_model = model.to_snake_case();

        let args = StorageS3Args::try_parse()?;
        let flush = Some(Self::INTERVAL_FLUSH);
        let ctx = Self {
            data: Arc::default(),
            kube: Arc::new(kube),
            model,
            plan_tx: Arc::new(plan_tx),
            storage: GlobalStorageContext::new(args, flush),
            storage_model,
        };

        Ok((ctx, plan_rx))
    }
}

/// Plans
impl WorldContext {
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
}

/// Storage
impl WorldContext {
    async fn get_table(&self, name: &str) -> Result<StorageContext> {
        self.storage.get_table(name).await
    }

    async fn get_all_metrics(
        &self,
        duration: Option<MetricDuration>,
    ) -> Result<Vec<MetricRow<'static>>> {
        let condition = match duration {
            Some(MetricDuration { begin_ns, end_ns }) => {
                format!("WHERE begin_ns >= {begin_ns} AND end_ns < {end_ns}")
            }
            None => String::default(),
        };

        let group = "name, namespace, kind, type, op, model";
        let columns = format!("{group}, sum(end_ns - begin_ns) as elapsed_ns, sum(len) as total_bytes, count(1) as len");
        let sql = format!(
            "SELECT {columns} FROM {model} {condition} GROUP BY {group}",
            model = &self.storage_model,
        );
        debug!("SQL = {sql}");

        let df = self.query_metric(&sql).await?;
        let batches = df.collect().await?;

        let mut writer = ArrayWriter::new(vec![]);
        for batch in batches {
            writer.write(&batch)?;
        }
        writer.finish()?;

        let data = writer.into_inner();
        if !data.is_empty() {
            ::serde_json::from_reader(&*data)
                .map_err(|error| anyhow!("failed to convert data into metrics: {error}"))
        } else {
            Ok(vec![])
        }
    }

    async fn get_all_metrics_with_last(
        &self,
        duration: Duration,
    ) -> Result<Vec<MetricRow<'static>>> {
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

        self.get_all_metrics(Some(duration)).await
    }

    async fn query_metric(&self, sql: &str) -> Result<DataFrame> {
        let table = self.get_table(&self.model).await?;
        let df = table.sql(sql).await?;
        Ok(df)
    }

    pub async fn update_metrics(&self, duration: Duration) -> Result<()> {
        let table = self.get_table(&self.model).await?;
        table.update().await?;

        let metrics = self.get_all_metrics_with_last(duration).await?;
        self.data.write().await.update_metrics(metrics).await;
        Ok(())
    }
}

/// World
impl WorldContext {
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

        let namespace = loop {
            let world_session = self.data.read().await;
            match world_session.get(namespace) {
                Some(namespace) => break namespace,
                None => {
                    drop(world_session);
                    if is_timeout().await {
                        return None;
                    }
                }
            }
        };

        loop {
            let namespace_session = namespace.read().await;
            if namespace_session.is_ready(name) {
                drop(namespace_session);
                break Some(namespace);
            } else {
                drop(namespace_session);
                if is_timeout().await {
                    return None;
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    async fn exists(&self, namespace: &str, name: &str) -> bool {
        let world_session = self.data.read().await;
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
