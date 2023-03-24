use actix_web::HttpResponse;
use dash_api::{function::FunctionActorSpec, kube::Client, serde_json::Value};
use ipis::{
    async_trait::async_trait,
    core::anyhow::{anyhow, Result},
    futures::TryFutureExt,
};
use serde::{Deserialize, Serialize};

use crate::{
    input::{InputField, InputTemplate},
    storage::{KubernetesStorageClient, StorageClient},
};

use self::job::FunctionActorJobClient;

pub mod job;

pub struct FunctionSession<'a> {
    client: FunctionActorClient,
    input: InputTemplate,
    metadata: &'a SessionContextMetadata,
}

#[async_trait]
pub trait FunctionSessionUpdateFields<Value> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        inputs: InputField<Value>,
    ) -> Result<()>;
}

#[async_trait]
impl<'a> FunctionSessionUpdateFields<String> for FunctionSession<'a> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        inputs: InputField<String>,
    ) -> Result<()> {
        self.input
            .update_field_string(storage, inputs)
            .await
            .map_err(|e| anyhow!("failed to parse inputs {:?}: {e}", &self.metadata.name))
    }
}

#[async_trait]
impl<'a> FunctionSessionUpdateFields<Value> for FunctionSession<'a> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        inputs: InputField<Value>,
    ) -> Result<()> {
        self.input
            .update_field_value(storage, inputs)
            .await
            .map_err(|e| anyhow!("failed to parse inputs {:?}: {e}", &self.metadata.name))
    }
}

impl<'a> FunctionSession<'a> {
    pub async fn load(
        kube: Client,
        metadata: &'a SessionContextMetadata,
    ) -> Result<FunctionSession<'a>> {
        let storage = KubernetesStorageClient { kube: &kube };
        let function = storage.load_function(&metadata.name).await?;

        let origin = &function.spec.input;
        let parsed = &function.get_native_spec().input;

        Ok(Self {
            client: FunctionActorClient::try_new(&kube, &function.spec.actor).await?,
            input: InputTemplate::new_empty(origin, parsed.clone()),
            metadata,
        })
    }

    async fn update_fields<Value>(&mut self, inputs: Vec<InputField<Value>>) -> Result<()>
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        let namespace = self.metadata.namespace.clone();
        let kube = self.client.kube().clone();
        let storage = StorageClient {
            namespace: &namespace,
            kube: &kube,
        };

        for input in inputs {
            self.update_field(&storage, input).await?;
        }
        Ok(())
    }

    pub async fn create_raw<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        inputs: Vec<InputField<Value>>,
    ) -> SessionResult
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata)
            .and_then(|session| session.try_create_raw(inputs))
            .await
            .into()
    }

    async fn try_create_raw<Value>(
        mut self,
        inputs: Vec<InputField<Value>>,
    ) -> Result<FunctionChannel>
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        let input = SessionContext {
            metadata: self.metadata.clone(),
            spec: {
                self.update_fields(inputs).await?;
                self.input.finalize()?
            },
        };

        self.client
            .create_raw(&input)
            .await
            .map_err(|e| anyhow!("failed to create function {:?}: {e}", &self.metadata.name))
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

    pub async fn create_raw<Spec>(&self, input: &SessionContext<Spec>) -> Result<FunctionChannel>
    where
        Spec: Serialize,
    {
        Ok(FunctionChannel {
            metadata: input.metadata.clone(),
            actor: match self {
                Self::Job(client) => client
                    .create_raw(input)
                    .await
                    .map(FunctionChannelKind::Job)?,
            },
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "result", content = "spec")]
pub enum SessionResult<T = FunctionChannel> {
    Ok(T),
    Err(String),
}

impl<T, E> From<Result<T, E>> for SessionResult<T>
where
    E: ToString,
{
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(error) => Self::Err(error.to_string()),
        }
    }
}

impl<T> From<SessionResult<T>> for HttpResponse
where
    T: Serialize,
{
    fn from(value: SessionResult<T>) -> Self {
        match value {
            SessionResult::Ok(_) => HttpResponse::Ok().json(value),
            SessionResult::Err(_) => HttpResponse::Forbidden().json(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionChannel {
    pub metadata: SessionContextMetadata,
    pub actor: FunctionChannelKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec")]
pub enum FunctionChannelKind {
    Job(self::job::FunctionChannelKindJob),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext<Spec> {
    pub metadata: SessionContextMetadata,
    pub spec: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextMetadata {
    pub name: String,
    pub namespace: String,
}
