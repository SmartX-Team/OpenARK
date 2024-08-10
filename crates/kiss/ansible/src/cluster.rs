use std::{borrow::Cow, collections::BTreeMap, net::IpAddr};

use anyhow::{anyhow, bail, Result};
use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kiss_api::r#box::{BoxCrd, BoxGroupRole, BoxGroupSpec, BoxSpec, BoxState};
use kube::{api::ListParams, Api, Client, Error};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Level};
use uuid::Uuid;

use crate::config::KissConfig;

pub struct ClusterState<'a> {
    config: Cow<'a, KissConfig>,
    control_planes: ClusterBoxGroup,
    owner_group: Cow<'a, BoxGroupSpec>,
    owner_uuid: Uuid,
    workers: Option<ClusterBoxGroup>,
}

impl<'a> ClusterState<'a> {
    #[instrument(level = Level::INFO, skip(kube, config, owner), err(Display))]
    pub async fn load(
        kube: &'a Client,
        config: &'a KissConfig,
        owner: &'a BoxSpec,
        load_workers: bool,
    ) -> Result<ClusterState<'a>, Error> {
        Ok(Self {
            config: Cow::Borrowed(config),
            control_planes: ClusterBoxGroup::load_control_planes(kube, &owner.group.cluster_name)
                .await?,
            owner_group: Cow::Borrowed(&owner.group),
            owner_uuid: owner.machine.uuid,
            workers: if load_workers {
                Some(ClusterBoxGroup::load_worker_nodes(kube, &owner.group.cluster_name).await?)
            } else {
                None
            },
        })
    }

    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    pub async fn load_current_cluster(
        kube: &'a Client,
        load_workers: bool,
    ) -> Result<ClusterState<'a>> {
        let config = KissConfig::try_default(kube).await?;
        let cluster_name = config.kiss_cluster_name.clone();

        // load the current control planes
        let control_planes = ClusterBoxGroup::load_control_planes(kube, &cluster_name).await?;

        // load a master node
        let filter = ClusterBoxFilter::Running;
        let owner = match control_planes.iter(filter).next() {
            Some((owner, _)) => owner.clone(),
            None => bail!("cluster {cluster_name:?} is not running"),
        };

        // load the current workers
        let workers = if load_workers {
            Some(ClusterBoxGroup::load_worker_nodes(kube, &cluster_name).await?)
        } else {
            None
        };

        Ok(Self {
            config: Cow::Owned(config),
            control_planes,
            owner_group: Cow::Owned(BoxGroupSpec {
                cluster_name,
                role: BoxGroupRole::ControlPlane,
            }),
            owner_uuid: owner.uuid,
            workers,
        })
    }

    pub fn get_first_control_plane(&self) -> Result<&BoxCrd> {
        self.control_planes
            .iter(ClusterBoxFilter::Running)
            .map(|(_, r#box)| r#box)
            .next()
            .ok_or_else(|| {
                anyhow!(
                    "Cluster \"{}\" status: control-plane nodes are ready",
                    &self.owner_group.cluster_name,
                )
            })
    }

    pub fn get_control_planes_as_string(&self) -> String {
        let filter = ClusterBoxFilter::RunningWith {
            uuid: self.owner_uuid,
        };
        let nodes = self.control_planes.to_vec(filter);

        const NODE_ROLE: &str = "kube_control_plane";
        get_nodes_as_string(nodes, NODE_ROLE)
    }

    pub fn get_etcd_nodes_as_string(&self) -> String {
        let filter = ClusterBoxFilter::RunningWith {
            uuid: self.owner_uuid,
        };
        let mut nodes = self.control_planes.to_vec(filter);

        // truncate the number of nodes to `etcd_nodes_max`
        if self.config.etcd_nodes_max > 0 {
            nodes.truncate(self.config.etcd_nodes_max);
        }

        // ETCD nodes should be odd (RAFT)
        if nodes.len() % 2 == 0 {
            nodes.pop();
        }

        const NODE_ROLE: &str = "etcd";
        get_nodes_as_string(nodes, NODE_ROLE)
    }

    pub fn get_worker_nodes(&self) -> Result<impl Iterator<Item = &BoxCrd>> {
        self.workers
            .as_ref()
            .map(|workers| {
                let filter = ClusterBoxFilter::Running;
                workers.iter(filter).map(|(_, object)| object)
            })
            .ok_or_else(|| anyhow!("worker nodes are not loaded"))
    }

    pub fn get_worker_nodes_as_string(&self) -> String {
        self.workers
            .as_ref()
            .map(|workers| {
                let filter = ClusterBoxFilter::Running;
                let nodes = workers.to_vec(filter);

                const NODE_ROLE: &str = "kube_node";
                get_nodes_as_string(nodes, NODE_ROLE)
            })
            .unwrap_or_default()
    }

    pub fn is_control_plane_ready(&self) -> bool {
        let control_planes_running = self.control_planes.num_running();
        let control_planes_total = self.control_planes.num_total();

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

    fn is_node_control_plane(&self) -> bool {
        self.control_planes.contains(self.owner_uuid)
    }

    pub fn is_joinable(&self) -> bool {
        if self.is_node_control_plane() {
            self.control_planes.is_next(self.owner_uuid)
        } else {
            self.is_control_plane_ready()
        }
    }

    pub fn is_new(&self) -> bool {
        self.is_node_control_plane() && !self.control_planes.is_running()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ClusterLockState {
    box_name: String,
    role: BoxGroupRole,
}

struct ClusterBoxGroup {
    nodes: BTreeMap<ClusterBoxState, BoxCrd>,
}

impl ClusterBoxGroup {
    #[instrument(level = Level::INFO, err(Display), skip(kube))]
    async fn load_control_planes(kube: &Client, cluster_name: &str) -> Result<Self, Error> {
        let api = Api::<BoxCrd>::all(kube.clone());
        let lp = ListParams::default();
        Ok(Self {
            nodes: api
                .list(&lp)
                .await?
                .items
                .into_iter()
                .filter(|r#box| {
                    r#box.spec.group.cluster_name == cluster_name
                        && r#box.spec.group.role == BoxGroupRole::ControlPlane
                })
                .map(|r#box| {
                    let key = ClusterBoxState::from_box(&r#box);
                    let value = r#box;
                    (key, value)
                })
                .collect(),
        })
    }

    #[instrument(level = Level::INFO, err(Display), skip(kube))]
    async fn load_worker_nodes(kube: &Client, cluster_name: &str) -> Result<Self, Error> {
        let api = Api::<BoxCrd>::all(kube.clone());
        let lp = ListParams::default();
        Ok(Self {
            nodes: api
                .list(&lp)
                .await?
                .items
                .into_iter()
                .filter(|r#box| {
                    r#box.spec.group.cluster_name == cluster_name
                        && r#box.spec.group.role != BoxGroupRole::ControlPlane
                })
                .map(|r#box| {
                    let key = ClusterBoxState::from_box(&r#box);
                    let value = r#box;
                    (key, value)
                })
                .collect(),
        })
    }

    fn contains(&self, uuid: Uuid) -> bool {
        self.nodes.iter().any(|(node, _)| node.uuid == uuid)
    }

    fn is_next(&self, uuid: Uuid) -> bool {
        self.nodes
            .iter()
            .find(|(node, _)| !node.is_running)
            .map(|(node, _)| node.uuid == uuid)
            .unwrap_or_default()
    }

    fn is_running(&self) -> bool {
        self.nodes.iter().any(|(node, _)| node.is_running)
    }

    fn iter(&self, filter: ClusterBoxFilter) -> impl Iterator<Item = (&ClusterBoxState, &BoxCrd)> {
        self.nodes
            .iter()
            .filter(|(node, _)| node.is_running || filter.contains(node.uuid))
            .sorted_by_key(|&(node, _)| (&node.created_at, node))
    }

    fn num_running(&self) -> usize {
        self.nodes
            .iter()
            .filter(|(node, _)| node.is_running)
            .count()
    }

    fn num_total(&self) -> usize {
        self.nodes.len()
    }

    fn to_vec(&self, filter: ClusterBoxFilter) -> Vec<&ClusterBoxState> {
        self.iter(filter).map(|(node, _)| node).collect()
    }
}

enum ClusterBoxFilter {
    Running,
    RunningWith { uuid: Uuid },
}

impl ClusterBoxFilter {
    fn contains(&self, target: Uuid) -> bool {
        match self {
            Self::Running => false,
            Self::RunningWith { uuid } => *uuid == target,
        }
    }
}

fn get_nodes_as_string(nodes: Vec<&ClusterBoxState>, node_role: &str) -> String {
    nodes
        .iter()
        .sorted_by_key(|&&node| {
            (
                // Place the unready node to the last
                // so that the cluster info should be preferred.
                !node.is_running,
                &node.created_at,
                node,
            )
        })
        .filter_map(|node| node.get_host())
        .map(|host| format!("{node_role}:{host}"))
        .join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ClusterBoxState {
    created_at: Option<Time>,
    hostname: String,
    ip: Option<IpAddr>,
    is_running: bool,
    uuid: Uuid,
}

impl AsRef<Self> for ClusterBoxState {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl ClusterBoxState {
    fn from_box(object: &BoxCrd) -> Self {
        Self {
            created_at: object.metadata.creation_timestamp.clone(),
            hostname: object.spec.machine.hostname(),
            ip: object
                .status
                .as_ref()
                .and_then(|status| status.access.primary.as_ref())
                .map(|interface| interface.address),
            is_running: object
                .status
                .as_ref()
                .map(|status| matches!(status.state, BoxState::Running))
                .unwrap_or_default(),
            uuid: object.spec.machine.uuid,
        }
    }

    fn get_host(&self) -> Option<String> {
        Some(format!("{}:{}", &self.hostname, self.ip?))
    }
}
