use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use dash_api::{
    job::{DashJobCrd, DashJobSpec},
    task::TaskCrd,
};
use dash_provider_api::{
    job::{TaskActorJobMetadata, TaskChannelKindJob},
    TaskChannelKind,
};
use futures::{AsyncBufReadExt, Stream, TryStreamExt};
use itertools::Itertools;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{DeleteParams, ListParams, LogParams, PostParams},
    core::ObjectMeta,
    Api, Client, ResourceExt,
};
use serde_json::Value;
use tracing::{instrument, Level};
use vine_api::user_session::UserSession;

pub(crate) const NAME: &str = "dash-provider-client";

pub struct DashProviderClient<'a> {
    api: Api<DashJobCrd>,
    client: Client,
    session: &'a UserSession,
}

impl<'a> DashProviderClient<'a> {
    pub fn new(client: Client, session: &'a UserSession) -> Self {
        Self {
            api: Api::namespaced(client.clone(), &session.namespace),
            client,
            session,
        }
    }

    #[cfg(feature = "dash-provider")]
    #[instrument(level = Level::INFO, skip(self, value), err(Display))]
    pub async fn create(
        &self,
        task_name: &str,
        value: BTreeMap<String, Value>,
    ) -> Result<DashJobCrd> {
        let storage = ::dash_provider::storage::KubernetesStorageClient {
            namespace: &self.session.namespace,
            kube: &self.client,
        };
        let task = storage.load_task(task_name).await?;
        self.create_raw(&task, value).await
    }

    #[instrument(level = Level::INFO, skip_all, fields(task_name = %task.name_any()), err(Display))]
    pub async fn create_raw(
        &self,
        task: &TaskCrd,
        value: BTreeMap<String, Value>,
    ) -> Result<DashJobCrd> {
        let task_name = task.name_any();
        let job_name = format!(
            "{name}-{uuid}",
            name = task_name,
            uuid = ::uuid::Uuid::new_v4(),
        );
        let data = DashJobCrd {
            metadata: ObjectMeta {
                name: Some(job_name.clone()),
                namespace: Some(self.session.namespace.clone()),
                finalizers: Some(vec![DashJobCrd::FINALIZER_NAME.into()]),
                labels: Some(
                    [
                        (DashJobCrd::LABEL_TARGET_TASK, task_name.clone()),
                        (
                            DashJobCrd::LABEL_TARGET_TASK_NAMESPACE,
                            task.namespace().unwrap(),
                        ),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
                ),
                ..Default::default()
            },
            spec: DashJobSpec {
                value,
                task: task_name.clone(),
            },
            status: None,
        };

        let pp = PostParams {
            dry_run: false,
            field_manager: Some(self::NAME.into()),
        };
        self.api
            .create(&pp, &data)
            .await
            .map_err(|error| anyhow!("failed to create job ({task_name} => {job_name}): {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn delete(&self, task_name: &str, job_name: &str) -> Result<()> {
        match self.get(task_name, job_name).await? {
            Some(_) => self.force_delete(task_name, job_name).await,
            None => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn force_delete(&self, task_name: &str, job_name: &str) -> Result<()> {
        let dp = DeleteParams::default();
        self.api
            .delete(job_name, &dp)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to delete job ({task_name} => {job_name}): {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get(&self, task_name: &str, job_name: &str) -> Result<Option<DashJobCrd>> {
        self.api
            .get_opt(job_name)
            .await
            .map_err(|error| anyhow!("failed to find job ({task_name} => {job_name}): {error}"))
            .and_then(|result| match result {
                Some(job) if job.spec.task == task_name => Ok(Some(job)),
                Some(job) => bail!(
                    "unexpected job: expected task name {expected:?}, but given {given:?}",
                    expected = job_name,
                    given = job.spec.task,
                ),
                None => Ok(None),
            })
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_list(&self) -> Result<Vec<DashJobCrd>> {
        let lp = ListParams::default();
        self.api
            .list(&lp)
            .await
            .map(|list| list.items)
            .map_err(|error| anyhow!("failed to list jobs: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_list_with_task_name(&self, task_name: &str) -> Result<Vec<DashJobCrd>> {
        let lp = ListParams {
            label_selector: Some(format!(
                "{key}={value}",
                key = DashJobCrd::LABEL_TARGET_TASK,
                value = task_name,
            )),
            ..Default::default()
        };
        self.api
            .list(&lp)
            .await
            .map(|list| list.items)
            .map_err(|error| anyhow!("failed to list jobs ({task_name}): {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_stream_logs(
        &self,
        task_name: &str,
        job_name: &str,
    ) -> Result<impl Stream<Item = Result<String, ::std::io::Error>>> {
        match self.get(task_name, job_name).await? {
            Some(job) => {
                match job
                    .status
                    .and_then(|status| status.channel)
                    .map(|channel| channel.actor)
                {
                    Some(TaskChannelKind::Job(TaskChannelKindJob {
                        metadata:
                            TaskActorJobMetadata {
                                container,
                                label_selector,
                            },
                        ..
                    })) => {
                        let api =
                            Api::<Pod>::namespaced(self.client.clone(), &self.session.namespace);

                        let lp = ListParams {
                            label_selector: label_selector.match_labels.map(|match_labels| {
                                match_labels
                                    .into_iter()
                                    .map(|(key, value)| format!("{key}={value}"))
                                    .join(",")
                            }),
                            ..Default::default()
                        };
                        let pod_name = match api.list(&lp).await {
                            Ok(list) if !list.items.is_empty() => list.items[0].name_any(),
                            Ok(_) => {
                                bail!("no such jod's pod: {task_name:?} => {job_name:?}")
                            }
                            Err(error) => bail!(
                                "failed to find job's pod ({task_name} => {job_name}): {error}"
                            ),
                        };

                        let lp = LogParams {
                            container: container.clone(),
                            follow: true,
                            pretty: true,
                            ..Default::default()
                        };
                        api.log_stream(&pod_name, &lp)
                            .await
                            .map(|stream| stream.lines())
                            .map_err(|error| {
                                anyhow!(
                                    "failed to get job logs ({task_name} => {job_name}): {error}"
                                )
                            })
                    }
                    None => {
                        bail!("only the K8S job can be watched: {task_name:?} => {job_name:?}")
                    }
                }
            }
            None => bail!("no such job: {task_name:?} => {job_name:?}"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_stream_logs_as_bytes(
        &self,
        task_name: &str,
        job_name: &str,
    ) -> Result<impl Stream<Item = Result<Bytes, ::std::io::Error>>> {
        self.get_stream_logs(task_name, job_name)
            .await
            .map(|stream| stream.map_ok(|line| line.into()))
    }

    #[cfg(feature = "dash-provider")]
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn restart(&self, task_name: &str, job_name: &str) -> Result<DashJobCrd> {
        match self.get(task_name, job_name).await? {
            Some(job) => {
                self.force_delete(task_name, job_name).await?;
                self.create(task_name, job.spec.value).await
            }
            None => bail!("no such job: {task_name:?} => {job_name:?}"),
        }
    }
}
