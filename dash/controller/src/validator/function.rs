use dash_api::{function::FunctionSpec, kube::Client, model::ModelFieldKindNativeSpec};
use dash_provider::{client::FunctionActorClient, storage::KubernetesStorageClient};
use ipis::core::anyhow::{bail, Result};

use super::model::ModelValidator;

pub struct FunctionValidator<'a> {
    pub kube: &'a Client,
}

impl<'a> FunctionValidator<'a> {
    pub async fn validate_function(
        &self,
        spec: FunctionSpec,
    ) -> Result<FunctionSpec<ModelFieldKindNativeSpec>> {
        let model_validator = ModelValidator {
            kubernetes_storage: KubernetesStorageClient { kube: self.kube },
        };
        let input = model_validator.validate_fields(spec.input).await?;
        let output = match spec.output {
            Some(output) => Some(model_validator.validate_fields(output).await?),
            None => None,
        };

        let actor = spec.actor;
        if let Err(e) = FunctionActorClient::try_new(self.kube, &actor).await {
            bail!("failed to validate function actor: {e}");
        }

        Ok(FunctionSpec {
            input,
            output,
            actor,
        })
    }
}
