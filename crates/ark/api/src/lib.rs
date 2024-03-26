use std::borrow::Cow;

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, TimeDelta, Utc};
use kube::ResourceExt;
use serde::{Deserialize, Serialize};

pub mod consts {
    pub const NAMESPACE: &str = "ark";

    pub const LABEL_BIND_BY_USER: &str = "ark.ulagbulag.io/bind.user";
    pub const LABEL_BIND_NAMESPACE: &str = "ark.ulagbulag.io/bind.namespace";
    pub const LABEL_BIND_NODE: &str = "ark.ulagbulag.io/bind.node";
    pub const LABEL_BIND_PERSISTENT: &str = "ark.ulagbulag.io/bind.persistent";
    pub const LABEL_BIND_STATUS: &str = "ark.ulagbulag.io/bind";
    pub const LABEL_BIND_TIMESTAMP: &str = "ark.ulagbulag.io/bind.timestamp";

    pub const HEADER_NAMESPACE: &str = "X-ARK-NAMESPACE";
}

pub trait NamespaceAny {
    fn namespace_any(&self) -> String;

    fn get_session_ref(&self) -> Result<SessionRef>;
}

impl<T> NamespaceAny for T
where
    T: ResourceExt,
{
    fn namespace_any(&self) -> String {
        self.namespace().unwrap_or_else(|| "default".into())
    }

    fn get_session_ref(&self) -> Result<SessionRef> {
        let name = self.name_any();

        let labels = self.labels();
        if labels
            .get(self::consts::LABEL_BIND_STATUS)
            .map(AsRef::as_ref)
            != Some("true")
        {
            bail!("session is not binded: {name:?}")
        }

        let namespace = match labels.get(self::consts::LABEL_BIND_NAMESPACE) {
            Some(namespace) => namespace.into(),
            None => {
                bail!("session namespace is missing: {name:?}")
            }
        };

        let node_name = match labels.get(self::consts::LABEL_BIND_NODE) {
            Some(node_name) => node_name.into(),
            None => {
                bail!("session nodename is missing: {name:?}")
            }
        };

        let timestamp = labels
            .get(self::consts::LABEL_BIND_TIMESTAMP)
            .and_then(|timestamp| {
                let timestamp: i64 = timestamp.parse().ok()?;
                DateTime::<Utc>::from_timestamp_millis(timestamp)
            });

        let user_name = match labels.get(self::consts::LABEL_BIND_BY_USER) {
            Some(user_name) => user_name.into(),
            None => {
                bail!("session username is missing: {name:?}")
            }
        };

        Ok(SessionRef {
            namespace,
            node_name,
            timestamp,
            user_name,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef<'a> {
    pub namespace: Cow<'a, str>,
    pub node_name: Cow<'a, str>,
    pub timestamp: Option<DateTime<Utc>>,
    pub user_name: Cow<'a, str>,
}

impl<'a> SessionRef<'a> {
    pub fn assert_started(&self) -> Result<()> {
        match self.is_started() {
            Some(true) => Ok(()),
            Some(false) => {
                let name = &self.node_name;
                let timeout = duration_session_start();
                bail!("session is in starting (timeout: {timeout}): {name:?}")
            }
            None => {
                let name = &self.node_name;
                bail!("session namespace is missing: {name:?}")
            }
        }
    }

    pub fn is_started(&self) -> Option<bool> {
        Some(Utc::now() >= self.timestamp? + duration_session_start())
    }

    pub fn into_owned(self) -> SessionRef<'static> {
        let Self {
            namespace,
            node_name,
            timestamp,
            user_name,
        } = self;
        SessionRef {
            namespace: namespace.into_owned().into(),
            node_name: node_name.into_owned().into(),
            timestamp,
            user_name: user_name.into_owned().into(),
        }
    }
}

fn duration_session_start() -> TimeDelta {
    Duration::try_seconds(5).unwrap()
}
