mod arrow;

use anyhow::{anyhow, Result};
use clap::Parser;
use dash_api::{
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingState},
    storage::ModelStorageKindSpec,
};
pub use dash_pipe_provider::{deltalake, Name};
use dash_pipe_provider::{
    deltalake::arrow::{compute::concat_batches, record_batch::RecordBatch},
    storage::{
        lakehouse::{decoder::TryIntoTableDecoder, StorageContext},
        StorageS3Args, Stream,
    },
};
use dash_provider::storage::ObjectStorageRef;
use deltalake::datafusion::prelude::DataFrame;
use futures::future::try_join_all;
use itertools::Itertools;
use kube::{api::ListParams, Api, Client};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::warn;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Parser)]
pub struct QueryClientArgs {
    /// Set a target namespace
    #[arg(long, env = "DASH_NAMESPACE", value_name = "NAMESPACE")]
    pub namespace: Option<String>,
}

pub struct QueryClient {
    ctx: StorageContext,
    tables: Vec<Name>,
}

impl QueryClient {
    pub async fn try_new(args: &QueryClientArgs) -> Result<Self> {
        let ctx = StorageContext::default();
        let mut tables = vec![];

        for (model, args) in load_models(args.namespace.as_deref()).await? {
            let (table, _, has_inited) = ctx.register_table_with_name(args, &model, None).await?;
            if has_inited {
                tables.push(table);
            } else {
                warn!("Model {model:?} is not inited yet; skipping...");
            }
        }

        Ok(Self { ctx, tables })
    }

    pub fn list_table_names(&self) -> &[Name] {
        &self.tables
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

async fn load_models(namespace: Option<&str>) -> Result<Vec<(Name, StorageS3Args)>> {
    let kube = Client::try_default()
        .await
        .map_err(|error| anyhow!("failed to init k8s client: {error}"))?;
    let namespace = namespace.unwrap_or(kube.default_namespace());

    let api = Api::<ModelStorageBindingCrd>::namespaced(kube.clone(), namespace);
    let lp = ListParams::default();
    let bindings = api.list(&lp).await?.items;

    let models = try_join_all(
        bindings
            .into_iter()
            .unique_by(|binding| binding.spec.model.clone())
            .map(|binding| {
                let kube = kube.clone();

                async move {
                    let name: Name = binding.spec.model.parse()?;
                    let status = match binding.status.as_ref() {
                        Some(status) => status,
                        None => return Ok(None),
                    };
                    if !matches!(status.state, ModelStorageBindingState::Ready) {
                        return Ok(None);
                    }

                    let storage = match status.storage.as_ref() {
                        Some(kind) => kind,
                        _ => return Ok(None),
                    };
                    let storage = match &storage.target().kind {
                        ModelStorageKindSpec::ObjectStorage(spec) => spec,
                        storage => {
                            warn!(
                                "Sorry, but the {kind:?} is not supported yet: {name}",
                                kind = storage.to_kind(),
                            );
                            return Ok(None);
                        }
                    };

                    ObjectStorageRef::load_storage_provider(&kube, namespace, &name, storage)
                        .await
                        .map(|storage| {
                            let credentials = storage.fetch_provider();
                            Some((
                                name,
                                StorageS3Args {
                                    access_key: credentials.access_key,
                                    region: StorageS3Args::default_region().into(),
                                    s3_endpoint: storage.endpoint,
                                    secret_key: credentials.secret_key,
                                },
                            ))
                        })
                }
            }),
    )
    .await?;

    Ok(models.into_iter().flatten().collect())
}
