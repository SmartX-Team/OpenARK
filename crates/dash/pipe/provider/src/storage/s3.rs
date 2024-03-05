use anyhow::{anyhow, bail, Error, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use dash_pipe_api::storage::StorageS3Args;
use futures::TryFutureExt;
use minio::s3::{
    args::{GetObjectArgs, PutObjectApiArgs, RemoveObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use tracing::{debug, instrument, Level, Span};

#[derive(Clone)]
pub struct Storage {
    base_url: BaseUrl,
    model: Option<Name>,
    name: String,
    pipe_name: Name,
    pipe_timestamp: String,
    provider: StaticProvider,
}

impl Storage {
    const STORAGE_TYPE: super::StorageType = super::StorageType::S3;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub fn try_new(
        StorageS3Args {
            access_key,
            region: _,
            s3_endpoint,
            secret_key,
        }: &StorageS3Args,
        name: String,
        model: Option<&Name>,
        pipe_name: &Name,
    ) -> Result<Self> {
        debug!("Initializing Storage Set ({model:?}) - S3");
        Ok(Self {
            base_url: BaseUrl::from_string(s3_endpoint.as_str().into())
                .map_err(|error| anyhow!("failed to parse s3 storage endpoint: {error}"))?,
            model: model.cloned(),
            name,
            pipe_name: pipe_name.clone(),
            pipe_timestamp: Utc::now()
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                .replace(':', "-"),
            provider: StaticProvider::new(access_key, secret_key, None),
        })
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn model(&self) -> Option<&Name> {
        self.model.as_ref()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn storage_type(&self) -> super::StorageType {
        Self::STORAGE_TYPE
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len,
            data.model = %model.as_str(),
            storage.name = %self.name,
            storage.r#type = %Self::STORAGE_TYPE,
        ),
        err(Display),
    )]
    async fn get(&self, model: &Name, path: &str) -> Result<Bytes> {
        let bucket_name = model.storage();
        let args = GetObjectArgs::new(bucket_name, path)?;

        // Record the result as part of the current span.
        let span = Span::current();
        let record_data_len = |bytes: Option<&Bytes>| {
            span.record(
                "data.len",
                bytes.map(|bytes| bytes.len()).unwrap_or_default(),
            );
        };

        Client::new(self.base_url.clone(), Some(&self.provider))
            .get_object(&args)
            .map_err(Error::from)
            .and_then(|object| async move {
                match object.bytes().await {
                    Ok(bytes) => {
                        record_data_len(Some(&bytes));
                        Ok(bytes)
                    }
                    Err(error) => {
                        record_data_len(None);
                        bail!("failed to get object data from S3 object store: {error}")
                    }
                }
            })
            .await
            .map_err(|error| anyhow!("failed to get object from S3 object store: {error}"))
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %bytes.len(),
            data.model = %model.as_str(),
            storage.name = %self.name,
            storage.r#type = %Self::STORAGE_TYPE,
        ),
        err(Display),
    )]
    async fn put_with_model(&self, model: &Name, path: &str, bytes: Bytes) -> Result<String> {
        let bucket_name = model.storage();
        let path = format!(
            "{kind}/{prefix}/{timestamp}/{path}",
            kind = super::name::KIND_STORAGE,
            prefix = &self.pipe_name,
            timestamp = &self.pipe_timestamp,
        );
        let args = PutObjectApiArgs::new(bucket_name, &path, &bytes)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .put_object_api(&args)
            .await
            .map(|_| path)
            .map_err(|error| anyhow!("failed to put object into S3 object store: {error}"))
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %model.as_str(),
            storage.name = %self.name,
            storage.r#type = %Self::STORAGE_TYPE,
        ),
        err(Display),
    )]
    async fn delete_with_model(&self, model: &Name, path: &str) -> Result<()> {
        let bucket_name = model.storage();
        let args = RemoveObjectArgs::new(bucket_name, path)?;

        Client::new(self.base_url.clone(), Some(&self.provider))
            .remove_object(&args)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to delete object from S3 object store: {error}"))
    }
}
