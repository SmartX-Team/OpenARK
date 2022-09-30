use std::time::Duration;

use ipis::tokio::time::sleep;
use k8s_openapi::{api::core::v1::ConfigMap, Resource};
use kube::{
    api::{Patch, PatchParams},
    Api, Client, Error,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::r#box::{BoxGroupRole, BoxSpec};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ClusterState {
    pub control_planes: Vec<String>,
    pub locked_by: Option<ClusterLockState>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
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
        api.get(&name).await.and_then(|config_map| {
            ::serde_json::to_value(config_map.data)
                .and_then(|data| ::serde_json::from_value(data))
                .map_err(Error::SerdeError)
        })
    }

    pub fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }

    fn is_locked_by(&self, owner: &BoxSpec) -> bool {
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
}
