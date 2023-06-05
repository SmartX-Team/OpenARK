use anyhow::{bail, Result};
use dash_api::{function::FunctionSpec, model::ModelFieldKindNativeSpec};
use dash_provider::{client::FunctionActorClient, storage::KubernetesStorageClient};
use kube::Client;

use super::model::ModelValidator;

pub struct FunctionValidator<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> FunctionValidator<'namespace, 'kube> {
    pub async fn validate_function(
        &self,
        spec: FunctionSpec,
    ) -> Result<FunctionSpec<ModelFieldKindNativeSpec>> {
        let model_validator = ModelValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: self.namespace,
                kube: self.kube,
            },
        };
        let input = model_validator.validate_fields(spec.input).await?;
        let output = match spec.output {
            Some(output) => Some(model_validator.validate_fields(output).await?),
            None => None,
        };

        let actor = spec.actor;
        if let Err(e) = FunctionActorClient::try_new(self.namespace, self.kube, &actor).await {
            bail!("failed to validate function actor: {e}");
        }

        Ok(FunctionSpec {
            input,
            output,
            actor,
        })
    }
}
