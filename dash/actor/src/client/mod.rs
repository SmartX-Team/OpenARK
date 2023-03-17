use dash_api::function::FunctionActorSpec;
use ipis::core::anyhow::Result;
use kiss_api::kube::Client;

use self::job::FunctionActorJobClient;

pub mod job;

pub enum FunctionActorClient {
    Job(Box<FunctionActorJobClient>),
}

impl FunctionActorClient {
    pub async fn try_new(kube: &Client, spec: FunctionActorSpec) -> Result<Self> {
        match spec {
            FunctionActorSpec::Job(spec) => FunctionActorJobClient::try_new(kube, spec)
                .await
                .map(Box::new)
                .map(Self::Job),
        }
    }
}
