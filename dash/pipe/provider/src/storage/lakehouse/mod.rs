pub mod decoder;
pub mod schema;

use std::{collections::HashMap, io::Cursor, ops, sync::Arc};

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_recursion::async_recursion;
use async_trait::async_trait;
use dash_pipe_api::storage::StorageS3Args;
use deltalake::{
    arrow::json::reader::infer_json_schema_from_seekable,
    datafusion::prelude::SessionContext,
    protocol::SaveMode,
    writer::{DeltaWriter, JsonWriter},
    DeltaOps, DeltaTable, DeltaTableBuilder, DeltaTableConfig, DeltaTableError, SchemaField,
};
use inflector::Inflector;
use schemars::{schema::RootSchema, JsonSchema};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::message::PipeMessage;

use self::{decoder::TryIntoTableDecoder, schema::FieldColumns};

#[derive(Clone, Default)]
pub struct Storage {
    session: Arc<Mutex<MaybeStorageTable>>,
}

impl Storage {
    pub async fn try_new<Value>(args: &StorageS3Args, model: Option<&Name>) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        debug!("Initializing Storage Set ({model:?}) - DeltaLake");
        MaybeStorageTable::try_new::<Value>(args, model)
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
        super::MetadataStorageMut::list_metadata(&mut *self.session.lock().await).await
    }

    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema,
    {
        if values.is_empty() {
            return Ok(());
        }

        super::MetadataStorageMut::put_metadata(&mut *self.session.lock().await, values).await
    }

    async fn flush(&self) -> Result<()> {
        super::MetadataStorageMut::<Value>::flush(&mut *self.session.lock().await).await
    }
}

#[derive(Default)]
struct MaybeStorageTable {
    inner: Option<StorageTable>,
}

impl MaybeStorageTable {
    async fn try_new<Value>(args: &StorageS3Args, model: Option<&Name>) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        Ok(Self {
            inner: match model {
                Some(model) => Some(StorageTable::try_new::<Value>(args, model).await?),
                None => None,
            },
        })
    }
}

#[async_trait]
impl<Value> super::MetadataStorageMut<Value> for MaybeStorageTable {
    async fn list_metadata(&mut self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        match self.inner.as_mut() {
            Some(inner) => {
                <StorageTable as super::MetadataStorageMut<Value>>::list_metadata(inner).await
            }
            None => bail!("cannot init dataframe from uninited DeltaLake table"),
        }
    }

    async fn put_metadata(&mut self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema,
    {
        match self.inner.as_mut() {
            Some(inner) => {
                <StorageTable as super::MetadataStorageMut<Value>>::put_metadata(inner, values)
                    .await
            }
            None => Ok(()),
        }
    }

    async fn flush(&mut self) -> Result<()> {
        match self.inner.as_mut() {
            Some(inner) => <StorageTable as super::MetadataStorageMut<Value>>::flush(inner).await,
            None => Ok(()),
        }
    }
}

pub struct StorageTable {
    ctx: StorageContext,
    model: String,
    table: Arc<DeltaTable>,
    writer: Option<JsonWriter>,
}

impl StorageTable {
    pub async fn try_new<Value>(args: &StorageS3Args, model: &Name) -> Result<Self>
    where
        Value: Default + JsonSchema,
    {
        let ctx = StorageContext::default();

        // get or create a table
        let (model, table, has_writer) = ctx
            .register_table_with_name(
                args.clone(),
                model.storage(),
                Some(::schemars::schema_for!(PipeMessage<Value, ()>)),
            )
            .await?;

        let writer = if has_writer {
            Some(init_writer(&table)?)
        } else {
            None
        };

        Ok(Self {
            ctx,
            model,
            table,
            writer,
        })
    }
}

#[async_trait]
impl<Value> super::MetadataStorageMut<Value> for StorageTable {
    async fn list_metadata(&mut self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + Default + DeserializeOwned,
    {
        let df = self
            .ctx
            .session
            .table(self.model.as_str())
            .await
            .map_err(|error| {
                anyhow!("failed to get object metadata list from DeltaLake object store: {error}")
            })?;

        df.try_into_decoder().await.map_err(|error| {
            anyhow!("failed to get object metadata from DeltaLake object store: {error}")
        })
    }

    async fn put_metadata(&mut self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Default + Serialize + JsonSchema,
    {
        self.put_metadata_impl(values).await
    }

    async fn flush(&mut self) -> Result<()> {
        match self.writer.as_mut() {
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
                    .flush_and_commit(&mut table)
                    .await
                    .map(|_| info!("commited object metadata"))
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

impl StorageTable {
    #[async_recursion]
    async fn put_metadata_impl<Value>(&mut self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
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
            None => {
                let reader = Cursor::new(
                    values
                        .iter()
                        .filter_map(|value| ::serde_json::to_vec(value).ok())
                        .flatten()
                        .collect::<Vec<_>>(),
                );
                let schema = infer_json_schema_from_seekable(reader, Some(MAX_READ_RECORDS))
                    .map_err(|error| anyhow!("failed to infer object metadata schema: {error}"))?;
                let columns = schema.to_data_columns().map_err(|error| {
                    anyhow!(
                        "failed to convert inferred object metadata schema into parquet: {error}"
                    )
                })?;

                self.table = Arc::new(create_table(clone_table(&self.table), columns).await?);
                self.writer = Some(init_writer(&self.table)?);

                self.put_metadata_impl(values).await
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct StorageContext {
    session: SessionContext,
}

impl ops::Deref for StorageContext {
    type Target = SessionContext;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl ops::DerefMut for StorageContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

impl StorageContext {
    pub async fn register_table_with_name(
        &self,
        StorageS3Args {
            access_key,
            s3_endpoint,
            region,
            secret_key,
        }: StorageS3Args,
        model: &str,
        fields: Option<RootSchema>,
    ) -> Result<(String, Arc<DeltaTable>, bool)> {
        let mut table = {
            let allow_http = s3_endpoint.scheme() == "http";
            let table_uri = format!(
                "s3a://{bucket_name}/{kind}/",
                bucket_name = model,
                kind = super::name::KIND_METADATA,
            );

            let mut backend_config: HashMap<String, String> = HashMap::new();
            backend_config.insert("AWS_ACCESS_KEY_ID".to_string(), access_key);
            backend_config.insert("AWS_ENDPOINT_URL".to_string(), {
                let mut endpoint = s3_endpoint.to_string();
                if endpoint.ends_with('/') {
                    endpoint.pop();
                }
                endpoint
            });
            backend_config.insert("AWS_REGION".to_string(), region);
            backend_config.insert("AWS_SECRET_ACCESS_KEY".to_string(), secret_key);
            backend_config.insert("AWS_S3_ALLOW_UNSAFE_RENAME".to_string(), "true".into());

            DeltaTableBuilder::from_uri(table_uri)
                .with_allow_http(allow_http)
                .with_storage_options(backend_config)
                .build()
                .map_err(|error| anyhow!("failed to init DeltaLake table: {error}"))?
        };

        // get or create a table
        let (table, has_inited) = match table.load().await {
            Ok(()) => {
                debug!("DeltaLake table schema: loaded");
                (table, true)
            }
            Err(DeltaTableError::NotATable(_)) => {
                let columns = fields
                    .map(|fields| {
                        fields.to_data_columns().map_err(|error| {
                            anyhow!("failed to convert metadata columns into parquet: {error}")
                        })
                    })
                    .transpose()?
                    .unwrap_or_default();

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

        let model = model.to_snake_case();
        let table = Arc::new(table);

        if has_inited {
            self.session
                .register_table(&model, table.clone())
                .map_err(|error| anyhow!("failed to load DeltaLake metadata session: {error}"))?;
        }

        Ok((model, table, has_inited))
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
