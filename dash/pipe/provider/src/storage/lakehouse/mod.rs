pub mod decoder;
pub mod schema;

use std::{
    collections::{BTreeMap, HashMap},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use dash_pipe_api::storage::StorageS3Args;
use deltalake::{
    arrow::json::reader::infer_json_schema_from_seekable,
    datafusion::{dataframe::DataFrame, execution::context::SessionContext},
    kernel::StructField,
    operations::create::CreateBuilder,
    protocol::SaveMode,
    writer::{DeltaWriter, JsonWriter},
    DeltaTable, DeltaTableBuilder, DeltaTableConfig, DeltaTableError,
};
use inflector::Inflector;
use schemars::{schema::RootSchema, JsonSchema};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tokio::{
    sync::{Mutex, RwLock},
    time::sleep,
};
use tracing::{debug, instrument, Level};

use crate::message::{DynValue, PipeMessage};

use self::{decoder::TryIntoTableDecoder, schema::FieldColumns};

#[async_trait]
pub trait StorageSessionContext {
    type Table;

    async fn register_table_with_name(
        &self,
        args: &StorageS3Args,
        model: &str,
        fields: Option<RootSchema>,
    ) -> Result<(
        String,
        <Self as StorageSessionContext>::Table,
        StorageTableState,
    )>;
}

#[async_trait]
impl StorageSessionContext for SessionContext {
    type Table = Arc<DeltaTable>;

    async fn register_table_with_name(
        &self,
        args: &StorageS3Args,
        model: &str,
        fields: Option<RootSchema>,
    ) -> Result<(
        String,
        <Self as StorageSessionContext>::Table,
        StorageTableState,
    )> {
        let (model, table, state) = load_table(args, model, fields).await?;
        let table = Arc::new(table);

        self.register_table(&model, table.clone())?;
        Ok((model, table, state))
    }
}

#[derive(Clone)]
pub struct GlobalStorageContext {
    args: StorageS3Args,
    flush: Option<Duration>,
    namespace: String,
    lock_table: Arc<AtomicBool>,
    storages: Arc<RwLock<BTreeMap<String, StorageContext>>>,
}

impl GlobalStorageContext {
    pub fn new(args: StorageS3Args, flush: Option<Duration>, namespace: String) -> Self {
        Self {
            args,
            flush,
            namespace,
            lock_table: Arc::default(),
            storages: Arc::default(),
        }
    }
}

impl GlobalStorageContext {
    pub async fn get_table(&self, model: &str) -> Result<StorageContext> {
        loop {
            {
                let storages = self.storages.read().await;
                if let Some(ctx) = storages.get(model).cloned() {
                    break Ok(ctx);
                }
            }

            // wait for other table operations to be finished
            if self.lock_table.swap(true, Ordering::SeqCst) {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            let release = || self.lock_table.store(false, Ordering::SeqCst);

            // load or create a table
            match StorageContext::try_new::<DynValue>(
                &self.args,
                self.namespace.clone(),
                model,
                self.flush,
            )
            .await
            {
                Ok(ctx) => {
                    self.storages
                        .write()
                        .await
                        .insert(model.into(), ctx.clone());

                    release();
                    break Ok(ctx);
                }
                Err(error) => {
                    release();
                    break Err(error);
                }
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct Storage {
    inner: Option<StorageContext>,
}

impl Storage {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_new<Value>(
        args: &StorageS3Args,
        namespace: String,
        model: Option<&Name>,
        flush: Option<Duration>,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        Ok(Self {
            inner: match model {
                Some(model) => Some(
                    StorageContext::try_new::<Value>(args, namespace, model.storage(), flush)
                        .await?,
                ),
                None => None,
            },
        })
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for Storage {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        match self.inner.as_ref() {
            Some(inner) => {
                <StorageContext as super::MetadataStorage<Value>>::list_metadata(inner).await
            }
            None => bail!("cannot init dataframe from uninited DeltaLake table"),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Serialize + JsonSchema,
    {
        match self.inner.as_ref() {
            Some(inner) => {
                <StorageContext as super::MetadataStorage<Value>>::put_metadata(inner, values).await
            }
            None => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn flush(&self) -> Result<()> {
        match self.inner.as_ref() {
            Some(inner) => <StorageContext as super::MetadataStorage<Value>>::flush(inner).await,
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
pub struct StorageContext {
    model: String,
    namespace: String,
    table: Arc<RwLock<DeltaTable>>,
    writer: Arc<Mutex<StorageTableWriter>>,
}

impl StorageContext {
    #[instrument(level = Level::INFO, skip(args), err(Display))]
    pub async fn try_new<Value>(
        args: &StorageS3Args,
        namespace: String,
        model: &str,
        flush: Option<Duration>,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        // parse fields
        let fields = if ::schemars::schema_for!(Value) == ::schemars::schema_for!(DynValue) {
            // do not infer types with dynamic types
            None
        } else {
            Some(::schemars::schema_for!(PipeMessage<Value, ()>))
        };

        // get or create a table
        let (model, table, state) = load_table(args, model, fields).await?;

        let writer = match state {
            StorageTableState::Inited => Some(init_writer(&table)?),
            StorageTableState::Uninited => None,
        };

        let table = Arc::new(RwLock::new(table));
        let writer = StorageTableWriter::new(table.clone(), writer, flush);

        Ok(Self {
            model,
            namespace,
            table,
            writer,
        })
    }

    #[must_use]
    pub async fn is_ready(&self) -> bool {
        let table = self.table.read().await;
        table.get_state().schema().is_some()
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    #[must_use]
    pub async fn sql(&self, sql: &str) -> Result<DataFrame> {
        let session = self.get_session().await?;
        session.sql(sql).await.map_err(|error| {
            anyhow!("failed to query object metadata from DeltaLake object store: {error}")
        })
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    #[must_use]
    async fn get_session(&self) -> Result<SessionContext> {
        self.try_get_session()
            .await
            .and_then(|option| option.ok_or_else(|| anyhow!("no metadata")))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    #[must_use]
    async fn try_get_session(&self) -> Result<Option<SessionContext>> {
        let old_table = self.table.read().await;
        if old_table.get_state().schema().is_none() {
            return Ok(None);
        }

        let mut table = DeltaTable::new(
            old_table.log_store(),
            DeltaTableConfig {
                require_tombstones: old_table.config.require_tombstones,
                require_files: old_table.config.require_files,
                log_buffer_size: old_table.config.log_buffer_size,
            },
        );
        table.state = old_table.state.clone();
        drop(old_table);

        let session = SessionContext::default();
        session.register_table(&self.model, Arc::new(table))?;
        Ok(Some(session))
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for StorageContext {
    #[instrument(level = Level::INFO, skip_all, fields(data.namespace = %self.namespace), err(Display))]
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value, ()>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        let session = self.get_session().await?;
        let df = session.table(&self.model).await.map_err(|error| {
            anyhow!("failed to get object metadata list from DeltaLake object store: {error}")
        })?;

        df.try_into_decoder().await.map_err(|error| {
            anyhow!("failed to get object metadata from DeltaLake object store: {error}")
        })
    }

    #[instrument(level = Level::INFO, skip_all, fields(data.len = %values.len(), data.namespace = %self.namespace), err(Display))]
    async fn put_metadata(&self, values: &[&PipeMessage<Value, ()>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Serialize + JsonSchema,
    {
        self.writer
            .lock()
            .await
            .write(values.iter().map(|value| json!(value)).collect())
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn flush(&self) -> Result<()> {
        self.writer.lock().await.flush().await
    }
}

struct StorageTableWriter {
    dirty: bool,
    flush: Option<Duration>,
    inner: Option<JsonWriter>,
    last_flushed: Instant,
    table: Arc<RwLock<DeltaTable>>,
}

impl StorageTableWriter {
    fn new(
        table: Arc<RwLock<DeltaTable>>,
        inner: Option<JsonWriter>,
        flush: Option<Duration>,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            dirty: false,
            flush,
            inner,
            last_flushed: Instant::now(),
            table,
        }))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get(&mut self, values: &[DynValue]) -> Result<&mut JsonWriter> {
        // dynamic table schema inferring
        if self.inner.is_none() {
            let reader = Cursor::new(
                values
                    .iter()
                    .filter_map(|value| ::serde_json::to_vec(value).ok())
                    .flatten()
                    .collect::<Vec<_>>(),
            );
            let schema = infer_json_schema_from_seekable(reader, None)
                .map_err(|error| anyhow!("failed to infer object metadata schema: {error}"))?;
            let columns = schema.to_data_columns().map_err(|error| {
                anyhow!("failed to convert inferred object metadata schema into parquet: {error}")
            })?;

            // assert ACID by acquiring WRITE access for table
            let mut table = self.table.write().await;
            *table = create_table(&table, columns).await?;
            self.inner = Some(init_writer(&table)?);
        }

        self.inner
            .as_mut()
            .ok_or_else(|| unreachable!("empty schema"))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn write(&mut self, values: Vec<DynValue>) -> Result<()> {
        self.dirty = true;
        match self.get(&values).await {
            Ok(writer) => {
                writer.write(values).await.map_err(|error| {
                    anyhow!("failed to put metadata into DeltaLake table: {error}")
                })?;

                if let Some(interval) = self.flush {
                    if self.last_flushed.elapsed() >= interval {
                        self.flush().await?;
                    }
                }
                Ok(())
            }
            Err(error) => bail!("failed to init DeltaLake writer: {error}"),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn flush(&mut self) -> Result<()> {
        if self.flush.is_some() && self.dirty {
            self.dirty = false;
            self.last_flushed = Instant::now();

            match self.inner.as_mut() {
                Some(writer) => {
                    // assert ACID by acquiring WRITE access for table
                    let mut table = self.table.write().await;
                    writer.flush_and_commit(&mut table)
                        .await
                        .map(|_| debug!("commited object metadata"))
                        .map_err(|error| {
                            anyhow!(
                                "failed to flush object metadata into DeltaLake object store: {error}"
                            )
                        })
                }
                None => Ok(()),
            }
        } else {
            Ok(())
        }
    }
}

#[instrument(level = Level::INFO, skip_all, err(Display))]
async fn load_table(
    StorageS3Args {
        access_key,
        s3_endpoint,
        region,
        secret_key,
    }: &StorageS3Args,
    model: &str,
    fields: Option<RootSchema>,
) -> Result<(String, DeltaTable, StorageTableState)> {
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

    let mut table = DeltaTableBuilder::from_uri(table_uri)
        .with_allow_http(allow_http)
        .with_storage_options(backend_config)
        .build()
        .map_err(|error| anyhow!("failed to init DeltaLake table: {error}"))?;

    let model = model.split('/').last().unwrap().to_snake_case();

    // get or create a table
    match table.load().await {
        Ok(()) => {
            debug!("DeltaLake table schema: loaded");
            Ok((model, table, StorageTableState::Inited))
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
                Ok((model, table, StorageTableState::Uninited))
            } else {
                debug!("DeltaLake table schema: creating statically");
                let table = create_table(&table, columns).await?;
                Ok((model, table, StorageTableState::Inited))
            }
        }
        Err(error) => {
            bail!("failed to load metadata table from DeltaLake object store: {error}")
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageTableState {
    Inited,
    Uninited,
}

#[instrument(level = Level::INFO, skip_all, err(Display))]
async fn create_table(
    table: &DeltaTable,
    columns: impl IntoIterator<Item = impl Into<StructField>>,
) -> Result<DeltaTable> {
    CreateBuilder::default()
        .with_log_store(table.log_store())
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
