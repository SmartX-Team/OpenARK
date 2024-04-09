use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use k8s_openapi::{
    api::core::v1::Node,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition, chrono::Utc,
    serde_json::json, Resource,
};
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use tracing::{info, instrument, warn, Level};
use vine_api::{
    user_auth::UserSessionResponse,
    user_box_binding::{UserBoxBindingCrd, UserBoxBindingSpec},
};
use vine_session::{is_persistent, is_persistent_by};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = UserBoxBindingCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::vine_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        vec![
            ::vine_api::user_box_quota::UserBoxQuotaCrd::crd(),
            ::vine_api::user_box_quota_binding::UserBoxQuotaBindingCrd::crd(),
        ]
    }

    #[instrument(level = Level::INFO, skip_all, fields(name = %data.name_any(), namespace = data.namespace()), err(Display))]
    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let UserBoxBindingSpec {
            user: user_name,
            r#box: node_name,
            autologin,
            expired_timestamp,
        } = &data.spec;

        let now = Utc::now();
        let is_expired = expired_timestamp
            .map(|expired_timestamp| now < expired_timestamp)
            .unwrap_or_default();

        if *autologin && !is_expired {
            enable_autologin(&manager.kube, node_name, user_name).await?;
        } else {
            disable_autologin(&manager.kube, node_name, user_name).await?;
        }

        match expired_timestamp {
            Some(expired_timestamp) if !is_expired => {
                let remained = *expired_timestamp - now;
                Ok(Action::requeue(
                    remained
                        .to_std()
                        .map(|remained| remained + <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK)
                        .unwrap_or(<Self as ::ark_core_k8s::manager::Ctx>::FALLBACK),
                ))
            }
            Some(_) => Ok(Action::await_change()),
            None => Ok(Action::requeue(
                <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
            )),
        }
    }
}

async fn enable_autologin(kube: &Client, node_name: &str, user_name: &str) -> Result<(), Error> {
    let node = match get_node(kube, node_name).await? {
        Some(node) => node,
        None => {
            warn!("skipping enabling autologin {node_name:?} => {user_name:?}: no such node");
            return Ok(());
        }
    };

    if is_persistent(&node) && !is_persistent_by(&node, user_name) {
        info!("skipping enabling autologin {node_name:?} => {user_name:?}: node is already persistent");
        return Ok(());
    }

    update_node_autologin(kube, node_name, Some(user_name)).await?;

    const LOGOUT_ON_FAILED: bool = false;
    match ::vine_rbac::login::execute(kube, node_name, user_name, LOGOUT_ON_FAILED).await {
        Ok(UserSessionResponse::Accept { .. }) => {
            info!("binded node: {node_name:?} => {user_name:?}");
            Ok(())
        }
        Ok(UserSessionResponse::Error(e)) => {
            warn!("failed to bind node: {node_name:?} => {user_name:?}: {e}");
            Ok(())
        }
        Err(e) => {
            warn!("failed to bind node: {node_name:?} => {user_name:?}: {e}");
            Ok(())
        }
    }
}

async fn disable_autologin(kube: &Client, node_name: &str, user_name: &str) -> Result<(), Error> {
    let node = match get_node(kube, node_name).await? {
        Some(node) => node,
        None => {
            warn!("skipping disabling autologin {node_name:?} => {user_name:?}: no such node");
            return Ok(());
        }
    };

    if !is_persistent_by(&node, user_name) {
        info!("skipping disabling autologin {node_name:?} => {user_name:?}: node is locked by other user");
        return Ok(());
    }

    update_node_autologin(kube, node_name, None).await?;

    match ::vine_rbac::logout::execute(kube, node_name, user_name).await {
        Ok(UserSessionResponse::Accept { .. }) => {
            info!("unbinded node: {node_name:?} => {user_name:?}");
            Ok(())
        }
        Ok(UserSessionResponse::Error(e)) => {
            warn!("failed to unbind node: {node_name:?} => {user_name:?}: {e}");
            Ok(())
        }
        Err(e) => {
            warn!("failed to unbind node: {node_name:?} => {user_name:?}: {e}");
            Ok(())
        }
    }
}

async fn get_node(kube: &Client, name: &str) -> Result<Option<Node>, Error> {
    let api = Api::<Node>::all(kube.clone());
    api.get_opt(name).await
}

async fn update_node_autologin(
    kube: &Client,
    node_name: &str,
    user_name: Option<&str>,
) -> Result<(), Error> {
    let api = Api::<Node>::all(kube.clone());
    let pp = PatchParams::apply(<Ctx as ::ark_core_k8s::manager::Ctx>::NAME).force();

    let persistent = user_name.is_some().to_string();
    let patch = Patch::Apply(json!({
        "apiVersion": Node::API_VERSION,
        "kind": Node::KIND,
        "metadata": {
            "labels": {
                ::ark_api::consts::LABEL_BIND_BY_USER: user_name,
                ::ark_api::consts::LABEL_BIND_PERSISTENT: &persistent,
            },
        },
    }));
    api.patch(node_name, &pp, &patch).await?;

    let user_name = user_name.unwrap_or_default();
    info!("updated autologin mode {node_name:?} => {user_name:?}");
    Ok(())
}
