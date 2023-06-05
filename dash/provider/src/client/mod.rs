use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dash_api::function::FunctionActorSpec;
use dash_provider_api::{
    FunctionChannel, FunctionChannelKind, SessionContext, SessionContextMetadata,
};
use futures::TryFutureExt;
use kube::Client;
use serde::Serialize;
use serde_json::Value;

use crate::{
    input::{InputField, InputTemplate},
    storage::{KubernetesStorageClient, StorageClient},
};

use self::job::FunctionActorJobClient;

pub mod job;

#[async_trait]
pub trait FunctionSessionUpdateFields<Value> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        input: InputField<Value>,
    ) -> Result<()>;
}

#[async_trait]
impl<'a> FunctionSessionUpdateFields<String> for FunctionSession<'a> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        input: InputField<String>,
    ) -> Result<()> {
        self.input
            .update_field_string(storage, input)
            .await
            .map_err(|e| anyhow!("failed to parse input {:?}: {e}", &self.metadata.name))
    }
}

#[async_trait]
impl<'a> FunctionSessionUpdateFields<Value> for FunctionSession<'a> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        input: InputField<Value>,
    ) -> Result<()> {
        self.input
            .update_field_value(storage, input)
            .await
            .map_err(|e| anyhow!("failed to parse input {:?}: {e}", &self.metadata.name))
    }
}

pub struct FunctionSession<'a> {
    client: FunctionActorClient,
    input: InputTemplate,
    metadata: &'a SessionContextMetadata,
}

impl<'a> FunctionSession<'a> {
    pub async fn load(
        kube: Client,
        metadata: &'a SessionContextMetadata,
    ) -> Result<FunctionSession<'a>> {
        let storage = KubernetesStorageClient {
            namespace: &metadata.namespace,
            kube: &kube,
        };
        let function = storage.load_function(&metadata.name).await?;

        let origin = &function.spec.input;
        let parsed = &function.get_native_spec().input;

        Ok(Self {
            client: FunctionActorClient::try_new(&metadata.namespace, &kube, &function.spec.actor)
                .await?,
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

    pub async fn exists<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        inputs: Vec<InputField<Value>>,
    ) -> Result<bool>
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata)
            .and_then(|session| session.try_exists(inputs))
            .await
    }

    async fn try_exists<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<bool>
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
            .exists(&input)
            .await
            .map_err(|e| anyhow!("failed to check function {:?}: {e}", &self.metadata.name))
    }

    pub async fn create<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        inputs: Vec<InputField<Value>>,
    ) -> Result<FunctionChannel>
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata)
            .and_then(|session| session.try_create(inputs))
            .await
    }

    async fn try_create<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<FunctionChannel>
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
            .create(&input)
            .await
            .map_err(|e| anyhow!("failed to create function {:?}: {e}", &self.metadata.name))
    }

    pub async fn delete<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        inputs: Vec<InputField<Value>>,
    ) -> Result<FunctionChannel>
    where
        Self: FunctionSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata)
            .and_then(|session| session.try_delete(inputs))
            .await
    }

    async fn try_delete<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<FunctionChannel>
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
            .delete(&input)
            .await
            .map_err(|e| anyhow!("failed to delete function {:?}: {e}", &self.metadata.name))
    }
}

pub enum FunctionActorClient {
    Job(Box<FunctionActorJobClient>),
}

impl FunctionActorClient {
    pub async fn try_new(namespace: &str, kube: &Client, spec: &FunctionActorSpec) -> Result<Self> {
        match spec {
            FunctionActorSpec::Job(spec) => {
                FunctionActorJobClient::try_new(namespace.into(), kube, spec)
                    .await
                    .map(Box::new)
                    .map(Self::Job)
            }
        }
    }

    pub const fn kube(&self) -> &Client {
        match self {
            Self::Job(client) => client.kube(),
        }
    }

    pub async fn exists<Spec>(&self, input: &SessionContext<Spec>) -> Result<bool>
    where
        Spec: Serialize,
    {
        match self {
            Self::Job(client) => client.exists(input).await,
        }
    }

    pub async fn create<Spec>(&self, input: &SessionContext<Spec>) -> Result<FunctionChannel>
    where
        Spec: Serialize,
    {
        Ok(FunctionChannel {
            metadata: input.metadata.clone(),
            actor: match self {
                Self::Job(client) => client.create(input).await.map(FunctionChannelKind::Job)?,
            },
        })
    }

    pub async fn delete<Spec>(&self, input: &SessionContext<Spec>) -> Result<FunctionChannel>
    where
        Spec: Serialize,
    {
        Ok(FunctionChannel {
            metadata: input.metadata.clone(),
            actor: match self {
                Self::Job(client) => client.delete(input).await.map(FunctionChannelKind::Job)?,
            },
        })
    }
}
