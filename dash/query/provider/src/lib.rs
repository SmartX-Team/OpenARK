mod arrow;
mod function;

use std::{
    collections::{btree_map::Keys, BTreeMap},
    sync::Arc,
};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use dash_api::{
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingState},
    pipe::{PipeCrd, PipeSpec, PipeState},
    storage::ModelStorageKindSpec,
};
pub use dash_pipe_provider::{deltalake, Name};
use dash_pipe_provider::{
    deltalake::{
        arrow::{compute::concat_batches, datatypes::Schema, record_batch::RecordBatch},
        DeltaTable,
    },
    messengers::{init_messenger, MessengerArgs},
    storage::{
        lakehouse::{decoder::TryIntoTableDecoder, StorageContext},
        StorageS3Args, Stream,
    },
};
use dash_provider::storage::ObjectStorageRef;
use deltalake::datafusion::prelude::DataFrame;
use futures::{future::try_join_all, Future};
use inflector::Inflector;
use itertools::Itertools;
use kube::{api::ListParams, Api, Client, ResourceExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{info, warn};

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
    ctx: StorageContext,
    tables: BTreeMap<String, Arc<DeltaTable>>,
}

impl QueryClient {
    pub async fn try_new(args: &QueryClientArgs) -> Result<Self> {
        let kube = Client::try_default()
            .await
            .map_err(|error| anyhow!("failed to init k8s client: {error}"))?;
        let namespace = args
            .namespace
            .as_deref()
            .unwrap_or(kube.default_namespace());

        let ctx = StorageContext::default();
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
            let (name, table, has_inited) =
                ctx.register_table_with_name(args, &model, None).await?;

            if has_inited {
                tables.insert(name, table);
            } else {
                warn!("Model {model:?} is not inited yet on {storage:?}; skipping...");
            }
        }

        // load functions after loading models
        for function in load_functions(&kube, &tables, namespace).await? {
            info!("Loading function: {function}");
            ctx.register_udf(function.try_into_udf(messenger.as_ref()).await?);
        }

        Ok(Self { ctx, tables })
    }

    pub fn list_table_names(&self) -> Keys<'_, String, Arc<DeltaTable>> {
        self.tables.keys()
    }

    pub async fn sql(&self, sql: &str) -> Result<DataFrame> {
        self.ctx
            .sql(sql)
            .await
            .map_err(|error| anyhow!("failed to query object metadata: {error}"))
    }

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

            let storage = status.storage?;
            let storage = match storage.into_target().kind {
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
                    ObjectStorageRef::load_storage_provider(&kube, namespace, &model_name, &storage)
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

async fn load_functions(
    kube: &Client,
    tables: &BTreeMap<String, Arc<DeltaTable>>,
    namespace: &str,
) -> Result<Vec<self::function::DashFunctionTemplate>> {
    async fn get_model_schema(
        tables: &BTreeMap<String, Arc<DeltaTable>>,
        name: &str,
    ) -> Result<Arc<Schema>> {
        match tables.get(&name.to_snake_case()) {
            Some(table) => table
                .get_state()
                .physical_arrow_schema(table.object_store())
                .await
                .map_err(|error| anyhow!("failed to load schema ({name}): {error}")),
            None => bail!("no such table: {name}"),
        }
    }

    let api = Api::<PipeCrd>::namespaced(kube.clone(), namespace);
    let lp = ListParams::default();
    let functions = api.list(&lp).await?.items;

    try_join_all(functions.into_iter().filter_map(|function| {
        let name: Name = function.name_any().parse().ok()?;

        let status = function.status?;
        if !matches!(status.state, PipeState::Ready) {
            return None;
        }

        let PipeSpec {
            input: model_in,
            output: model_out,
            exec: _,
        } = function.spec;

        Some(async move {
            let spec = PipeSpec {
                input: get_model_schema(tables, &model_in).await?,
                output: get_model_schema(tables, &model_out).await?,
                exec: (),
            };
            self::function::DashFunctionTemplate::new(name, model_in, spec)
        })
    }))
    .await
}
