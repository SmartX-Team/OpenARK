use anyhow::Result;
use dash_api::{
    model::ModelSpec,
    model_storage_binding::{ModelStorageBindingSpec, ModelStorageBindingSyncPolicy},
    storage::ModelStorageSpec,
};

use super::{model::ModelValidator, storage::ModelStorageValidator};

pub struct ModelStorageBindingValidator<'namespace, 'kube> {
    pub model: ModelValidator<'namespace, 'kube>,
    pub model_storage: ModelStorageValidator<'namespace, 'kube>,
    pub sync_policy: Option<ModelStorageBindingSyncPolicy>,
}

impl<'namespace, 'kube> ModelStorageBindingValidator<'namespace, 'kube> {
    pub async fn validate_model_storage_binding(
        &self,
        spec: &ModelStorageBindingSpec,
    ) -> Result<(ModelSpec, ModelStorageSpec, ModelStorageBindingSyncPolicy)> {
        let model = self
            .model
            .kubernetes_storage
            .load_model(&spec.model)
            .await?;

        let storage = self
            .model
            .kubernetes_storage
            .load_model_storage(&spec.storage)
            .await?;

        self.model_storage
            .bind_model(&storage, &model, self.sync_policy)
            .await
            .map(|sync_policy| (model.spec, storage.spec, sync_policy))
    }
}
