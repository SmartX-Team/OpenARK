use std::{collections::BTreeSet, net::IpAddr, time::Duration};

use ipis::{itertools::Itertools, tokio::time::sleep};
use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::Time, Resource};
use kube::{
    api::{ListParams, Patch, PatchParams, PostParams},
    core::ObjectMeta,
    Api, Client, Error, ResourceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::r#box::{BoxCrd, BoxGroupRole, BoxGroupSpec, BoxSpec};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ClusterState {
    pub control_planes: BTreeSet<ClusterBoxState>,
    pub etcd_nodes: BTreeSet<ClusterBoxState>,
    pub locked_by: Option<ClusterLockState>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterBoxState {
    pub created_at: Option<Time>,
    pub name: String,
    pub hostname: String,
    pub ip: IpAddr,
}

impl ClusterBoxState {
    fn to_string(&self) -> String {
        format!("{}:{}", &self.hostname, &self.ip)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterLockState {
    pub box_name: String,
    pub role: BoxGroupRole,
}

impl ClusterState {
    fn get_name_by_owner(owner: &BoxSpec) -> String {
        format!("cluster-state-{}", &owner.group.cluster_name)
    }

    pub async fn load(kube: &Client, owner: &BoxSpec) -> Result<Self, Error> {
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let name = Self::get_name_by_owner(owner);

        match api.get_opt(&name).await.and_then(|config_map| {
            config_map
                .map(|config_map| {
                    ::serde_json::to_value(config_map.data)
                        .and_then(|data| ::serde_json::from_value(data))
                        .map_err(Error::SerdeError)
                })
                .transpose()
        })? {
            Some(e) => Ok(e),
            None => {
                let data = Self::default();
                let config_map = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        ..Default::default()
                    },
                    data: ::serde_json::to_value(&data)
                        .and_then(|data| ::serde_json::from_value(data))
                        .map_err(Error::SerdeError)?,
                    immutable: Some(false),
                    ..Default::default()
                };
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kiss-api".into()),
                };
                api.create(&pp, &config_map).await?;
                Ok(data)
            }
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }

    pub fn is_locked_by(&self, owner: &BoxSpec) -> bool {
        let box_name = owner.machine.uuid.to_string();
        self.locked_by
            .as_ref()
            .map(|lock| &lock.box_name == &box_name && lock.role == owner.group.role)
            .unwrap_or_default()
    }

    pub async fn lock(&self, kube: &Client, owner: &BoxSpec) -> Result<bool, Error> {
        // is it already locked?
        if self.is_locked() {
            return Ok(self.is_locked_by(owner));
        }

        // update lock state
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let name = Self::get_name_by_owner(owner);

        let patch = Patch::Apply(json!({
            "apiVersion": ConfigMap::API_VERSION,
            "kind": ConfigMap::KIND,
            "data": {
                "lockedBy": owner.group,
            },
        }));
        let pp = PatchParams::apply("kiss-api").force();
        api.patch(&name, &pp, &patch).await?;

        // synchronize with the others and wait for the result
        sleep(Duration::from_secs(1)).await;

        // is it failed to lock?
        let updated = Self::load(kube, owner).await?;
        Ok(updated.is_locked_by(owner))
    }

    pub async fn release(&self, kube: &Client, owner: &BoxSpec) -> Result<(), Error> {
        // is it not locked?
        if !self.is_locked() || !self.is_locked_by(owner) {
            return Ok(());
        }

        // update lock state
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let name = Self::get_name_by_owner(owner);

        let patch = Patch::Apply(json!({
            "apiVersion": ConfigMap::API_VERSION,
            "kind": ConfigMap::KIND,
            "data": {
                "lockedBy": owner.group,
            },
        }));
        let pp = PatchParams::apply("kiss-api").force();
        api.patch(&name, &pp, &patch).await?;
        Ok(())
    }

    pub fn get_control_planes_as_string(&self) -> String {
        self.control_planes
            .iter()
            .map(|r#box| r#box.to_string())
            .join(" ")
    }

    pub fn get_etcd_nodes_as_string(&self) -> String {
        self.etcd_nodes
            .iter()
            .map(|r#box| r#box.to_string())
            .join(" ")
    }

    pub async fn update_control_planes(
        &mut self,
        kube: &Client,
        owner: &BoxCrd,
    ) -> Result<(), Error> {
        // check box and cluster state
        if !(owner
            .status
            .as_ref()
            .and_then(|status| status.bind_group.as_ref())
            .map(|bind_group| bind_group.role == BoxGroupRole::ControlPlane)
            .unwrap_or_default()
            && self.is_locked()
            && self.is_locked_by(&owner.spec))
        {
            return Ok(());
        }

        // load control planes
        {
            let api = Api::<BoxCrd>::all(kube.clone());

            let fields = &[
                format!("spec.group.cluster_name={}", &owner.spec.group.cluster_name,),
                format!("spec.group.role=ControlPlane"),
                format!(
                    "status.bind_group.cluster_name={}",
                    &owner.spec.group.cluster_name,
                ),
                format!("status.bind_group.role=ControlPlane"),
                format!("status.state=Running"),
            ];
            let lp = ListParams::default().fields(&fields.join(","));
            self.control_planes = api
                .list(&lp)
                .await?
                .items
                .into_iter()
                .map(|r#box| ClusterBoxState {
                    created_at: r#box.metadata.creation_timestamp.clone(),
                    name: r#box.name_any(),
                    hostname: r#box.spec.machine.hostname(),
                    ip: r#box.spec.access.address_primary,
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
        {
            let ns = crate::consts::NAMESPACE;
            let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
            let name = Self::get_name_by_owner(&owner.spec);

            let patch = Patch::Apply(json!({
                "apiVersion": ConfigMap::API_VERSION,
                "kind": ConfigMap::KIND,
                "data": {
                    "controlPlanes": &self.control_planes,
                },
            }));
            let pp = PatchParams::apply("kiss-api").force();
            api.patch(&name, &pp, &patch).await?;
        }
        Ok(())
    }
}
