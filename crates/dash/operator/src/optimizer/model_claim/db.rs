use anyhow::Result;
use async_trait::async_trait;
use dash_api::storage::db::ModelStorageDatabaseSpec;
use dash_provider_api::data::Capacity;
use kube::Client;
use tracing::warn;

#[async_trait]
impl super::GetCapacity for ModelStorageDatabaseSpec {
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        _kube: &'kube Client,
        _namespace: &'namespace str,
        _storage_name: &str,
    ) -> Result<Option<Capacity>> {
        warn!("unsupported storage type for fallback optimizer: Database");
        Ok(None)
    }
}

#[async_trait]
impl super::GetTraffic for ModelStorageDatabaseSpec {}
