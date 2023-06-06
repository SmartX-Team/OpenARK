use std::future::Future;

use anyhow::{bail, Result};
use dash_api::job::DashJobCrd;
use dash_provider::{client::FunctionSession, input::InputField, storage::KubernetesStorageClient};
use dash_provider_api::{FunctionChannel, SessionContextMetadata};
use kube::{Client, ResourceExt};
use serde_json::Value;

pub struct DashJobValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> DashJobValidator<'namespace, 'kube> {
    pub async fn create(&self, job: DashJobCrd) -> Result<FunctionChannel> {
        let function_name = job.spec.function.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            match FunctionSession::create(kube.clone(), &metadata, &function_name, inputs.clone())
                .await
            {
                Ok(channel) => Ok(channel),
                Err(error_create) => {
                    match FunctionSession::delete(kube, &metadata, &function_name, inputs).await {
                        Ok(_) => Err(error_create),
                        Err(error_revert) => bail!("{error_create}\n{error_revert}"),
                    }
                }
            }
        })
        .await
    }

    pub async fn is_running(&self, job: DashJobCrd) -> Result<bool> {
        let function_name = job.spec.function.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            FunctionSession::exists(kube, &metadata, &function_name, inputs).await
        })
        .await
    }

    pub async fn delete(&self, job: DashJobCrd) -> Result<FunctionChannel> {
        let function_name = job.spec.function.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            FunctionSession::delete(kube, &metadata, &function_name, inputs).await
        })
        .await
    }

    async fn execute<F, Fut, R>(&self, job: DashJobCrd, f: F) -> Result<R>
    where
        F: FnOnce(Client, SessionContextMetadata, Vec<InputField<Value>>) -> Fut,
        Fut: Future<Output = Result<R>>,
    {
        let kube = self.kubernetes_storage.kube.clone();
        let metadata = SessionContextMetadata {
            name: job.name_any(),
            namespace: job.namespace().unwrap(),
        };
        let inputs = job
            .spec
            .value
            .into_iter()
            .map(|(key, value)| InputField {
                name: format!("/{key}/"),
                value,
            })
            .collect::<Vec<_>>();

        f(kube, metadata, inputs).await
    }
}
