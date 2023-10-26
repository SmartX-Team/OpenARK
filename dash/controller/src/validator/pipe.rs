use anyhow::Result;
use dash_api::{
    model::{
        ModelFieldKindExtendedSpec, ModelFieldKindSpec, ModelFieldSpec, ModelFieldsNativeSpec,
    },
    pipe::PipeSpec,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::Client;

use super::model::ModelValidator;

pub struct PipeValidator<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> PipeValidator<'namespace, 'kube> {
    pub async fn validate_pipe(&self, spec: PipeSpec) -> Result<PipeSpec<ModelFieldsNativeSpec>> {
        let model_validator = ModelValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: self.namespace,
                kube: self.kube,
            },
        };
        let validate_model = |name| async {
            model_validator
                .validate_fields(vec![ModelFieldSpec {
                    name: "/".into(),
                    kind: ModelFieldKindSpec::Extended(ModelFieldKindExtendedSpec::Model { name }),
                    attribute: Default::default(),
                }])
                .await
        };

        let PipeSpec { input, output } = spec;

        Ok(PipeSpec {
            input: validate_model(input).await?,
            output: validate_model(output).await?,
        })
    }
}
