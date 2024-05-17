use std::time::Duration;

use anyhow::Result;
use kubegraph_api::{
    connector::NetworkConnectors, db::NetworkGraphDB, problem::ProblemSpec, twin::LocalTwin,
};
use tokio::time::sleep;
use tracing::{error, info, warn};

pub async fn loop_forever(db: impl NetworkConnectors + NetworkGraphDB) {
    loop {
        if let Err(error) = try_loop_forever(&db).await {
            error!("failed to run twin: {error}");

            let duration = Duration::from_secs(5);
            warn!("restaring twin in {duration:?}...");
            sleep(duration).await
        }
    }
}

async fn try_loop_forever(db: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()> {
    #[cfg(feature = "vm-local")]
    let vm = ::kubegraph_vm_local::VirtualMachine::default();

    let twin = NetworkTwin::try_default().await?;
    twin.try_loop_forever(db).await
}

struct NetworkTwin {
    #[cfg(feature = "twin-simulator")]
    simulator: ::kubegraph_twin_simulator::Twin,
}

impl NetworkTwin {
    async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "twin-simulator")]
            simulator: ::kubegraph_twin_simulator::Twin::default(),
        })
    }

    async fn try_loop_forever(self, db: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()> {
        loop {
            let mut graph = db.get_graph(None).await?;
            let problem = ProblemSpec {
                ..Default::default()
            };
            graph.nodes = self.simulator.execute(graph.clone(), &problem)?;

            let duration = Duration::from_secs(5);
            info!("restaring twin in {duration:?}...");
            sleep(duration).await
        }
        todo!()
    }
}
