use anyhow::{anyhow, bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingSpec,
        ModelStorageBindingState, ModelStorageBindingStatus, ModelStorageBindingStorageSourceSpec,
        ModelStorageBindingStorageSpec, ModelStorageBindingSyncPolicy,
    },
    storage::{ModelStorageCrd, ModelStorageSpec},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{core::ObjectMeta, Resource, ResourceExt};
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
        binding: &ModelStorageBindingCrd,
    ) -> Result<UpdateContext> {
        let ctx = self.load_context(&binding.spec).await?;

        self.validate_model_storage_binding_with(ctx, binding).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_binding_with(
        &self,
        ctx: Context<'_>,
        binding: &ModelStorageBindingCrd,
    ) -> Result<UpdateContext> {
        let Context {
            model,
            state:
                State {
                    storage_source,
                    storage_source_binding_name,
                    storage_source_uid,
                    storage_target,
                    storage_target_name,
                    storage_target_uid,
                },
        } = ctx;

        let storage = ModelStorageBindingStorageSpec {
            source: storage_source.as_ref().map(|storage| storage.as_deref()),
            source_binding_name: storage_source_binding_name.as_deref(),
            target: &storage_target,
            target_name: storage_target_name,
        };

        self.model_storage
            .bind_model(binding, storage, &model)
            .await?;

        let model_name = model.name_any();
        let storage_source_name = storage_source.as_ref().map(|spec| spec.name.into());
        let storage_sync_policy = storage_source.as_ref().map(|spec| spec.sync_policy);
        let storage_target_name = storage_target_name.to_string();

        let mut owner_references = vec![
            OwnerReference {
                api_version: ModelCrd::api_version(&()).into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: ModelCrd::kind(&()).into(),
                name: model_name.clone(),
                uid: model
                    .uid()
                    .ok_or_else(|| anyhow!("failed to get model uid: {model_name}"))?,
            },
            OwnerReference {
                api_version: ModelStorageCrd::api_version(&()).into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: ModelStorageCrd::kind(&()).into(),
                name: storage_target_name.clone(),
                uid: storage_target_uid.clone(),
            },
        ];
        if let Some((name, uid)) = storage_source_name.clone().zip(storage_source_uid.clone()) {
            owner_references.push(OwnerReference {
                api_version: ModelStorageCrd::api_version(&()).into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: ModelStorageCrd::kind(&()).into(),
                name,
                uid,
            })
        }

        Ok(UpdateContext {
            deletion_policy: binding.spec.deletion_policy,
            model: Some(model.spec),
            model_name: Some(model_name),
            owner_references: Some(owner_references),
            state: ModelStorageBindingState::Ready,
            storage_source: storage_source.map(|spec| spec.storage),
            storage_source_name,
            storage_source_binding_name,
            storage_source_uid: storage_source_uid,
            storage_sync_policy,
            storage_target: Some(storage_target),
            storage_target_name: Some(storage_target_name),
            storage_target_uid: Some(storage_target_uid),
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
                    storage_source_uid: _,
                    storage_target,
                    storage_target_name,
                    storage_target_uid: _,
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
        binding: &ModelStorageBindingCrd,
        last_status: &ModelStorageBindingStatus,
    ) -> Result<Option<UpdateContext>> {
        let ctx = self.load_context(&binding.spec).await?;

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
            storage_source_uid: last_status.storage_source_uid.clone(),
            storage_target: last_status.storage_target.clone().unwrap(),
            storage_target_name: last_status.storage_target_name.as_deref().unwrap(),
            storage_target_uid: last_status.storage_target_uid.clone().unwrap_or_default(),
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

            self.delete_with(ctx, &binding.spec).await?;
        }

        // (Re)bind
        self.validate_model_storage_binding_with(ctx, binding)
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
            Some((source_name, _)) => self
                .model
                .kubernetes_storage
                .load_model_storage(source_name)
                .await
                .map(Some)?,
            None => None,
        };
        let storage_source_uid = storage_source.as_ref().and_then(|cr| cr.uid());
        let storage_source_spec =
            spec.storage
                .source()
                .zip(storage_source)
                .map(
                    |((name, sync_policy), cr)| ModelStorageBindingStorageSourceSpec {
                        name: name.as_str(),
                        storage: cr.spec,
                        sync_policy,
                    },
                );

        let storage_source_binding_name = spec.storage.source_binding_name().map(Into::into);

        let storage_target_name = spec.storage.target().as_str();
        let storage_target = self
            .model
            .kubernetes_storage
            .load_model_storage(storage_target_name)
            .await?;
        let storage_target_uid = storage_target.uid().ok_or_else(|| {
            anyhow!("failed to get target model storage uid: {storage_target_name}")
        })?;
        let storage_target_spec = storage_target.spec;

        Ok(Context {
            model,
            state: State {
                storage_source: storage_source_spec,
                storage_source_binding_name,
                storage_source_uid,
                storage_target: storage_target_spec,
                storage_target_name,
                storage_target_uid,
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
    pub(crate) owner_references: Option<Vec<OwnerReference>>,
    pub(crate) state: ModelStorageBindingState,
    pub(crate) storage_source: Option<ModelStorageSpec>,
    pub(crate) storage_source_binding_name: Option<String>,
    pub(crate) storage_source_name: Option<String>,
    pub(crate) storage_source_uid: Option<String>,
    pub(crate) storage_sync_policy: Option<ModelStorageBindingSyncPolicy>,
    pub(crate) storage_target: Option<ModelStorageSpec>,
    pub(crate) storage_target_name: Option<String>,
    pub(crate) storage_target_uid: Option<String>,
}

#[derive(PartialEq)]
struct State<'a> {
    storage_source: Option<ModelStorageBindingStorageSourceSpec<'a, ModelStorageSpec>>,
    storage_source_binding_name: Option<String>,
    storage_source_uid: Option<String>,
    storage_target: ModelStorageSpec,
    storage_target_name: &'a str,
    storage_target_uid: String,
}
