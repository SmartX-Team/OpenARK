use std::{collections::BTreeSet, net::IpAddr};

use ipis::{itertools::Itertools, log::info};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::{api::ListParams, Api, Client, Error};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::r#box::{BoxCrd, BoxGroupRole, BoxSpec, BoxState};

pub struct ClusterState<'a> {
    owner: &'a BoxSpec,
    control_planes: BTreeSet<ClusterBoxState>,
}

impl<'a> ClusterState<'a> {
    pub async fn load(kube: &'a Client, owner: &'a BoxSpec) -> Result<ClusterState<'a>, Error> {
        Ok(Self {
            owner,
            // load the current control planes
            control_planes: {
                let api = Api::<BoxCrd>::all(kube.clone());
                let lp = ListParams::default();
                api.list(&lp)
                    .await?
                    .items
                    .into_iter()
                    .filter(|r#box| {
                        r#box.spec.group.cluster_name == owner.group.cluster_name
                            && r#box.spec.group.role == BoxGroupRole::ControlPlane
                    })
                    .map(|r#box| ClusterBoxState {
                        created_at: r#box.metadata.creation_timestamp.clone(),
                        name: r#box.spec.machine.uuid.to_string(),
                        hostname: r#box.spec.machine.hostname(),
                        ip: r#box
                            .status
                            .as_ref()
                            .and_then(|status| status.access.primary.as_ref())
                            .map(|interface| interface.address),
                        is_running: r#box
                            .status
                            .as_ref()
                            .map(|status| matches!(status.state, BoxState::Running))
                            .unwrap_or_default(),
                    })
                    .collect()
            },
        })
    }

    fn get_control_plane_index(&self) -> Option<usize> {
        self.control_planes
            .iter()
            .enumerate()
            .filter(|(_, control_plane)| control_plane.name == self.owner.machine.uuid.to_string())
            .map(|(index, _)| index)
            .next()
    }

    fn get_control_planes_running(&self) -> usize {
        self.control_planes
            .iter()
            .filter(|node| node.is_running)
            .count()
    }

    fn get_control_planes_total(&self) -> usize {
        self.control_planes.len()
    }

    pub fn get_control_planes_as_string(&self) -> String {
        let nodes = self.control_planes.iter().take(
            self.get_control_plane_index()
                .map(|index| index + 1)
                .unwrap_or_else(|| self.get_control_planes_total()),
        );

        const NODE_ROLE: &str = "kube_control_plane";
        Self::get_nodes_as_string(nodes, NODE_ROLE)
    }

    pub fn get_etcd_nodes_as_string(&self) -> String {
        let mut nodes: Vec<_> = self
            .control_planes
            .iter()
            .take(
                self.get_control_plane_index()
                    .map(|index| index + 1)
                    .unwrap_or_else(|| self.get_control_planes_total()),
            )
            .collect();

        // estimate the number of default nodes
        let num_default_nodes = usize::from(self.owner.group.is_default());

        // ETCD nodes should be odd (RAFT)
        if (nodes.len() + num_default_nodes) % 2 == 0 {
            nodes.pop();
        }

        const NODE_ROLE: &str = "etcd";
        Self::get_nodes_as_string(nodes, NODE_ROLE)
    }

    fn get_nodes_as_string<I, Item>(nodes: I, node_role: &str) -> String
    where
        I: IntoIterator<Item = Item>,
        Item: AsRef<ClusterBoxState>,
    {
        nodes
            .into_iter()
            .filter_map(|r#box| r#box.as_ref().get_host())
            .map(|host| format!("{node_role}:{host}"))
            .join(" ")
    }

    pub fn is_control_plane_ready(&self) -> bool {
        let control_planes_running = self.get_control_planes_running();
        let control_planes_total = self.get_control_planes_total();

        info!(
            "Cluster \"{}\" status: {}/{} control-plane nodes are ready",
            &self.owner.group.cluster_name, control_planes_running, control_planes_total,
        );

        // test the count of nodes
        if control_planes_running == 0 && !self.owner.group.is_default() {
            info!(
                "Cluster \"{}\" status: no control-plane nodes are defined",
                &self.owner.group.cluster_name,
            );
            return false;
        }

        // assert all control plane nodes are ready
        control_planes_running == control_planes_total
    }

    pub fn is_joinable(&self) -> bool {
        match self.get_control_plane_index() {
            Some(index) => index == self.get_control_planes_running(),
            None => self.is_control_plane_ready(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterBoxState {
    pub created_at: Option<Time>,
    pub name: String,
    pub hostname: String,
    pub ip: Option<IpAddr>,
    pub is_running: bool,
}

impl AsRef<Self> for ClusterBoxState {
    fn as_ref(&self) -> &Self {
        self
    }
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
