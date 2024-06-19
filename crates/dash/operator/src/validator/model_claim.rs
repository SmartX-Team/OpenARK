use anyhow::{anyhow, bail, Result};
use dash_api::{
    model_claim::{ModelClaimCrd, ModelClaimDeletionPolicy, ModelClaimSpec, ModelClaimState},
    model_storage_binding::ModelStorageBindingCrd,
};
use dash_provider::storage::KubernetesStorageClient;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{api::ObjectMeta, Resource, ResourceExt};
use tracing::{instrument, Level};

use crate::optimizer::model_claim::ModelClaimOptimizer;

pub struct ModelClaimValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
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
                    spec: Some(crd.spec.clone()),
                    state: ModelClaimState::Ready,
                });
            }
        }

        // create model storage binding
        let optimizer = ModelClaimOptimizer::new(self.kubernetes_storage);
        let binding = optimizer
            .optimize_model_storage_binding(field_manager, &model, crd.spec.storage)
            .await?;

        let owner_references = match binding {
            Some(cr) => vec![to_owner_reference(cr.metadata)?],
            None => bail!("failed to find suitable model storage"),
        };

        Ok(UpdateContext {
            owner_references: Some(owner_references),
            spec: Some(crd.spec.clone()),
            state: ModelClaimState::Ready,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, crd: &ModelClaimCrd) -> Result<()> {
        match crd.status.as_ref().and_then(|status| status.spec.as_ref()) {
            Some(spec) => match spec.deletion_policy {
                ModelClaimDeletionPolicy::Delete => self.delete_model(crd).await,
                ModelClaimDeletionPolicy::Retain => Ok(()),
            },
            None => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete_model(&self, crd: &ModelClaimCrd) -> Result<()> {
        self.kubernetes_storage.delete_model(&crd.name_any()).await
    }
}

pub(crate) struct UpdateContext {
    pub(crate) owner_references: Option<Vec<OwnerReference>>,
    pub(crate) spec: Option<ModelClaimSpec>,
    pub(crate) state: ModelClaimState,
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
