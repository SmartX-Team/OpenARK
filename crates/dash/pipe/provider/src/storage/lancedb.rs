use std::{env, sync::Arc};

use anyhow::{anyhow, bail, Error, Result};
use ark_core_k8s::data::Name;
use arrow::array::{RecordBatchIterator, RecordBatchReader};
use async_trait::async_trait;
use dash_pipe_api::storage::StorageS3Args;
use lancedb::{
    connection::{Connection, CreateTableBuilder, CreateTableMode},
    table::{AddDataMode, AddDataOptions, WriteOptions},
    Error as LanceError, TableRef,
};
use object_store::aws::AwsCredential;
use schemars::{schema::RootSchema, JsonSchema};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, instrument, Level};

use crate::{
    message::{DynValue, PipeMessage},
    schema::arrow::{decoder::TryIntoTableDecoder, encoder::TryIntoRecordBatch, ToArrowSchema},
};

#[derive(Clone, Default)]
pub struct Storage {
    inner: Option<StorageContext>,
}

impl Storage {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_new<Value>(
        args: &StorageS3Args,
        name: String,
        mode: AddDataMode,
        model: Option<&Name>,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        Ok(Self {
            inner: match model {
                Some(model) => {
                    Some(StorageContext::try_new::<Value>(args, name, mode, model.storage()).await?)
                }
                None => None,
            },
        })
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for Storage {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        match self.inner.as_ref() {
            Some(inner) => {
                <StorageContext as super::MetadataStorage<Value>>::list_metadata(inner).await
            }
            None => bail!("cannot init dataframe from uninited LanceDB table"),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn put_metadata(&self, values: &[&PipeMessage<Value>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Clone + Serialize + JsonSchema,
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
    conn: Connection,
    mode: AddDataMode,
    model_raw: String,
    name: String,
    table: Arc<RwLock<Option<TableRef>>>,
}

impl StorageContext {
    const STORAGE_TYPE: super::MetadataStorageType = super::MetadataStorageType::LanceDB;

    #[instrument(level = Level::INFO, skip(args), err(Display))]
    pub async fn try_new<Value>(
        args: &StorageS3Args,
        name: String,
        mode: AddDataMode,
        model: &str,
    ) -> Result<Self>
    where
        Value: JsonSchema,
    {
        // parse schema
        let schema = if ::schemars::schema_for!(Value) == ::schemars::schema_for!(DynValue) {
            // do not infer types with dynamic types
            None
        } else {
            Some(::schemars::schema_for!(PipeMessage<Value, ()>))
        };

        // get or create a table
        let model_raw = model.to_string();
        let (conn, table) = load_table(args, &model_raw, schema).await?;

        let table = Arc::new(RwLock::new(table));

        Ok(Self {
            conn,
            mode,
            model_raw,
            name,
            table,
        })
    }

    #[must_use]
    pub async fn is_ready(&self) -> bool {
        let table = self.table.read().await;
        table.is_some()
    }

    async fn get_table(&self) -> Result<TableRef> {
        self.table
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("table not inited yet"))
    }
}

#[async_trait]
impl<Value> super::MetadataStorage<Value> for StorageContext {
    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %self.model_raw,
            storage.name = &self.name,
            storage.r#type = %Self::STORAGE_TYPE,
        ),
        err(Display),
    )]
    async fn list_metadata(&self) -> Result<super::Stream<PipeMessage<Value>>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        let table = self.get_table().await?;
        let query = table.query();

        query
            .try_into_decoder()
            .await
            .map_err(|error| anyhow!("failed to get object metadata from LanceDB: {error}"))
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %values.len(),
            data.mode = ?self.mode,
            data.model = %self.model_raw,
            storage.name = &self.name,
            storage.r#type = %Self::STORAGE_TYPE,
        ),
        err(Display),
    )]
    async fn put_metadata(&self, values: &[&PipeMessage<Value>]) -> Result<()>
    where
        Value: 'async_trait + Send + Sync + Clone + Serialize + JsonSchema,
    {
        if values.is_empty() {
            return Ok(());
        }

        let schema = Arc::new(values.to_arrow_schema()?);
        let batch = match values.try_into_record_batch(schema.clone())? {
            Some(batch) => Ok(batch),
            None => return Ok(()),
        };
        let batches = Box::new(RecordBatchIterator::new(vec![batch], schema));

        let table = self.table.read().await;
        match table.as_ref() {
            Some(table) => {
                let options = AddDataOptions {
                    mode: self.mode.clone(),
                    write_options: WriteOptions::default(),
                };

                table
                    .add(batches, options)
                    .await
                    .map_err(|error| anyhow!("failed to put object metadata into LanceDB: {error}"))
            }
            None => {
                drop(table);

                {
                    let mut table = self.table.write().await;
                    *table = Some(
                        create_table(
                            &self.conn,
                            self.model_raw.clone(),
                            CreateTableData::Batches(batches),
                        )
                        .await?,
                    );
                }
                Ok(())
            }
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
    schema: Option<RootSchema>,
) -> Result<(Connection, Option<TableRef>)> {
    let allow_http = s3_endpoint.scheme() == "http";
    let table_uri = format!(
        "s3://{bucket_name}/{kind}/",
        bucket_name = model,
        kind = super::name::KIND_METADATA,
    );

    // currently (lancedb==0.4.11), it uses env to configure endpoint and other configurations
    env::set_var("AWS_ALLOW_HTTP", allow_http.to_string());
    env::set_var("AWS_ENDPOINT", s3_endpoint.as_str());

    let conn = ::lancedb::connect(&table_uri)
        .aws_creds(AwsCredential {
            key_id: access_key.clone(),
            secret_key: secret_key.clone(),
            token: None,
        })
        .region(region)
        .execute()
        .await
        .map_err(|error| anyhow!("failed to connect to LanceDB: {error}"))?;

    // get or create a table
    match conn.open_table(super::name::KIND_METADATA).execute().await {
        Ok(table) => {
            debug!("LanceDB table schema: loaded");
            Ok((conn, Some(table)))
        }
        Err(LanceError::TableNotFound { name }) => match schema {
            Some(schema) => {
                debug!("LanceDB table schema: creating statically");
                let table = create_table(&conn, name, CreateTableData::Schema(schema)).await?;
                Ok((conn, Some(table)))
            }
            None => {
                debug!("LanceDB table schema: lazy-inferring dynamically");
                Ok((conn, None))
            }
        },
        Err(error) => {
            bail!("failed to load metadata table from LanceDB: {error}")
        }
    }
}

#[instrument(level = Level::INFO, skip_all, err(Display))]
async fn create_table(conn: &Connection, name: String, data: CreateTableData) -> Result<TableRef> {
    const MODE: CreateTableMode = CreateTableMode::Overwrite;

    fn error(error: impl ::std::error::Error) -> Error {
        anyhow!("failed to create a metadata table on LanceDB: {error}")
    }

    #[async_trait]
    trait Build {
        async fn build(self) -> Result<TableRef>;
    }

    macro_rules! impl_build {
        ( $has_data:expr ) => {
            #[async_trait]
            impl Build for CreateTableBuilder<$has_data> {
                async fn build(self) -> Result<TableRef> {
                    self.mode(MODE).execute().await.map_err(error)
                }
            }
        };
    }

    impl_build!(false);
    impl_build!(true);

    match data {
        CreateTableData::Batches(batches) => conn.create_table(name, batches).build().await,
        CreateTableData::Schema(schema) => {
            conn.create_empty_table(name, schema.to_arrow_schema()?.into())
                .build()
                .await
        }
    }
}

enum CreateTableData {
    Batches(Box<dyn RecordBatchReader + Send>),
    Schema(RootSchema),
}
