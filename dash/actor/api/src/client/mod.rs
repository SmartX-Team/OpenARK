use dash_api::function::FunctionActorSpec;
use ipis::core::anyhow::Result;
use kiss_api::kube::Client;
use serde::{Deserialize, Serialize};

use crate::{
    input::{InputFieldString, InputTemplate},
    source::SourceClient,
};

use self::job::FunctionActorJobClient;

pub mod job;

pub struct FunctionSession {
    client: FunctionActorClient,
    input: InputTemplate,
}

impl FunctionSession {
    pub async fn load(kube: Client, name: &str) -> Result<Self> {
        let (original, parsed) = SourceClient { kube: &kube }.load_function(name).await?;

        Ok(Self {
            client: FunctionActorClient::try_new(&kube, original.actor).await?,
            input: InputTemplate::new_empty(&original.input, parsed.input),
        })
    }

    pub async fn update_fields_string(&mut self, inputs: Vec<InputFieldString>) -> Result<()> {
        let source = SourceClient {
            kube: self.client.kube(),
        };

        self.input.update_fields_string(&source, inputs).await
    }

    pub async fn create_raw(self) -> Result<()> {
        let input = SessionContext {
            // TODO: to be implemented
            namespace: "dash".to_string(),
            spec: self.input.finalize()?,
        };

        self.client.create_raw(&input).await
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

    pub async fn create_raw<Spec>(&self, input: &SessionContext<Spec>) -> Result<()>
    where
        Spec: Serialize,
    {
        match self {
            Self::Job(client) => client.create_raw(input).await,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext<Spec> {
    pub namespace: String,
    pub spec: Spec,
}
