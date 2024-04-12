use anyhow::{bail, Result};
use clap::Parser;
use kiss_ansible::{cluster::ClusterState, AnsibleClient, AnsibleJob};
use tracing::{info, instrument, Level};

#[derive(Clone, Debug, Parser)]
pub struct ClusterUpgradeArgs {
    // #[arg(long, env = "KISS_CLUSTER_NAME", value_name = "NAME")]
    // pub cluster_name: Option<String>,
}

impl ClusterUpgradeArgs {
    #[instrument(level = Level::INFO, err(Display))]
    pub(crate) async fn run(self) -> Result<()> {
        let kube = ::kube::Client::try_default().await?;
        let client = AnsibleClient::try_default(&kube).await?;

        info!("Selecting default cluster");
        let cluster = ClusterState::load_current_cluster(&kube).await?;

        let job = AnsibleJob {
            cron: None,
            task: "upgrade",
            r#box: cluster.get_first_control_plane()?,
            new_group: None,
            new_state: None,
        };

        if client.spawn(&kube, job).await? {
            info!("Submitted!");
            Ok(())
        } else {
            bail!("Cancelled upgrading cluster")
        }
    }
}
