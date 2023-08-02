use std::borrow::Cow;

pub mod package;

pub mod consts {
    pub const NAMESPACE: &str = "ark";

    pub const LABEL_BIND_BY_USER: &str = "ark.ulagbulag.io/bind.user";
    pub const LABEL_BIND_NAMESPACE: &str = "ark.ulagbulag.io/bind.namespace";
    pub const LABEL_BIND_NODE: &str = "ark.ulagbulag.io/bind.node";
    pub const LABEL_BIND_STATUS: &str = "ark.ulagbulag.io/bind";
    pub const LABEL_BIND_PERSISTENT: &str = "ark.ulagbulag.io/bind.persistent";
    pub const LABEL_BIND_TIMESTAMP: &str = "ark.ulagbulag.io/bind.timestamp";

    pub const HEADER_NAMESPACE: &str = "X-ARK-NAMESPACE";
}

pub trait NamespaceAny {
    fn namespace_any(&self) -> String;

    fn get_session_ref(&self) -> ::anyhow::Result<SessionRef>;
}

impl<T> NamespaceAny for T
where
    T: ::kube::ResourceExt,
{
    fn namespace_any(&self) -> String {
        self.namespace().unwrap_or_else(|| "default".into())
    }

    fn get_session_ref(&self) -> ::anyhow::Result<SessionRef> {
        let name = self.name_any();

        let labels = self.labels();
        if labels
            .get(self::consts::LABEL_BIND_STATUS)
            .map(AsRef::as_ref)
            != Some("true")
        {
            ::anyhow::bail!("session is not binded: {name:?}")
        }

        let duration_session_start = ::chrono::Duration::seconds(5);
        match labels
            .get(self::consts::LABEL_BIND_TIMESTAMP)
            .and_then(|timestamp| {
                let timestamp: i64 = timestamp.parse().ok()?;
                let naive_date_time = ::chrono::NaiveDateTime::from_timestamp_millis(timestamp)?;
                Some(::chrono::DateTime::<::chrono::Utc>::from_utc(
                    naive_date_time,
                    ::chrono::Utc,
                ))
            }) {
            Some(timestamp) if ::chrono::Utc::now() - timestamp >= duration_session_start => {}
            Some(_) => {
                ::anyhow::bail!(
                    "session is in starting (timeout: {duration_session_start}): {name:?}"
                )
            }
            None => {
                ::anyhow::bail!("session timestamp is missing: {name:?}")
            }
        }

        let namespace = match labels.get(self::consts::LABEL_BIND_NAMESPACE) {
            Some(namespace) => namespace.into(),
            None => {
                ::anyhow::bail!("session namespace is missing: {name:?}")
            }
        };

        let node_name = match labels.get(self::consts::LABEL_BIND_NODE) {
            Some(node_name) => node_name.into(),
            None => {
                ::anyhow::bail!("session nodename is missing: {name:?}")
            }
        };

        let user_name = match labels.get(self::consts::LABEL_BIND_BY_USER) {
            Some(user_name) => user_name.into(),
            None => {
                ::anyhow::bail!("session username is missing: {name:?}")
            }
        };

        Ok(SessionRef {
            namespace,
            node_name,
            user_name,
        })
    }
}

pub struct SessionRef<'a> {
    pub namespace: Cow<'a, str>,
    pub node_name: Cow<'a, str>,
    pub user_name: Cow<'a, str>,
}

impl<'a> SessionRef<'a> {
    pub fn into_owned(self) -> SessionRef<'static> {
        let Self {
            namespace,
            node_name,
            user_name,
        } = self;
        SessionRef {
            namespace: namespace.into_owned().into(),
            node_name: node_name.into_owned().into(),
            user_name: user_name.into_owned().into(),
        }
    }
}
