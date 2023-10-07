mod decoder;
mod schema;

use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use datafusion::prelude::SessionContext;
use deltalake::{
    protocol::SaveMode,
    writer::{DeltaWriter, JsonWriter},
    DeltaOps, DeltaTable, DeltaTableBuilder, DeltaTableConfig, DeltaTableError, ObjectStore, Path,
};
use futures::TryFutureExt;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::message::PipeMessage;

use self::{decoder::TryIntoTableDecoder, schema::FieldColumns};

#[derive(Clone)]
pub struct Storage {
    table: Arc<DeltaTable>,
    writer: Option<Arc<Mutex<JsonWriter>>>,
}

impl Storage {
    pub async fn try_new<Value>(
        super::StorageS3Args {
            access_key,
            s3_endpoint,
            region,
            secret_key,
        }: &super::StorageS3Args,
        bucket_name: &str,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        let mut table = {
            let allow_http = s3_endpoint.scheme() == "http";
            let table_uri = format!("s3a://{bucket_name}/");

            let mut backend_config: HashMap<String, String> = HashMap::new();
            backend_config.insert("AWS_ACCESS_KEY_ID".to_string(), access_key.clone());
            backend_config.insert("AWS_ENDPOINT_URL".to_string(), {
                let mut endpoint = s3_endpoint.to_string();
                if endpoint.ends_with('/') {
                    endpoint.pop();
                }
                endpoint
            });
            backend_config.insert("AWS_REGION".to_string(), region.clone());
            backend_config.insert("AWS_SECRET_ACCESS_KEY".to_string(), secret_key.clone());
            backend_config.insert("AWS_S3_ALLOW_UNSAFE_RENAME".to_string(), "true".into());

            DeltaTableBuilder::from_uri(table_uri)
                .with_allow_http(allow_http)
                .with_storage_options(backend_config)
                .build()
                .map_err(|error| anyhow!("failed to init DeltaLake table: {error}"))?
        };

        // get or create a table
        let (table, has_writer) = match table.load().await {
            Ok(()) => (table, true),
            Err(DeltaTableError::NotATable(_)) => {
                let columns = ::schemars::schema_for!(PipeMessage<Value, ()>)
                    .to_data_types()
                    .map_err(|error| {
                        anyhow!("failed to convert metadata columns into parquet: {error}")
                    })?;

                if columns.is_empty() {
                    (table, false)
                } else {
                    let table = DeltaOps::from(table)
                        .create()
                        .with_columns(columns)
                        .with_save_mode(SaveMode::Append)
                        .await
                        .map_err(|error| {
                            anyhow!(
                        "failed to create a metadata table on DeltaLake object store: {error}"
                    )
                        })?;
                    (table, true)
                }
            }
            Err(error) => {
                bail!("failed to load metadata table from DeltaLake object store: {error}")
            }
        };

        let writer = if has_writer {
            Some(Arc::new(Mutex::new(
                JsonWriter::for_table(&table).map_err(|error| {
                    anyhow!("failed to init json writer from DeltaLake object store: {error}")
                })?,
            )))
        } else {
            None
        };

        Ok(Self {
            writer,
            table: table.into(),
        })
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for Storage {
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        let ctx = SessionContext::new();
        ctx.register_table("table", self.table.clone())
            .map_err(|error| anyhow!("failed to load DeltaLake metadata sesion: {error}"))?;

        let df = ctx.table("table").await.map_err(|error| {
            anyhow!("failed to get object metadata list from DeltaLake object store: {error}")
        })?;

        df.try_into_decoder().await.map_err(|error| {
            anyhow!("failed to get object metadata from DeltaLake object store: {error}")
        })
    }

    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Serialize + JsonSchema,
    {
        match self.writer.as_ref() {
            Some(writer) => writer
                .lock()
                .await
                .write(values.iter().map(|value| json!(value)).collect())
                .await
                .map_err(|error| {
                    anyhow!("failed to put object metadata into DeltaLake object store: {error}")
                }),
            None => bail!("cannot put object metadata into empty DeltaLake table"),
        }
    }

    async fn flush(&self) -> Result<()> {
        match self.writer.as_ref() {
            Some(writer) => {
                let mut table = DeltaTable::new(
                    self.table.object_store(),
                    DeltaTableConfig {
                        ..self.table.config
                    },
                );
                table.load().await.map_err(|error| {
                    anyhow!("failed to reload metadata table from DeltaLake object store: {error}")
                })?;

                writer
                    .lock()
                    .await
                    .flush_and_commit(&mut table)
                    .await
                    .map(|_| ())
                    .map_err(|error| {
                        anyhow!(
                            "failed to flush object metadata into DeltaLake object store: {error}"
                        )
                    })
            }
            None => Ok(()),
        }
    }
}

#[async_trait]
impl super::Storage for Storage {
    fn storage_type(&self) -> super::StorageType {
        super::StorageType::LakeHouse
    }

    async fn get(&self, path: &Path) -> Result<Bytes> {
        self.table
            .object_store()
            .get(path)
            .map_err(Error::from)
            .and_then(|object| async move {
                match object.bytes().await {
                    Ok(bytes) => Ok(bytes),
                    Err(error) => {
                        bail!("failed to get object data from DeltaLake object store: {error}")
                    }
                }
            })
            .await
            .map_err(|error| anyhow!("failed to get object from DeltaLake object store: {error}"))
    }

    async fn put(&self, path: &Path, bytes: Bytes) -> Result<()> {
        self.table
            .object_store()
            .put(path, bytes)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to put object into DeltaLake object store: {error}"))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        self.table
            .object_store()
            .delete(path)
            .await
            .map(|_| ())
            .map_err(|error| {
                anyhow!("failed to delete object from DeltaLake object store: {error}")
            })
    }
}
