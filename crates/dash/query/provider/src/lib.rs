mod arrow;
mod function;

use std::{
    collections::{btree_map::Keys, BTreeMap},
    ops,
    sync::Arc,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use dash_api::{
    function::{FunctionCrd, FunctionSpec, FunctionState},
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingState},
    storage::ModelStorageKindSpec,
};
use dash_pipe_api::storage::StorageS3Args;
pub use dash_pipe_provider::{deltalake, Name};
use dash_pipe_provider::{
    deltalake::{
        arrow::{compute::concat_batches, datatypes::Schema, record_batch::RecordBatch},
        datafusion::execution::context::SessionContext,
        delta_datafusion::DataFusionMixins,
        DeltaTable,
    },
    messengers::{init_messenger, Messenger, MessengerArgs},
    schema::arrow::decoder::TryIntoTableDecoder,
    storage::{
        deltalake::{StorageSessionContext, StorageTableState},
        Stream,
    },
};
use dash_provider::storage::ObjectStorageSession;
use deltalake::datafusion::prelude::DataFrame;
use futures::{
    stream::{self, FuturesUnordered},
    Future, StreamExt,
};
use inflector::Inflector;
use itertools::Itertools;
use kube::{api::ListParams, Api, Client, ResourceExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{info, instrument, warn, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct QueryClientArgs {
    #[command(flatten)]
    pub messenger: MessengerArgs,

    /// Set a target namespace
    #[arg(long, env = "DASH_NAMESPACE", value_name = "NAMESPACE")]
    pub namespace: Option<String>,
}

#[derive(Clone)]
pub struct QueryClient {
    ctx: SessionContext,
    tables: BTreeMap<String, Arc<DeltaTable>>,
}

impl QueryClient {
    #[instrument(level = Level::INFO, skip(args), err(Display))]
    pub async fn try_new(args: &QueryClientArgs) -> Result<Self> {
        let kube = Client::try_default()
            .await
            .map_err(|error| anyhow!("failed to init k8s client: {error}"))?;
        let namespace = args
            .namespace
            .as_deref()
            .unwrap_or(kube.default_namespace());

        let ctx = SessionContext::default();
        let mut tables = BTreeMap::default();

        // load messenger
        let messenger = init_messenger(&args.messenger).await?;

        // load models
        for (model, storage, args) in load_models(&kube, namespace).await? {
            if tables.contains_key(&model) {
                continue;
            }

            info!("Loading model: {model}");
            let args = args.await?;
            let (name, table, state) = ctx.register_table_with_name(&args, &model, None).await?;

            match state {
                StorageTableState::Inited => {
                    tables.insert(name, table);
                }
                StorageTableState::Uninited => {
                    warn!("Model {model:?} is not inited yet on {storage:?}; skipping...");
                }
            }
        }

        // load functions after loading models
        for function in load_functions(&kube, messenger.as_ref(), &tables, namespace).await? {
            ctx.register_udf(function.into());
        }

        Ok(Self { ctx, tables })
    }

    pub fn list_table_names(&self) -> Keys<'_, String, Arc<DeltaTable>> {
        self.tables.keys()
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn sql(&self, sql: &str) -> Result<DataFrame> {
        self.ctx
            .sql(sql)
            .await
            .map_err(|error| anyhow!("failed to query object metadata: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn sql_and_decode<Value>(&self, sql: &str) -> Result<Stream<Value>>
    where
        Value: 'static + Send + DeserializeOwned,
    {
        self.sql(sql)
            .await?
            .try_into_decoder()
            .await
            .map_err(|error| anyhow!("failed to decode object metadata: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn sql_and_flatten(&self, sql: &str) -> Result<Option<RecordBatch>> {
        self.sql(sql)
            .await?
            .collect()
            .await
            .map_err(|error| anyhow!("failed to collect object metadata: {error}"))
            .and_then(|records| {
                records
                    .first()
                    .map(|record_sample| {
                        concat_batches(&record_sample.schema(), &records)
                            .map(self::arrow::IntoFlattened::into_flattened)
                            .map_err(|error| anyhow!("failed to concat object metadata: {error}"))
                    })
                    .transpose()
            })
    }
}

impl ops::Deref for QueryClient {
    type Target = SessionContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

#[instrument(level = Level::INFO, skip(kube), err(Display))]
async fn load_models<'a>(
    kube: &'a Client,
    namespace: &'a str,
) -> Result<
    impl Iterator<
            Item = (
                String,
                String,
                impl Future<Output = Result<StorageS3Args>> + 'a,
            ),
        > + 'a,
> {
    let api = Api::<ModelStorageBindingCrd>::namespaced(kube.clone(), namespace);
    let lp = ListParams::default();
    let bindings = api.list(&lp).await?.items;

    Ok(bindings
        .into_iter()
        .unique_by(|binding| {
            let model_name = binding.spec.model.clone();
            let storage_name = binding.spec.storage.target().clone();
            (model_name, storage_name)
        })
        .filter_map(move |binding| {
            let model_name = binding.spec.model;
            let storage_name = binding.spec.storage.target().clone();

            let status = binding.status?;
            if !matches!(status.state, ModelStorageBindingState::Ready) {
                return None;
            }

            let storage = status.storage_target?;
            let storage = match storage.kind {
                ModelStorageKindSpec::ObjectStorage(spec) => spec,
                storage => {
                    warn!(
                        "Sorry, but the {kind:?} is not supported yet: {model_name}",
                        kind = storage.to_kind(),
                    );
                    return None;
                }
            };

            let kube = kube.clone();

            let args = {
                let model_name = model_name.clone();
                async move {
                    ObjectStorageSession::load_storage_provider(
                        &kube,
                        namespace,
                        &model_name,
                        None,
                        &storage,
                        None,
                    )
                    .await
                    .map(|object_storage| {
                        let credentials = object_storage.fetch_provider();
                        StorageS3Args {
                            access_key: credentials.access_key,
                            region: StorageS3Args::default_region().into(),
                            s3_endpoint: object_storage.endpoint,
                            secret_key: credentials.secret_key,
                        }
                    })
                }
            };

            Some((model_name, storage_name, args))
        }))
}

#[instrument(level = Level::INFO, skip(kube, messenger, tables), err(Display))]
async fn load_functions(
    kube: &Client,
    messenger: &dyn Messenger,
    tables: &BTreeMap<String, Arc<DeltaTable>>,
    namespace: &str,
) -> Result<Vec<self::function::DashFunction>> {
    async fn get_model_schema(
        tables: &BTreeMap<String, Arc<DeltaTable>>,
        name: &str,
    ) -> Option<Arc<Schema>> {
        let table = tables.get(&name.to_snake_case())?;
        match table
            .snapshot()
            .and_then(|snapshot| snapshot.arrow_schema())
        {
            Ok(schema) => Some(schema),
            Err(error) => {
                warn!("failed to load function schema ({name}): {error}; skipping...");
                None
            }
        }
    }

    let api = Api::<FunctionCrd>::namespaced(kube.clone(), namespace);
    let lp = ListParams::default();
    let functions = api.list(&lp).await?.items;

    Ok(stream::iter(functions)
        .filter_map(|function| async move {
            let function_name = function.name_any();
            let name: Name = function_name.parse().ok()?;
            info!("Loading function: {name}");

            let status = function.status?;
            if !matches!(status.state, FunctionState::Ready) {
                return None;
            }

            let FunctionSpec {
                input: model_in,
                output: model_out,
                exec: _,
                type_,
                volatility,
            } = function.spec;

            let spec = FunctionSpec {
                input: get_model_schema(tables, &model_in).await?,
                output: get_model_schema(tables, &model_out).await?,
                exec: (),
                type_,
                volatility,
            };
            match self::function::DashFunction::try_new(messenger, name, model_in, spec).await {
                Ok(function) => Some(function),
                Err(error) => {
                    warn!("failed to load function ({function_name}): {error}; skipping...");
                    None
                }
            }
        })
        .collect::<FuturesUnordered<_>>()
        .await
        .into_iter()
        .collect())
}
