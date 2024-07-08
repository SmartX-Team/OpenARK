use std::{convert::identity, fmt};

use anyhow::{anyhow, Error, Result};
use ark_api::{NamespaceAny, SessionRef};
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Pod, PodCondition};
use kube::{
    api::{AttachParams, AttachedProcess, ListParams},
    Api, Client, ResourceExt,
};
use tokio::{spawn, task::yield_now};
use tracing::{instrument, Level};
use vine_api::user::UserCrd;

#[async_trait]
pub trait SessionExecExt
where
    Self: fmt::Debug + SessionExec,
{
    #[instrument(level = Level::INFO, skip(kube, command), err(Display))]
    async fn exec_with_tty<I>(&self, kube: Client, command: I) -> Result<Vec<Process>>
    where
        I: 'static + Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>,
    {
        let ap = AttachParams::interactive_tty();
        <Self as SessionExec>::exec(self, kube, ap, command).await
    }

    #[instrument(level = Level::INFO, skip(kube, command), err(Display))]
    async fn exec_without_tty<I>(&self, kube: Client, command: I) -> Result<Vec<Process>>
    where
        I: 'static + Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>,
    {
        let ap = AttachParams {
            stdin: false,
            stdout: true,
            stderr: true,
            tty: false,
            ..Default::default()
        };
        <Self as SessionExec>::exec(self, kube, ap, command).await
    }
}

#[async_trait]
impl<T> SessionExecExt for T where Self: fmt::Debug + SessionExec {}

#[async_trait]
pub trait SessionExec {
    async fn list(kube: Client) -> Result<Vec<Self>>
    where
        Self: Sized;

    async fn load<Item>(kube: Client, user_names: &[Item]) -> Result<Vec<Self>>
    where
        Self: Sized,
        Item: Send + Sync + AsRef<str>,
        [Item]: fmt::Debug;

    async fn exec<I>(&self, kube: Client, ap: AttachParams, command: I) -> Result<Vec<Process>>
    where
        I: 'static + Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>;
}

#[async_trait]
impl<'a> SessionExec for SessionRef<'a> {
    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn list(kube: Client) -> Result<Vec<Self>> {
        let api = Api::<UserCrd>::all(kube);
        let lp = ListParams {
            label_selector: Some(format!(
                "{key}=true",
                key = ::ark_api::consts::LABEL_BIND_STATUS,
            )),
            ..Default::default()
        };

        api.list(&lp)
            .await
            .map(collect_user_sessions)
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn load<Item>(kube: Client, user_names: &[Item]) -> Result<Vec<Self>>
    where
        Item: Send + Sync + AsRef<str>,
        [Item]: fmt::Debug,
    {
        let api = Api::<UserCrd>::all(kube);

        user_names
            .iter()
            .map(|user_name| api.get(user_name.as_ref()))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
            .map(|users: Vec<_>| collect_user_sessions(users))
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(kube, ap, command), err(Display))]
    async fn exec<I>(&self, kube: Client, ap: AttachParams, command: I) -> Result<Vec<Process>>
    where
        I: 'static + Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>,
    {
        let api = Api::<Pod>::namespaced(kube, &self.namespace);
        let lp = ListParams {
            label_selector: Some("name=desktop".into()),
            ..Default::default()
        };
        let pods = api.list(&lp).await?.into_iter().filter(|pod| {
            fn check_condition(conditions: &[PodCondition], type_: &str) -> bool {
                conditions
                    .iter()
                    .find(|condition| condition.type_ == type_)
                    .map(|condition| condition.status == "True")
                    .unwrap_or_default()
            }

            pod.status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .map(|conditions| {
                    check_condition(conditions, "PodScheduled") // Running
                        && !check_condition(conditions, "DisruptionTarget") // not Terminating
                })
                .unwrap_or_default()
        });

        pods.map(|pod| {
            let api = api.clone();
            let ap = AttachParams {
                container: Some("desktop-environment".into()),
                ..ap
            };
            let command = command.clone();
            spawn(async move {
                yield_now().await;

                let name = pod.name_any();
                let namespace = pod.namespace();
                match api.exec(&name, command, &ap).await {
                    Ok(ap) => Ok(Process {
                        ap,
                        name,
                        namespace,
                    }),
                    Err(error) => {
                        let namespace = namespace.unwrap_or_else(|| name.clone());
                        Err(anyhow!("failed to execute to {namespace}: {error}"))
                    }
                }
            })
        })
        .collect::<FuturesUnordered<_>>()
        .map(|handle| handle.map_err(Error::from).and_then(identity))
        .try_collect()
        .await
    }
}

pub struct Process {
    pub ap: AttachedProcess,
    pub name: String,
    pub namespace: Option<String>,
}

impl Process {
    pub async fn join(self) -> Result<()> {
        self.ap.join().await.map_err(|error| {
            let namespace = self.namespace.unwrap_or(self.name);
            anyhow!("failed to execute to {namespace}: {error}")
        })
    }
}

fn collect_user_sessions<I>(users: I) -> Vec<SessionRef<'static>>
where
    I: IntoIterator<Item = UserCrd>,
{
    users
        .into_iter()
        .filter_map(|user| {
            user.get_session_ref()
                .map(|session| session.into_owned())
                .ok()
        })
        .collect()
}
