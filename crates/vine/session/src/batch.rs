use std::fmt;

use anyhow::{anyhow, Error, Result};
use ark_api::{NamespaceAny, SessionRef};
use futures::{stream::FuturesUnordered, StreamExt};
use k8s_openapi::api::core::v1::Node;
use kube::{api::ListParams, Api, Client, ResourceExt};
use regex::Regex;
use tokio::spawn;
use tracing::{debug, error, instrument, warn, Level};

use crate::exec::SessionExecExt;

pub struct BatchCommandArgs<C, U> {
    pub command: C,
    pub terminal: bool,
    pub users: BatchCommandUsers<U>,
    pub wait: bool,
}

impl<C, U> BatchCommandArgs<C, U> {
    pub async fn exec(&self, kube: &Client) -> Result<usize>
    where
        C: 'static + Send + Sync + Clone + fmt::Debug + IntoIterator,
        <C as IntoIterator>::Item: Sync + Into<String>,
        U: AsRef<str>,
    {
        let Self {
            command: command_args,
            terminal,
            users,
            wait,
        } = self;

        let mut command: Vec<String> = Vec::default();
        {
            if *terminal {
                command.push("xfce4-terminal".into());
                command.push("--disable-server".into());
                command.push("-x".into());
            }
            command.push("/usr/bin/env".into());
            command.push("sh".into());
            command.push("-c".into());
            command.extend(command_args.clone().into_iter().map(Into::into));
        }

        let sessions_all = collect_user_sessions(kube).await?;
        let sessions_filtered = users.filter(sessions_all)?;
        let num_sessions = sessions_filtered.len();

        let processes = sessions_filtered.into_iter().map(|session| {
            let kube = kube.clone();
            let command = command.clone();
            spawn(async move { session.exec_without_tty(kube, command).await })
        });

        processes
            .collect::<FuturesUnordered<_>>()
            .then(|result| async move {
                match result
                    .map_err(Error::from)
                    .and_then(|result| result.map_err(Error::from))
                {
                    Ok(processes) => {
                        if *wait {
                            processes
                                .into_iter()
                                .map(|process| async move {
                                    match process.join().await {
                                        Ok(()) => (),
                                        Err(error) => {
                                            error!("{error}");
                                        }
                                    }
                                })
                                .collect::<FuturesUnordered<_>>()
                                .collect::<()>()
                                .await;
                        }
                    }
                    Err(error) => {
                        warn!("failed to command: {error}");
                    }
                }
            })
            .collect::<()>()
            .await;
        Ok(num_sessions)
    }
}

pub enum BatchCommandUsers<U> {
    All,
    List(Vec<U>),
    Pattern(U),
}

impl<U> BatchCommandUsers<U>
where
    U: AsRef<str>,
{
    pub(crate) fn filter(
        &self,
        sessions_all: impl Iterator<Item = SessionRef<'static>>,
    ) -> Result<Vec<SessionRef<'static>>> {
        match self {
            Self::All => Ok(sessions_all.collect()),
            Self::List(items) => Ok(sessions_all
                .filter(|session| items.iter().any(|item| item.as_ref() == session.user_name))
                .collect()),
            Self::Pattern(re) => {
                let re = Regex::new(re.as_ref())
                    .map_err(|error| anyhow!("failed to parse box regex pattern: {error}"))?;

                Ok(sessions_all
                    .filter(|session| re.is_match(&session.user_name))
                    .collect())
            }
        }
    }
}

#[instrument(level = Level::INFO, skip(kube), err(Display))]
pub(crate) async fn collect_user_sessions(
    kube: &Client,
) -> Result<impl Iterator<Item = SessionRef<'static>>> {
    let api = Api::<Node>::all(kube.clone());
    let lp = ListParams::default();
    api.list_metadata(&lp)
        .await
        .map(|list| {
            list.items
                .into_iter()
                .filter_map(|item| match item.get_session_ref() {
                    Ok(session) => Some(session.into_owned()),
                    Err(error) => {
                        let name = item.name_any();
                        debug!("failed to get session {name}: {error}");
                        None
                    }
                })
        })
        .map_err(|error| anyhow!("failed to list nodes: {error}"))
}
