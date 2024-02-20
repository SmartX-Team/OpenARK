use anyhow::{bail, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct Storage {
    model: Option<Name>,
}

impl Storage {
    pub fn new(model: Option<&Name>) -> Self {
        Self {
            model: model.cloned(),
        }
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn model(&self) -> Option<&Name> {
        self.model.as_ref()
    }

    fn storage_type(&self) -> super::StorageType {
        super::StorageType::Passthrough
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %_model.as_str(),
        ),
        err(Display),
    )]
    async fn get(&self, _model: &Name, _path: &str) -> Result<Bytes> {
        bail!("Passthrough storage does not support GET operation.")
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %_bytes.len(),
            data.model = %_model.as_str(),
        ),
        err(Display),
    )]
    async fn put_with_model(&self, _model: &Name, _path: &str, _bytes: Bytes) -> Result<String> {
        bail!("Passthrough storage does not support PUT operation.")
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %_model.as_str(),
        ),
        err(Display),
    )]
    async fn delete_with_model(&self, _model: &Name, _path: &str) -> Result<()> {
        bail!("Passthrough storage does not support DELETE operation.")
    }
}
