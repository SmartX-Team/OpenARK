use anyhow::{anyhow, bail, Result};
use dash_api::{
    model_claim::{ModelClaimCrd, ModelClaimDeletionPolicy, ModelClaimState, ModelClaimStatus},
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy},
    storage::ModelStorageKind,
};
use dash_provider::storage::KubernetesStorageClient;
use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::apis::meta::v1::OwnerReference,
};
use kube::{api::ObjectMeta, Resource, ResourceExt};
use prometheus_http_query::Client as PrometheusClient;
use tracing::{instrument, Level};

use crate::optimizer::model_claim::ModelClaimOptimizer;

pub struct ModelClaimValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
    pub prometheus_client: &'kube PrometheusClient,
    pub prometheus_url: &'kube str,
}

impl<'namespace, 'kube> ModelClaimValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_claim(
        &self,
        field_manager: &str,
        crd: &ModelClaimCrd,
    ) -> Result<UpdateContext> {
        // create model
        let model = self
            .kubernetes_storage
            .load_model_or_create_as_dynamic(field_manager, &crd.name_any())
            .await?;

        // check model is already binded
        {
            let bindings = self
                .kubernetes_storage
                .load_model_storage_bindings(&model.name_any())
                .await?;

            if !bindings.is_empty() {
                let owner_references = bindings
                    .into_iter()
                    .map(|(metadata, _)| to_owner_reference(metadata))
                    .collect::<Result<_>>()?;
                return Ok(UpdateContext {
                    owner_references: Some(owner_references),
                    state: ModelClaimState::Ready,
                    resources: crd.spec.resources.clone(),
                    storage: crd.spec.storage,
                    storage_name: None,
                });
            }
        }

        // create model storage binding
        let optimizer = ModelClaimOptimizer::new(
            field_manager,
            self.kubernetes_storage,
            self.prometheus_client,
            crd.spec.binding_policy,
        );
        let deletion_policy = match crd.spec.deletion_policy {
            ModelClaimDeletionPolicy::Delete => ModelStorageBindingDeletionPolicy::Delete,
            ModelClaimDeletionPolicy::Retain => ModelStorageBindingDeletionPolicy::Retain,
        };
        let binding = optimizer
            .optimize_model_storage_binding(
                &model,
                crd.spec.storage,
                crd.spec.resources.clone(),
                deletion_policy,
            )
            .await?;

        let (owner_references, storage_name) = match binding {
            Some(cr) => {
                let owner_references = vec![to_owner_reference(cr.metadata.clone())?];
                let storage_name = cr.spec.storage.target().clone();
                (owner_references, storage_name)
            }
            None => bail!("failed to find suitable model storage"),
        };

        Ok(UpdateContext {
            owner_references: Some(owner_references),
            resources: crd.spec.resources.clone(),
            state: ModelClaimState::Ready,
            storage: crd.spec.storage,
            storage_name: Some(storage_name),
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_claim_replacement(
        &self,
        field_manager: &str,
        crd: &ModelClaimCrd,
        last_status: &ModelClaimStatus,
    ) -> Result<Option<UpdateContext>> {
        // TODO: to be implemented
        bail!("Unimplemented yet!")
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, crd: &ModelClaimCrd) -> Result<()> {
        match crd.spec.deletion_policy {
            ModelClaimDeletionPolicy::Delete => self.delete_model(crd).await,
            ModelClaimDeletionPolicy::Retain => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete_model(&self, crd: &ModelClaimCrd) -> Result<()> {
        self.kubernetes_storage.delete_model(&crd.name_any()).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete_model_storage_bindings(&self, crd: &ModelClaimCrd) -> Result<()> {
        self.kubernetes_storage
            .delete_model_storage_binding_by_model(&crd.name_any())
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn replace(
        &self,
        field_manager: &str,
        crd: &ModelClaimCrd,
        last_status: &ModelClaimStatus,
    ) -> Result<Option<UpdateContext>> {
        // TODO: to be implemented (ASAP)
        bail!("Unimplemented yet!")
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn update(
        &self,
        field_manager: &str,
        crd: &ModelClaimCrd,
        last_status: &ModelClaimStatus,
    ) -> Result<Option<UpdateContext>> {
        if !is_changed(crd, last_status) {
            return Ok(None);
        }

        if !crd.spec.allow_replacement {
            // Update the storage

            // Unbind
            self.delete_model_storage_bindings(crd).await?;

            // (Re)bind
            self.validate_model_claim(field_manager, crd)
                .await
                .map(Some)
        } else {
            self.replace(field_manager, crd, last_status).await
        }
    }
}

pub(crate) struct UpdateContext {
    pub(crate) owner_references: Option<Vec<OwnerReference>>,
    pub(crate) resources: Option<ResourceRequirements>,
    pub(crate) state: ModelClaimState,
    pub(crate) storage: Option<ModelStorageKind>,
    pub(crate) storage_name: Option<String>,
}

#[derive(PartialEq)]
struct State<'a> {
    resources: Option<&'a ResourceRequirements>,
    storage: Option<ModelStorageKind>,
    storage_name: &'a str,
}

fn is_changed(crd: &ModelClaimCrd, last_status: &ModelClaimStatus) -> bool {
    let (before, after) = match (
        last_status.storage_name.as_deref(),
        crd.spec.storage_name.as_deref(),
    ) {
        (Some(before), Some(after)) => (before, after),
        (Some(_), None) => return true,
        (None, Some(_)) => return true,
        (None, None) => return false,
    };

    // Test changed
    let state = State {
        resources: crd.spec.resources.as_ref(),
        storage: crd.spec.storage,
        storage_name: after,
    };
    let state_last = State {
        resources: last_status.resources.as_ref(),
        storage: last_status.storage,
        storage_name: before,
    };
    state_last == state
}

fn to_owner_reference(metadata: ObjectMeta) -> Result<OwnerReference> {
    let name = metadata
        .name
        .ok_or_else(|| anyhow!("failed to get model storage binding name"))?;
    let uid = metadata
        .uid
        .ok_or_else(|| anyhow!("failed to get model storage binding uid: {name}"))?;

    Ok(OwnerReference {
        api_version: ModelStorageBindingCrd::api_version(&()).into(),
        block_owner_deletion: Some(true),
        controller: None,
        kind: ModelStorageBindingCrd::kind(&()).into(),
        name,
        uid,
    })
}
