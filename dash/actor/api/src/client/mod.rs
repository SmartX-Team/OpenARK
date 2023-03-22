use dash_api::function::FunctionActorSpec;
use ipis::core::anyhow::Result;
use kiss_api::kube::Client;
use serde::{Deserialize, Serialize};

use crate::{
    input::{InputFieldString, InputTemplate},
    storage::KubernetesStorageClient,
};

use self::job::FunctionActorJobClient;

pub mod job;

pub struct FunctionSession {
    client: FunctionActorClient,
    input: InputTemplate,
}

impl FunctionSession {
    pub async fn load(kube: Client, name: &str) -> Result<Self> {
        let storage = KubernetesStorageClient { kube: &kube };
        let function = storage.load_function(name).await?;

        let origin = &function.spec.input;
        let parsed = &function.get_native_spec().input;

        Ok(Self {
            client: FunctionActorClient::try_new(&kube, &function.spec.actor).await?,
            input: InputTemplate::new_empty(origin, parsed.clone()),
        })
    }

    pub async fn update_fields_string(&mut self, inputs: Vec<InputFieldString>) -> Result<()> {
        let storage = KubernetesStorageClient {
            kube: self.client.kube(),
        };

        self.input.update_fields_string(&storage, inputs).await
    }

    pub async fn create_raw(self) -> Result<()> {
        let input = SessionContext {
            // TODO: to be implemented
            metadata: SessionContextMetadata {
                namespace: "vine".to_string(),
            },
            spec: self.input.finalize()?,
        };

        self.client.create_raw(&input).await
    }
}

pub enum FunctionActorClient {
    Job(Box<FunctionActorJobClient>),
}

impl FunctionActorClient {
    pub async fn try_new(kube: &Client, spec: &FunctionActorSpec) -> Result<Self> {
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
    pub metadata: SessionContextMetadata,
    pub spec: Spec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextMetadata {
    pub namespace: String,
}
