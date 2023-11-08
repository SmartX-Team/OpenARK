use std::{collections::BTreeSet, net::IpAddr};

use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kiss_api::r#box::{BoxCrd, BoxGroupRole, BoxGroupSpec, BoxSpec, BoxState};
use kube::{api::ListParams, Api, Client, Error};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Level};

use crate::config::KissConfig;

pub struct ClusterState<'a> {
    config: &'a KissConfig,
    owner_group: &'a BoxGroupSpec,
    owner_uuid: String,
    control_planes: BTreeSet<ClusterBoxState>,
}

impl<'a> ClusterState<'a> {
    #[instrument(level = Level::INFO, skip(kube, config, owner), err(Display))]
    pub async fn load(
        kube: &'a Client,
        config: &'a KissConfig,
        owner: &'a BoxSpec,
    ) -> Result<ClusterState<'a>, Error> {
        Ok(Self {
            config,
            owner_group: &owner.group,
            owner_uuid: owner.machine.uuid.to_string(),
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

    fn is_node_control_plane(&self) -> bool {
        self.control_planes
            .iter()
            .any(|node| node.name == self.owner_uuid)
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

    pub fn get_control_planes_running_as_vec(&self) -> Vec<&ClusterBoxState> {
        self.control_planes
            .iter()
            .filter(|node| node.is_running || node.name == self.owner_uuid)
            .sorted_by(|a, b| (&a.created_at, a).cmp(&(&b.created_at, b)))
            .collect()
    }

    pub fn get_control_planes_as_string(&self) -> String {
        let nodes = self.get_control_planes_running_as_vec();

        const NODE_ROLE: &str = "kube_control_plane";
        Self::get_nodes_as_string(nodes, NODE_ROLE)
    }

    pub fn get_etcd_nodes_as_string(&self) -> String {
        let mut nodes = self.get_control_planes_running_as_vec();

        // estimate the number of default nodes
        let num_default_nodes = if self.owner_group.is_default() {
            self.config.bootstrapper_node_size
        } else {
            0
        };

        // truncate the number of nodes to `etcd_nodes_max`
        if self.config.etcd_nodes_max > 0 {
            nodes.truncate(if self.config.etcd_nodes_max < num_default_nodes {
                0
            } else {
                self.config.etcd_nodes_max - num_default_nodes
            });
        }

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
            &self.owner_group.cluster_name, control_planes_running, control_planes_total,
        );

        // test the count of nodes
        if control_planes_running == 0 && !self.owner_group.is_default() {
            info!(
                "Cluster \"{}\" status: no control-plane nodes are defined",
                &self.owner_group.cluster_name,
            );
            return false;
        }

        // assert all control plane nodes are ready
        control_planes_running == control_planes_total
    }

    pub fn is_joinable(&self) -> bool {
        if self.is_node_control_plane() {
            self.control_planes
                .iter()
                .find(|node| !node.is_running)
                .map(|node| node.name == self.owner_uuid)
                .unwrap_or_default()
        } else {
            self.is_control_plane_ready()
        }
    }

    pub fn is_new(&self) -> bool {
        self.is_node_control_plane() && self.get_control_planes_running() == 0
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
