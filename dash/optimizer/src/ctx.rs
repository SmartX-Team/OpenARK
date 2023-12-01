use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dash_api::{model_claim::ModelClaimBindingPolicy, storage::ModelStorageCrd};
use kube::Client;
use tokio::{
    spawn,
    sync::{mpsc, RwLock},
    time::sleep,
};
use tracing::{error, info, instrument, Level};

use crate::{
    plan::Plan,
    storage::{NamespacedStorageDimension, StorageDimension},
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
    pub(crate) storage: Arc<RwLock<StorageDimension>>,
}

/// Init
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_default() -> Result<OptimizerContext> {
        info!("creating optimizer context");
        let (plan_tx, plan_rx) = mpsc::channel(1024);
        let ctx = Self {
            kube: Arc::new(Client::try_default().await?),
            plan_tx: Arc::new(plan_tx),
            storage: Arc::default(),
        };

        spawn({
            let ctx = ctx.clone();
            async move { ctx.loop_forever_plan(plan_rx).await }
        });

        Ok(ctx)
    }
}

/// Storage
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
        let storage_session = self.storage.read().await;
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
        let storage_session = self.storage.read().await;
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
    ) -> Option<Arc<RwLock<NamespacedStorageDimension>>> {
        const INTERVAL: Duration = Duration::from_millis(100);

        let mut elapsed = Duration::default();
        let is_timeout = || async move {
            elapsed += INTERVAL;
            sleep(INTERVAL).await;

            match timeout {
                Some(timeout) if timeout <= elapsed => true,
                _ => false,
            }
        };

        let storage = loop {
            let storage_session = self.storage.read().await;
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

/// Standalone runners
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all)]
    async fn loop_forever_plan(self, mut rx: mpsc::Receiver<Box<dyn Plan>>) {
        while let Some(plan) = rx.recv().await {
            match plan.exec(&self).await {
                Ok(()) => continue,
                Err(error) => {
                    error!("failed to spawn plan: {error}");
                }
            }
        }
    }
}
