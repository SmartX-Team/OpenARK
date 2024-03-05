use anyhow::Result;
use ark_core_k8s::data::Name;
use dash_api::{
    function::{FunctionExec, FunctionSpec},
    model::{
        ModelFieldKindExtendedSpec, ModelFieldKindSpec, ModelFieldSpec, ModelFieldsNativeSpec,
    },
};
use dash_provider::storage::KubernetesStorageClient;
use kube::Client;
use straw_api::{
    function::{StrawFunction, StrawFunctionType},
    plugin::PluginContext,
};
use straw_provider::StrawSession;
use tracing::{instrument, Level};

use super::model::ModelValidator;

pub struct FunctionValidator<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> FunctionValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_function(
        &self,
        spec: FunctionSpec,
    ) -> Result<FunctionSpec<ModelFieldsNativeSpec>> {
        let FunctionSpec {
            input,
            output,
            exec,
            type_,
            volatility,
        } = spec;

        let models = Models { input, output };

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

        Ok(FunctionSpec {
            input: validate_model(models.input.clone().into()).await?,
            output: validate_model(models.output.clone().into()).await?,
            exec: self.validate_exec(type_, exec, models).await?,
            type_,
            volatility,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_exec(
        &self,
        type_: StrawFunctionType,
        exec: FunctionExec,
        models: Models,
    ) -> Result<FunctionExec> {
        match exec {
            FunctionExec::Placeholder {} => Ok(exec),
            FunctionExec::Straw(function) => self
                .validate_exec_straw(type_, function, models)
                .await
                .map(FunctionExec::Straw),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_exec_straw(
        &self,
        type_: StrawFunctionType,
        function: StrawFunction,
        models: Models,
    ) -> Result<StrawFunction> {
        if type_ == StrawFunctionType::Pipe {
            self.validate_model_storage_binding(&models).await?;
        }

        let ctx = PluginContext::new(type_, Some(models.input), Some(models.output));
        let session = StrawSession::new(self.kube.clone(), Some(self.namespace.into()));
        session.create(&ctx, &function).await.map(|()| function)
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_binding(&self, models: &Models) -> Result<()> {
        let client = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        client.ensure_model_storage_binding(&models.input).await?;
        client.ensure_model_storage_binding(&models.output).await?;
        Ok(())
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, spec: &FunctionSpec) -> Result<()> {
        match &spec.exec {
            FunctionExec::Placeholder {} => Ok(()),
            FunctionExec::Straw(function) => self.delete_exec_straw(function).await,
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete_exec_straw(&self, function: &StrawFunction) -> Result<()> {
        let session = StrawSession::new(self.kube.clone(), Some(self.namespace.into()));
        session.delete(function).await
    }
}

struct Models {
    input: Name,
    output: Name,
}
