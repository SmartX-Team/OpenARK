use dash_api::function::{FunctionActorSpec, FunctionCrd, FunctionState};
use ipis::core::anyhow::{bail, Result};
use kiss_api::kube::{Api, Client};

use crate::input::InputTemplate;

use self::job::FunctionActorJobClient;

pub mod job;

pub struct FunctionSession {
    client: FunctionActorClient,
    pub input: InputTemplate,
}

impl FunctionSession {
    pub async fn load(kube: Client, name: &str) -> Result<Self> {
        let api = Api::<FunctionCrd>::all(kube.clone());
        let function = api.get(name).await?;

        let spec = match function.status {
            Some(status) if status.state == Some(FunctionState::Ready) => match status.spec {
                Some(spec) => spec,
                None => bail!("function has no spec status: {name:?}"),
            },
            Some(_) | None => bail!("function is not ready: {name:?}"),
        };

        Ok(Self {
            client: FunctionActorClient::try_new(&kube, function.spec.actor).await?,
            input: InputTemplate::new_empty(spec.input),
        })
    }
}

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

    pub const fn kube(&self) -> &Client {
        match self {
            Self::Job(client) => client.kube(),
        }
    }
}
