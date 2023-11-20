use anyhow::Result;
use async_trait::async_trait;
use dash_api::storage::kubernetes::ModelStorageKubernetesSpec;
use dash_provider_api::data::Capacity;
use kube::Client;
use tracing::{instrument, warn, Level};

#[async_trait]
impl super::GetCapacity for ModelStorageKubernetesSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        _kube: &'kube Client,
        _namespace: &'namespace str,
        _storage_name: String,
    ) -> Result<Option<Capacity>> {
        warn!("unsupported storage type for fallback optimizer: Kubernetes");
        Ok(None)
    }
}
