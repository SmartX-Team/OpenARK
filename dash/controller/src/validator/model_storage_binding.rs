use dash_api::{
    model::ModelSpec, model_storage_binding::ModelStorageBindingSpec, storage::ModelStorageSpec,
};
use ipis::core::anyhow::Result;

use super::{model::ModelValidator, storage::ModelStorageValidator};

pub struct ModelStorageBindingValidator<'a> {
    pub model: ModelValidator<'a>,
    pub model_storage: ModelStorageValidator<'a>,
}

impl<'a> ModelStorageBindingValidator<'a> {
    pub async fn validate_model_storage_binding(
        &self,
        spec: &ModelStorageBindingSpec,
    ) -> Result<(ModelSpec, ModelStorageSpec)> {
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
            .bind_model(&storage, &model)
            .await
            .map(|()| (model.spec, storage.spec))
    }
}
