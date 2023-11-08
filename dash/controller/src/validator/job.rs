use std::future::Future;

use anyhow::{bail, Result};
use dash_api::job::DashJobCrd;
use dash_provider::{client::TaskSession, input::InputField, storage::KubernetesStorageClient};
use dash_provider_api::{SessionContextMetadata, TaskChannel};
use kube::{Client, ResourceExt};
use serde_json::Value;
use tracing::{instrument, Level};

pub struct DashJobValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> DashJobValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, fields(job.name = %job.name_any(), job.namespace = job.namespace()), err(Display))]
    pub async fn create(&self, job: DashJobCrd) -> Result<TaskChannel> {
        let task_name = job.spec.task.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            match TaskSession::create(kube.clone(), &metadata, &task_name, inputs.clone()).await {
                Ok(channel) => Ok(channel),
                Err(error_create) => {
                    match TaskSession::delete(kube, &metadata, &task_name, inputs).await {
                        Ok(_) => Err(error_create),
                        Err(error_revert) => bail!("{error_create}\n{error_revert}"),
                    }
                }
            }
        })
        .await
    }

    #[instrument(level = Level::INFO, skip_all, fields(job.name = %job.name_any(), job.namespace = job.namespace()), err(Display))]
    pub async fn is_running(&self, job: DashJobCrd) -> Result<bool> {
        let task_name = job.spec.task.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            TaskSession::exists(kube, &metadata, &task_name, inputs).await
        })
        .await
    }

    #[instrument(level = Level::INFO, skip_all, fields(job.name = %job.name_any(), job.namespace = job.namespace()), err(Display))]
    pub async fn delete(&self, job: DashJobCrd) -> Result<TaskChannel> {
        let task_name = job.spec.task.clone();

        self.execute(job, |kube, metadata, inputs| async move {
            TaskSession::delete(kube, &metadata, &task_name, inputs).await
        })
        .await
    }

    #[instrument(level = Level::INFO, skip_all, fields(job.name = %job.name_any(), job.namespace = job.namespace()), err(Display))]
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
