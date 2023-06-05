use anyhow::{bail, Result};
use dash_api::job::DashJobCrd;
use dash_provider::{client::FunctionSession, input::InputField, storage::KubernetesStorageClient};
use dash_provider_api::SessionContextMetadata;
use kube::ResourceExt;

pub struct DashJobValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> DashJobValidator<'namespace, 'kube> {
    pub async fn create(&self, job: DashJobCrd) -> Result<()> {
        let kube = self.kubernetes_storage.kube.clone();
        let metadata = SessionContextMetadata {
            name: job.name_any(),
            namespace: job.namespace().unwrap(),
        };
        let inputs = vec![InputField {
            name: "/".to_string(),
            value: job.spec.value,
        }];

        match FunctionSession::create(kube.clone(), &metadata, inputs.clone()).await {
            Ok(_) => Ok(()),
            Err(error_create) => match FunctionSession::delete(kube, &metadata, inputs).await {
                Ok(_) => Err(error_create),
                Err(error_revert) => bail!("{error_create}\n{error_revert}"),
            },
        }
    }

    pub async fn is_running(&self, job: DashJobCrd) -> Result<bool> {
        let kube = self.kubernetes_storage.kube.clone();
        let metadata = SessionContextMetadata {
            name: job.name_any(),
            namespace: job.namespace().unwrap(),
        };
        let inputs = vec![InputField {
            name: "/".to_string(),
            value: job.spec.value,
        }];

        FunctionSession::exists(kube, &metadata, inputs).await
    }

    pub async fn delete(&self, job: DashJobCrd) -> Result<()> {
        let kube = self.kubernetes_storage.kube.clone();
        let metadata = SessionContextMetadata {
            name: job.name_any(),
            namespace: job.namespace().unwrap(),
        };
        let inputs = vec![InputField {
            name: "/".to_string(),
            value: job.spec.value,
        }];

        FunctionSession::delete(kube, &metadata, inputs)
            .await
            .map(|_| ())
    }
}
