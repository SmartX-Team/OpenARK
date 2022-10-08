use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
    ops,
    time::Duration,
};

use ipis::{
    itertools::Itertools,
    tokio::{
        sync::{Mutex, MutexGuard},
        time::sleep,
    },
};
use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::Time, Resource};
use kube::{
    api::{ListParams, Patch, PatchParams, PostParams},
    core::ObjectMeta,
    Api, Client, Error,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::r#box::{BoxCrd, BoxGroupRole, BoxSpec, BoxState};

#[derive(Debug, Default)]
pub struct ClusterManager {
    lock: Mutex<()>,
}

impl ClusterManager {
    pub async fn load_state<'a, 'b>(
        &'a self,
        kube: &'b Client,
        owner: &'b BoxCrd,
    ) -> Result<ClusterStateGuard<'a, 'b>, Error> {
        Ok(ClusterStateGuard {
            _guard: self.lock.lock().await,
            kube,
            owner,
            inner: ClusterStateGuard::load(kube, &owner.spec).await?,
        })
    }
}

pub struct ClusterStateGuard<'a, 'b> {
    _guard: MutexGuard<'a, ()>,
    kube: &'b Client,
    owner: &'b BoxCrd,
    pub inner: ClusterState,
}

impl<'a, 'b> ops::Deref for ClusterStateGuard<'a, 'b> {
    type Target = ClusterState;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, 'b> ops::DerefMut for ClusterStateGuard<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, 'b> ClusterStateGuard<'a, 'b> {
    fn get_name_by_owner(owner: &BoxSpec) -> String {
        format!("cluster-state-{}", &owner.group.cluster_name)
    }

    async fn load(kube: &Client, owner: &BoxSpec) -> Result<ClusterState, Error> {
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let name = Self::get_name_by_owner(owner);

        let config_map = api.get_opt(&name).await?;
        match config_map
            .as_ref()
            .and_then(|config_map| config_map.data.as_ref())
            .and_then(|data| data.get("state"))
            .map(|state| ::serde_json::from_str(state).map_err(Error::SerdeError))
            .transpose()?
        {
            Some(cluster_state) => Ok(cluster_state),
            None => {
                let cluster_state = ClusterState::default();
                let config_map = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        ..Default::default()
                    },
                    data: Some({
                        let mut data = BTreeMap::default();
                        data.insert(
                            "state".into(),
                            ::serde_json::to_string(&cluster_state).map_err(Error::SerdeError)?,
                        );
                        data
                    }),
                    immutable: Some(false),
                    ..Default::default()
                };
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kiss-api".into()),
                };
                api.create(&pp, &config_map).await?;
                Ok(cluster_state)
            }
        }
    }

    async fn patch(&mut self) -> Result<(), Error> {
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(self.kube.clone(), ns);
        let name = Self::get_name_by_owner(&self.owner.spec);

        let patch = Patch::Apply(json!({
            "apiVersion": ConfigMap::API_VERSION,
            "kind": ConfigMap::KIND,
            "data": {
                "state": ::serde_json::to_string(&self.inner).map_err(Error::SerdeError)?,
            },
        }));
        let pp = PatchParams::apply("kiss-api").force();
        api.patch(&name, &pp, &patch).await?;
        Ok(())
    }

    pub async fn lock(&mut self) -> Result<bool, Error> {
        // is it already locked?
        if self.is_locked() {
            return Ok(self.is_locked_by(&self.owner.spec));
        }

        // update lock state
        self.locked_by = Some(ClusterLockState {
            box_name: self.owner.spec.machine.uuid.to_string(),
            role: self.owner.spec.group.role,
        });
        self.patch().await?;

        // synchronize with the others and wait for the result
        sleep(Duration::from_secs(1)).await;

        // is it failed to lock?
        let updated = Self::load(&self.kube, &self.owner.spec).await?;
        Ok(updated.is_locked_by(&self.owner.spec))
    }

    pub async fn release(&mut self) -> Result<bool, Error> {
        // is it not locked?
        if !self.is_locked() {
            return Ok(true);
        }
        if !self.is_locked_by(&self.owner.spec) {
            return Ok(false);
        }

        // update lock state
        self.locked_by = None;
        self.patch().await?;
        Ok(true)
    }

    pub fn get_control_planes_as_string(&self) -> String {
        self.control_planes
            .iter()
            .filter_map(|r#box| r#box.get_host())
            .join(" ")
    }

    pub fn get_etcd_nodes_as_string(&self) -> String {
        self.etcd_nodes
            .iter()
            .filter_map(|r#box| r#box.get_host())
            .join(" ")
    }

    pub async fn is_control_plane_ready(&self) -> Result<bool, Error> {
        // load the current control planes
        let api = Api::<BoxCrd>::all(self.kube.clone());
        let lp = ListParams::default();
        let control_planes: BTreeSet<_> = api
            .list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|r#box| {
                &r#box.spec.group.cluster_name == &self.owner.spec.group.cluster_name
                    && r#box.spec.group.role == BoxGroupRole::ControlPlane
            })
            .map(|r#box| ClusterBoxState {
                created_at: r#box.metadata.creation_timestamp.clone(),
                name: r#box.spec.machine.uuid.to_string(),
                hostname: r#box.spec.machine.hostname(),
                ip: r#box
                    .status
                    .as_ref()
                    .and_then(|status| status.access.as_ref())
                    .map(|access| access.address_primary),
            })
            .collect();

        // assert all control plane nodes are ready
        Ok(&self.control_planes == &control_planes)
    }

    pub async fn update_control_planes(&mut self) -> Result<(), Error> {
        // check lock state
        if !(self.is_locked() && self.is_locked_by(&self.owner.spec)) {
            return Ok(());
        }

        // load control planes
        {
            let api = Api::<BoxCrd>::all(self.kube.clone());
            let lp = ListParams::default();
            self.control_planes = api
                .list(&lp)
                .await?
                .items
                .into_iter()
                .filter(|r#box| {
                    &r#box.spec.group.cluster_name == &self.owner.spec.group.cluster_name
                        && r#box.spec.group.role == BoxGroupRole::ControlPlane
                        && r#box
                            .status
                            .as_ref()
                            .map(|status| status.state == BoxState::Running)
                            .unwrap_or_default()
                })
                .map(|r#box| ClusterBoxState {
                    created_at: r#box.metadata.creation_timestamp.clone(),
                    name: r#box.spec.machine.uuid.to_string(),
                    hostname: r#box.spec.machine.hostname(),
                    ip: r#box
                        .status
                        .as_ref()
                        .and_then(|status| status.access.as_ref())
                        .map(|access| access.address_primary),
                })
                .collect();
            self.etcd_nodes = self
                .control_planes
                .iter()
                // etcd nodes should be odd
                .skip(self.control_planes.len() % 2 + 1)
                .cloned()
                .collect();
        }

        // save to object
        self.patch().await
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ClusterState {
    pub control_planes: BTreeSet<ClusterBoxState>,
    pub etcd_nodes: BTreeSet<ClusterBoxState>,
    pub locked_by: Option<ClusterLockState>,
}

impl ClusterState {
    fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }

    fn is_locked_by(&self, owner: &BoxSpec) -> bool {
        let box_name = owner.machine.uuid.to_string();
        self.locked_by
            .as_ref()
            .map(|lock| &lock.box_name == &box_name && lock.role == owner.group.role)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterBoxState {
    pub created_at: Option<Time>,
    pub name: String,
    pub hostname: String,
    pub ip: Option<IpAddr>,
}

impl ClusterBoxState {
    fn get_host(&self) -> Option<String> {
        Some(format!("{}:{}", &self.hostname, self.ip?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterLockState {
    pub box_name: String,
    pub role: BoxGroupRole,
}
