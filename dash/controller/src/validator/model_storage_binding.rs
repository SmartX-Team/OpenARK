use anyhow::Result;
use dash_api::{
    model::ModelSpec,
    model_storage_binding::{
        ModelStorageBindingSpec, ModelStorageBindingStorageKind,
        ModelStorageBindingStorageKindClonedSpec, ModelStorageBindingStorageKindOwnedSpec,
        ModelStorageBindingStorageSpec,
    },
    storage::ModelStorageSpec,
};

use super::{model::ModelValidator, storage::ModelStorageValidator};

pub struct ModelStorageBindingValidator<'namespace, 'kube> {
    pub model: ModelValidator<'namespace, 'kube>,
    pub model_storage: ModelStorageValidator<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelStorageBindingValidator<'namespace, 'kube> {
    pub async fn validate_model_storage_binding(
        &self,
        spec: &ModelStorageBindingSpec,
    ) -> Result<(ModelSpec, ModelStorageBindingStorageKind<ModelStorageSpec>)> {
        let model = self
            .model
            .kubernetes_storage
            .load_model(&spec.model)
            .await?;

        let source = match spec.storage.source() {
            Some((source_name, sync_policy)) => self
                .model
                .kubernetes_storage
                .load_model_storage(source_name)
                .await
                .map(|source| Some((source_name.as_str(), source, sync_policy)))?,
            None => None,
        };
        let target_name = spec.storage.target();
        let target = self
            .model
            .kubernetes_storage
            .load_model_storage(target_name)
            .await?;
        let storage = ModelStorageBindingStorageSpec {
            source: source
                .as_ref()
                .map(|(source_name, source, sync_policy)| (*source_name, source, *sync_policy)),
            target: &target,
            target_name,
        };

        self.model_storage
            .bind_model(storage, &model)
            .await
            .map(|()| {
                let storage = match source {
                    Some((_, source, sync_policy)) => ModelStorageBindingStorageKind::Cloned(
                        ModelStorageBindingStorageKindClonedSpec {
                            source: source.spec,
                            target: target.spec,
                            sync_policy,
                        },
                    ),
                    None => ModelStorageBindingStorageKind::Owned(
                        ModelStorageBindingStorageKindOwnedSpec {
                            target: target.spec,
                        },
                    ),
                };
                (model.spec, storage)
            })
    }
}
