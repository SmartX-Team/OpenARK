use std::future::Future;

use anyhow::Result;
use dash_provider::client::job::TaskActorJobClient;
use dash_provider_api::{job::TaskActorJobMetadata, SessionContext, SessionContextMetadata};
use kube::Client;
use tracing::{instrument, Level};

#[derive(Copy, Clone)]
pub struct InjectorValidator<'namespace, 'kube> {
    pub content: &'static str,
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> InjectorValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn create(&self) -> Result<()> {
        self.execute(|client, input| async move { client.create(&input).await })
            .await
            .map(|_| ())
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self) -> Result<()> {
        self.execute(|client, input| async move { client.delete(&input).await })
            .await
            .map(|_| ())
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn exists(&self) -> Result<bool> {
        self.execute(|client, input| async move { client.exists(&input).await })
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn execute<F, Fut, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(TaskActorJobClient, SessionContext<()>) -> Fut,
        Fut: Future<Output = Result<R>>,
    {
        let name = "dash-observability".to_string();
        let namespace = self.namespace.to_string();
        let metadata = TaskActorJobMetadata::default();

        let client = TaskActorJobClient::from_raw_content(
            self.kube.clone(),
            metadata,
            namespace.clone(),
            &name,
            self.content,
            true,
        )?;

        let input = SessionContext {
            metadata: SessionContextMetadata { name, namespace },
            spec: (),
        };
        f(client, input).await
    }
}
