mod decoder;
mod schema;

use std::{collections::HashMap, io::Cursor, sync::Arc};

use anyhow::{anyhow, bail, Result};
use arrow_json::reader::infer_json_schema_from_seekable;
use async_recursion::async_recursion;
use async_trait::async_trait;
use datafusion::prelude::SessionContext;
use deltalake::{
    protocol::SaveMode,
    writer::{DeltaWriter, JsonWriter},
    DeltaOps, DeltaTable, DeltaTableBuilder, DeltaTableConfig, DeltaTableError, SchemaField,
};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::debug;

use crate::message::{Name, PipeMessage};

use self::{decoder::TryIntoTableDecoder, schema::FieldColumns};

#[derive(Clone)]
pub struct Storage {
    session: Arc<Mutex<StorageSession>>,
}

impl Storage {
    pub async fn try_new<Value>(args: &super::StorageS3Args, model: Option<&Name>) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        debug!("Initializing Storage Set ({model:?}) - DeltaLake");
        StorageSession::try_new::<Value>(args, model)
            .await
            .map(|session| Self {
                session: Arc::new(Mutex::new(session)),
            })
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for Storage {
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        self.session.lock().await.list_metadata().await
    }

    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema,
    {
        if values.is_empty() {
            return Ok(());
        }

        self.session.lock().await.put_metadata(values).await
    }

    async fn flush(&self) -> Result<()> {
        self.session.lock().await.flush().await
    }
}

struct StorageSession {
    table: Option<Arc<DeltaTable>>,
    writer: Option<JsonWriter>,
}

unsafe impl Send for StorageSession {}
unsafe impl Sync for StorageSession {}

impl StorageSession {
    async fn try_new<Value>(
        super::StorageS3Args {
            access_key,
            s3_endpoint,
            region,
            secret_key,
        }: &super::StorageS3Args,
        model: Option<&Name>,
    ) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        match model {
            Some(model) => {
                let mut table = {
                    let allow_http = s3_endpoint.scheme() == "http";
                    let table_uri = format!(
                        "s3a://{bucket_name}/{kind}/",
                        bucket_name = model,
                        kind = super::name::KIND_METADATA,
                    );

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
                    Ok(()) => {
                        debug!("DeltaLake table schema: loaded");
                        (table, true)
                    }
                    Err(DeltaTableError::NotATable(_)) => {
                        let columns = ::schemars::schema_for!(PipeMessage<Value, ()>)
                            .to_data_columns()
                            .map_err(|error| {
                                anyhow!("failed to convert metadata columns into parquet: {error}")
                            })?;

                        if columns.is_empty() {
                            debug!("DeltaLake table schema: lazy-inferring dynamically");
                            (table, false)
                        } else {
                            debug!("DeltaLake table schema: creating statically");
                            let table = create_table(table, columns).await?;
                            (table, true)
                        }
                    }
                    Err(error) => {
                        bail!("failed to load metadata table from DeltaLake object store: {error}")
                    }
                };

                let writer = if has_writer {
                    Some(init_writer(&table)?)
                } else {
                    None
                };

                Ok(Self {
                    table: Some(table.into()),
                    writer,
                })
            }
            None => Ok(Self {
                table: None,
                writer: None,
            }),
        }
    }
}

impl StorageSession {
    async fn list_metadata<Value>(&self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        match self.table.as_ref() {
            Some(table) => {
                let ctx = SessionContext::new();
                ctx.register_table("table", table.clone())
                    .map_err(|error| {
                        anyhow!("failed to load DeltaLake metadata sesion: {error}")
                    })?;

                let df = ctx.table("table").await.map_err(|error| {
                    anyhow!(
                        "failed to get object metadata list from DeltaLake object store: {error}"
                    )
                })?;

                df.try_into_decoder().await.map_err(|error| {
                    anyhow!("failed to get object metadata from DeltaLake object store: {error}")
                })
            }
            None => bail!("cannot list object metadata from uninited DeltaLake table"),
        }
    }

    #[async_recursion]
    async fn put_metadata<Value>(&mut self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: Send + Sync + Default + Serialize + JsonSchema,
    {
        const MAX_READ_RECORDS: usize = 1_000;

        match self.writer.as_mut() {
            Some(writer) => writer
                .write(values.iter().map(|value| json!(value)).collect())
                .await
                .map_err(|error| {
                    anyhow!("failed to put object metadata into DeltaLake object store: {error}")
                }),
            // dynamic table schema inferring
            None => match self.table.as_mut() {
                Some(table) => {
                    let reader = Cursor::new(
                        values
                            .iter()
                            .filter_map(|value| ::serde_json::to_vec(value).ok())
                            .flatten()
                            .collect::<Vec<_>>(),
                    );
                    let schema = infer_json_schema_from_seekable(reader, Some(MAX_READ_RECORDS))
                        .map_err(|error| {
                            anyhow!("failed to infer object metadata schema: {error}")
                        })?;
                    let columns = schema.to_data_columns().map_err(|error| {
                        anyhow!("failed to convert inferred object metadata schema into parquet: {error}")
                    })?;

                    *table = Arc::new(create_table(clone_table(table), columns).await?);
                    self.writer = Some(init_writer(table)?);

                    self.put_metadata(values).await
                }
                None => Ok(()),
            },
        }
    }

    async fn flush(&mut self) -> Result<()> {
        match self.table.as_ref().zip(self.writer.as_mut()) {
            Some((table, writer)) => {
                let mut table =
                    DeltaTable::new(table.object_store(), DeltaTableConfig { ..table.config });
                table.load().await.map_err(|error| {
                    anyhow!("failed to reload metadata table from DeltaLake object store: {error}")
                })?;

                writer
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

fn clone_table(table: &DeltaTable) -> DeltaTable {
    DeltaTable::new(table.object_store(), DeltaTableConfig { ..table.config })
}

async fn create_table(
    table: DeltaTable,
    columns: impl IntoIterator<Item = impl Into<SchemaField>>,
) -> Result<DeltaTable> {
    DeltaOps::from(table)
        .create()
        .with_columns(columns)
        .with_save_mode(SaveMode::Append)
        .await
        .map_err(|error| {
            anyhow!("failed to create a metadata table on DeltaLake object store: {error}")
        })
}

fn init_writer(table: &DeltaTable) -> Result<JsonWriter> {
    JsonWriter::for_table(table)
        .map_err(|error| anyhow!("failed to init json writer from DeltaLake object store: {error}"))
}
