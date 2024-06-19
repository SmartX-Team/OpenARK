use anyhow::{bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    model_storage_binding::{
        ModelStorageBindingDeletionPolicy, ModelStorageBindingSpec, ModelStorageBindingState,
        ModelStorageBindingStatus, ModelStorageBindingStorageSourceSpec,
        ModelStorageBindingStorageSpec, ModelStorageBindingSyncPolicy,
    },
    storage::ModelStorageSpec,
};
use kube::{core::ObjectMeta, ResourceExt};
use tracing::{error, instrument, Level};

use super::{model::ModelValidator, storage::ModelStorageValidator};

pub struct ModelStorageBindingValidator<'namespace, 'kube> {
    pub model: ModelValidator<'namespace, 'kube>,
    pub model_storage: ModelStorageValidator<'namespace, 'kube>,
    pub namespace: &'namespace str,
    pub name: &'namespace str,
}

impl<'namespace, 'kube> ModelStorageBindingValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_storage_binding(
        &self,
        spec: &ModelStorageBindingSpec,
    ) -> Result<UpdateContext> {
        let ctx = self.load_context(spec).await?;

        self.validate_model_storage_binding_with(ctx, spec).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_binding_with(
        &self,
        ctx: Context<'_>,
        spec: &ModelStorageBindingSpec,
    ) -> Result<UpdateContext> {
        let Context {
            model,
            state:
                State {
                    storage_source,
                    storage_source_binding_name,
                    storage_target,
                    storage_target_name,
                },
        } = ctx;

        let storage = ModelStorageBindingStorageSpec {
            source: storage_source.as_ref().map(|storage| storage.as_deref()),
            source_binding_name: storage_source_binding_name.as_deref(),
            target: &storage_target,
            target_name: storage_target_name,
        };

        self.model_storage.bind_model(storage, &model).await?;

        let model_name = model.name_any();
        let storage_source_name = storage_source.as_ref().map(|spec| spec.name.into());
        let storage_sync_policy = storage_source.as_ref().map(|spec| spec.sync_policy);
        let storage_target_name = storage_target_name.into();

        Ok(UpdateContext {
            deletion_policy: spec.deletion_policy,
            model: Some(model.spec),
            model_name: Some(model_name),
            state: ModelStorageBindingState::Ready,
            storage_source: storage_source.map(|spec| spec.storage),
            storage_source_name,
            storage_source_binding_name,
            storage_sync_policy,
            storage_target: Some(storage_target),
            storage_target_name: Some(storage_target_name),
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, spec: &ModelStorageBindingSpec) -> Result<()> {
        match self.load_context(spec).await {
            Ok(ctx) => self.delete_with(ctx, spec).await,
            Err(error) => {
                let Self {
                    namespace, name, ..
                } = self;

                error!("failed to delete model storage binding gracefully ({namespace}/{name}): {error}");
                Ok(())
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete_with(&self, ctx: Context<'_>, spec: &ModelStorageBindingSpec) -> Result<()> {
        let Context {
            model,
            state:
                State {
                    storage_source,
                    storage_source_binding_name,
                    storage_target,
                    storage_target_name,
                },
        } = ctx;

        let storage = ModelStorageBindingStorageSpec {
            source: storage_source.as_ref().map(|storage| storage.as_deref()),
            source_binding_name: storage_source_binding_name.as_deref(),
            target: &storage_target,
            target_name: storage_target_name,
        };

        self.model_storage
            .unbind_model(storage, &model, spec.deletion_policy)
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn update(
        &self,
        spec: &ModelStorageBindingSpec,
        last_status: &ModelStorageBindingStatus,
    ) -> Result<Option<UpdateContext>> {
        let ctx = self.load_context(spec).await?;

        // Assert: model should not be changed
        if last_status.model.as_ref() != Some(&ctx.model.spec) {
            bail!("model should be immutable")
        }

        // Test changed
        let state_last = State {
            storage_source: last_status
                .storage_source
                .clone()
                .zip(last_status.storage_source_name.as_deref())
                .zip(last_status.storage_sync_policy)
                .map(
                    |((storage, name), sync_policy)| ModelStorageBindingStorageSourceSpec {
                        name,
                        storage,
                        sync_policy,
                    },
                ),
            storage_source_binding_name: last_status.storage_source_binding_name.clone(),
            storage_target: last_status.storage_target.clone().unwrap(),
            storage_target_name: last_status.storage_target_name.as_deref().unwrap(),
        };
        if state_last == ctx.state {
            return Ok(None);
        }

        // Unbind
        {
            let ctx = Context {
                model: ModelCrd {
                    metadata: ObjectMeta {
                        name: last_status.model_name.clone(),
                        ..Default::default()
                    },
                    spec: last_status.model.clone().unwrap(),
                    status: None,
                },
                state: state_last,
            };

            self.delete_with(ctx, spec).await?;
        }

        // (Re)bind
        self.validate_model_storage_binding_with(ctx, spec)
            .await
            .map(Some)
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn load_context<'a>(&self, spec: &'a ModelStorageBindingSpec) -> Result<Context<'a>> {
        let model = self
            .model
            .kubernetes_storage
            .load_model(&spec.model)
            .await?;

        let storage_source = match spec.storage.source() {
            Some((source_name, sync_policy)) => self
                .model
                .kubernetes_storage
                .load_model_storage(source_name)
                .await
                .map(|source| {
                    Some(ModelStorageBindingStorageSourceSpec {
                        name: source_name,
                        storage: source.spec,
                        sync_policy,
                    })
                })?,
            None => None,
        };

        let storage_target_name = spec.storage.target();
        let storage_target = self
            .model
            .kubernetes_storage
            .load_model_storage(storage_target_name)
            .await?;

        Ok(Context {
            model,
            state: State {
                storage_source,
                storage_source_binding_name: spec.storage.source_binding_name().map(Into::into),
                storage_target: storage_target.spec,
                storage_target_name: storage_target_name.as_str(),
            },
        })
    }
}

struct Context<'a> {
    model: ModelCrd,
    state: State<'a>,
}

pub(crate) struct UpdateContext {
    pub(crate) deletion_policy: ModelStorageBindingDeletionPolicy,
    pub(crate) model: Option<ModelSpec>,
    pub(crate) model_name: Option<String>,
    pub(crate) state: ModelStorageBindingState,
    pub(crate) storage_source: Option<ModelStorageSpec>,
    pub(crate) storage_source_binding_name: Option<String>,
    pub(crate) storage_source_name: Option<String>,
    pub(crate) storage_sync_policy: Option<ModelStorageBindingSyncPolicy>,
    pub(crate) storage_target: Option<ModelStorageSpec>,
    pub(crate) storage_target_name: Option<String>,
}

#[derive(PartialEq)]
struct State<'a> {
    storage_source: Option<ModelStorageBindingStorageSourceSpec<'a, ModelStorageSpec>>,
    storage_source_binding_name: Option<String>,
    storage_target: ModelStorageSpec,
    storage_target_name: &'a str,
}
