use anyhow::{bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    model_storage_binding::ModelStorageBindingSyncPolicy,
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec,
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec,
        ModelStorageSpec,
    },
};
use dash_provider::storage::{DatabaseStorageClient, KubernetesStorageClient, ObjectStorageClient};
use itertools::Itertools;
use kube::ResourceExt;

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    const SYNC_POLICY_DATABASE: ModelStorageBindingSyncPolicy =
        ModelStorageBindingSyncPolicy::Never;
    const SYNC_POLICY_KUBERNETES: ModelStorageBindingSyncPolicy =
        ModelStorageBindingSyncPolicy::Never;
    const SYNC_POLICY_OBJECT: ModelStorageBindingSyncPolicy = ModelStorageBindingSyncPolicy::Never;

    pub async fn validate_model_storage(&self, name: &str, spec: &ModelStorageSpec) -> Result<()> {
        self.validate_model_storage_conflict(name, spec.kind.to_kind())
            .await?;

        match &spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                self.validate_model_storage_database(spec).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => self.validate_model_storage_kubernetes(spec),
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.validate_model_storage_object(name, spec).await
            }
        }
    }

    async fn validate_model_storage_conflict(
        &self,
        name: &str,
        kind: ModelStorageKind,
    ) -> Result<()> {
        let conflicted = self
            .kubernetes_storage
            .load_model_storages_by(|k| kind == k.to_kind())
            .await?;

        if conflicted.is_empty() {
            Ok(())
        } else {
            bail!(
                "model storage already exists ({name} => {kind}): {list:?}",
                list = conflicted.into_iter().map(|item| item.name_any()).join(","),
            )
        }
    }

    async fn validate_model_storage_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
    ) -> Result<()> {
        DatabaseStorageClient::try_new(storage).await.map(|_| ())
    }

    fn validate_model_storage_kubernetes(
        &self,
        storage: &ModelStorageKubernetesSpec,
    ) -> Result<()> {
        let ModelStorageKubernetesSpec {} = storage;
        Ok(())
    }

    async fn validate_model_storage_object(
        &self,
        name: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<()> {
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            name,
            storage,
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn bind_model(
        &self,
        storage: &ModelStorageCrd,
        model: &ModelCrd,
        sync_policy: Option<ModelStorageBindingSyncPolicy>,
    ) -> Result<ModelStorageBindingSyncPolicy> {
        match &storage.spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                self.bind_model_to_database(spec, model, sync_policy).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => {
                self.bind_model_to_kubernetes(spec, model, sync_policy)
            }
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.bind_model_to_object(spec, &storage.name_any(), model, sync_policy)
                    .await
            }
        }
    }

    async fn bind_model_to_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
        model: &ModelCrd,
        sync_policy: Option<ModelStorageBindingSyncPolicy>,
    ) -> Result<ModelStorageBindingSyncPolicy> {
        let sync_policy = sync_policy.unwrap_or(Self::SYNC_POLICY_DATABASE);
        assert_sync_policy_to_be_never(sync_policy)?;

        DatabaseStorageClient::try_new(storage)
            .await?
            .get_session(model)
            .update_table()
            .await
            .map(|()| sync_policy)
    }

    fn bind_model_to_kubernetes(
        &self,
        storage: &ModelStorageKubernetesSpec,
        model: &ModelCrd,
        sync_policy: Option<ModelStorageBindingSyncPolicy>,
    ) -> Result<ModelStorageBindingSyncPolicy> {
        let sync_policy = sync_policy.unwrap_or(Self::SYNC_POLICY_KUBERNETES);
        assert_sync_policy_to_be_never(sync_policy)?;

        let ModelStorageKubernetesSpec {} = storage;
        match model.spec {
            ModelSpec::CustomResourceDefinitionRef(_) => Ok(sync_policy),
            _ => bail!("kubernetes storage can only used for CRDs"),
        }
    }

    async fn bind_model_to_object(
        &self,
        storage: &ModelStorageObjectSpec,
        storage_name: &str,
        model: &ModelCrd,
        sync_policy: Option<ModelStorageBindingSyncPolicy>,
    ) -> Result<ModelStorageBindingSyncPolicy> {
        let sync_policy = sync_policy.unwrap_or(Self::SYNC_POLICY_OBJECT);

        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            storage_name,
            storage,
        )
        .await?
        .get_session(model, sync_policy)
        .create_bucket()
        .await
        .map(|()| sync_policy)
    }
}

fn assert_sync_policy_to_be_never(sync_policy: ModelStorageBindingSyncPolicy) -> Result<()> {
    if matches!(sync_policy, ModelStorageBindingSyncPolicy::Never) {
        Ok(())
    } else {
        bail!("sync policy should be \"Never\"")
    }
}
