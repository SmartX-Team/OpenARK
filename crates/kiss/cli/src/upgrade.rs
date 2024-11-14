use anyhow::{bail, Result};
use clap::{Parser, ValueEnum};
use futures::{stream::FuturesUnordered, TryStreamExt};
use kiss_ansible::{cluster::ClusterState, AnsibleClient, AnsibleJob, AnsibleResourceType};
use kiss_api::r#box::BoxCrd;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::{info, instrument, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterUpgradeArgs {
    // #[arg(long, env = "KISS_CLUSTER_NAME", value_name = "NAME")]
    // pub cluster_name: Option<String>,
    #[arg(env = "KISS_CLUSTER_GROUP", value_name = "TYPE")]
    pub group: ClusterUpgradeGroupType,
}

impl ClusterUpgradeArgs {
    #[instrument(level = Level::INFO, err(Display))]
    pub(crate) async fn run(self) -> Result<()> {
        let group = self.group;

        let kube = ::kube::Client::try_default().await?;
        let client = AnsibleClient::try_default(&kube).await?;

        info!("Selecting default cluster");
        let cluster = ClusterState::load_current_cluster(&kube, group.has_workers()).await?;

        let status = match group {
            ClusterUpgradeGroupType::ControlPlane => {
                self.run_control_plane(&kube, client, cluster).await?
            }
            ClusterUpgradeGroupType::Workers => self.run_workers(&kube, client, cluster).await?,
        };

        match status {
            ClusterUpgradeStatus::Completed => {
                info!("Submitted!");
                Ok(())
            }
            ClusterUpgradeStatus::PartiallyCompleted => {
                bail!("Partially cancelled upgrading cluster {group}")
            }
            ClusterUpgradeStatus::Failed => {
                bail!("Cancelled upgrading cluster {group}")
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, client, cluster), err(Display))]
    async fn run_control_plane<'a>(
        &self,
        kube: &::kube::Client,
        client: AnsibleClient,
        cluster: ClusterState<'a>,
    ) -> Result<ClusterUpgradeStatus> {
        let first_node = cluster.get_first_control_plane()?;
        let job = create_job(first_node);

        if client.spawn(kube, job).await? {
            Ok(ClusterUpgradeStatus::Completed)
        } else {
            Ok(ClusterUpgradeStatus::Failed)
        }
    }

    #[instrument(level = Level::INFO, skip(kube, client, cluster), err(Display))]
    async fn run_workers<'a>(
        &self,
        kube: &::kube::Client,
        client: AnsibleClient,
        cluster: ClusterState<'a>,
    ) -> Result<ClusterUpgradeStatus> {
        let status: Vec<_> = cluster
            .get_worker_nodes()?
            .map(create_job)
            .map(|job| client.spawn(kube, job))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await?;

        if status.iter().all(|&e| e) {
            Ok(ClusterUpgradeStatus::Completed)
        } else if status.iter().any(|&e| e) {
            Ok(ClusterUpgradeStatus::PartiallyCompleted)
        } else {
            Ok(ClusterUpgradeStatus::Failed)
        }
    }
}

#[derive(Copy, Clone, Debug, Display, EnumString, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterUpgradeGroupType {
    ControlPlane,
    Workers,
}

impl ClusterUpgradeGroupType {
    const fn has_workers(&self) -> bool {
        matches!(self, Self::Workers)
    }
}

enum ClusterUpgradeStatus {
    Completed,
    PartiallyCompleted,
    Failed,
}

fn create_job(target_box: &BoxCrd) -> AnsibleJob {
    AnsibleJob {
        cron: None,
        task: "upgrade",
        r#box: target_box,
        new_group: None,
        new_state: None,
        is_critical: true,
        resource_type: AnsibleResourceType::Normal,
        use_workers: false,
    }
}
