use anyhow::{bail, Result};
use dash_api::{model::ModelFieldKindNativeSpec, task::TaskSpec};
use dash_provider::{client::TaskActorClient, storage::KubernetesStorageClient};
use kube::Client;
use tracing::{instrument, Level};

use super::model::ModelValidator;

pub struct TaskValidator<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> TaskValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_task(
        &self,
        spec: TaskSpec,
    ) -> Result<TaskSpec<ModelFieldKindNativeSpec>> {
        let model_validator = ModelValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: self.namespace,
                kube: self.kube,
            },
        };
        let input = model_validator.validate_fields(spec.input).await?;

        let actor = spec.actor;
        if let Err(e) = TaskActorClient::try_new(self.namespace, self.kube, &actor).await {
            bail!("failed to validate task actor: {e}");
        }

        Ok(TaskSpec { input, actor })
    }
}
