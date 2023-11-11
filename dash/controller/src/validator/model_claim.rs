use anyhow::{bail, Result};
use dash_api::model_claim::{ModelClaimCrd, ModelClaimDeletionPolicy};
use dash_optimizer_client::OptimizerClient;
use dash_provider::storage::KubernetesStorageClient;
use kube::ResourceExt;
use tracing::{instrument, Level};

pub struct ModelClaimValidator<'client, 'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
    pub optimizer: &'client OptimizerClient,
}

impl<'client, 'namespace, 'kube> ModelClaimValidator<'client, 'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_claim(
        &self,
        field_manager: &str,
        crd: &ModelClaimCrd,
    ) -> Result<()> {
        // create model
        let model = self
            .kubernetes_storage
            .load_model_or_create_as_dynamic(field_manager, &crd.name_any())
            .await?;

        // check model is already binded
        if !self
            .kubernetes_storage
            .load_model_storage_bindings(&model.name_any())
            .await?
            .is_empty()
        {
            return Ok(());
        }

        // create model storage binding
        match self
            .optimizer
            .optimize_model_storage_binding(
                field_manager,
                self.kubernetes_storage,
                &model,
                crd.spec.binding_policy,
                crd.spec.storage,
            )
            .await?
        {
            Some(_) => Ok(()),
            None => bail!(
                "failed to bind to proper model storage: {name:?}",
                name = crd.name_any(),
            ),
        }
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
