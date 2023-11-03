use anyhow::Result;
use dash_api::{
    model::{
        ModelFieldKindExtendedSpec, ModelFieldKindSpec, ModelFieldSpec, ModelFieldsNativeSpec,
    },
    pipe::{PipeExec, PipeSpec},
};
use dash_provider::storage::KubernetesStorageClient;
use kube::Client;
use straw_api::{pipe::StrawPipe, plugin::PluginContext};
use straw_provider::StrawSession;

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

        let PipeSpec {
            input,
            output,
            exec,
        } = spec;

        Ok(PipeSpec {
            input: validate_model(input.into()).await?,
            output: validate_model(output.into()).await?,
            exec: self.validate_exec(exec).await?,
        })
    }

    async fn validate_exec(&self, exec: PipeExec) -> Result<PipeExec> {
        match exec {
            PipeExec::Placeholder {} => Ok(exec),
            PipeExec::Straw(exec) => self.validate_exec_straw(exec).await.map(PipeExec::Straw),
        }
    }

    async fn validate_exec_straw(&self, exec: StrawPipe) -> Result<StrawPipe> {
        let ctx = PluginContext::default();
        let session = StrawSession::new(self.kube.clone(), Some(self.namespace.into()));
        session.create(&ctx, &exec).await.map(|()| exec)
    }
}
