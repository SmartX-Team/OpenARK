pub mod job;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dash_api::task::TaskActorSpec;
use dash_provider_api::{SessionContext, SessionContextMetadata, TaskChannel, TaskChannelKind};
use futures::TryFutureExt;
use kube::Client;
use serde::Serialize;
use serde_json::Value;

use crate::{
    input::{InputField, InputTemplate},
    storage::{KubernetesStorageClient, StorageClient},
};

#[async_trait]
pub trait TaskSessionUpdateFields<Value> {
    async fn update_field(
        &mut self,
        storage: &StorageClient,
        input: InputField<Value>,
    ) -> Result<()>;
}

#[async_trait]
impl<'a> TaskSessionUpdateFields<String> for TaskSession<'a> {
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
impl<'a> TaskSessionUpdateFields<Value> for TaskSession<'a> {
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

pub struct TaskSession<'a> {
    client: TaskActorClient,
    input: InputTemplate,
    metadata: &'a SessionContextMetadata,
}

impl<'a> TaskSession<'a> {
    pub async fn load(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        task_name: &str,
    ) -> Result<TaskSession<'a>> {
        let storage = KubernetesStorageClient {
            namespace: &metadata.namespace,
            kube: &kube,
        };
        let task = storage.load_task(task_name).await?;

        let origin = &task.spec.input;
        let parsed = &task.get_native_spec().input;

        Ok(Self {
            client: TaskActorClient::try_new(&metadata.namespace, &kube, &task.spec.actor).await?,
            input: InputTemplate::new_empty(origin, parsed.clone()),
            metadata,
        })
    }

    async fn update_fields<Value>(&mut self, inputs: Vec<InputField<Value>>) -> Result<()>
    where
        Self: TaskSessionUpdateFields<Value>,
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
        task_name: &str,
        inputs: Vec<InputField<Value>>,
    ) -> Result<bool>
    where
        Self: TaskSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata, task_name)
            .and_then(|session| session.try_exists(inputs))
            .await
    }

    async fn try_exists<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<bool>
    where
        Self: TaskSessionUpdateFields<Value>,
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
            .map_err(|e| anyhow!("failed to check task {:?}: {e}", &self.metadata.name))
    }

    pub async fn create<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        task_name: &str,
        inputs: Vec<InputField<Value>>,
    ) -> Result<TaskChannel>
    where
        Self: TaskSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata, task_name)
            .and_then(|session| session.try_create(inputs))
            .await
    }

    async fn try_create<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<TaskChannel>
    where
        Self: TaskSessionUpdateFields<Value>,
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
            .map_err(|e| anyhow!("failed to create task {:?}: {e}", &self.metadata.name))
    }

    pub async fn delete<Value>(
        kube: Client,
        metadata: &'a SessionContextMetadata,
        task_name: &str,
        inputs: Vec<InputField<Value>>,
    ) -> Result<TaskChannel>
    where
        Self: TaskSessionUpdateFields<Value>,
    {
        Self::load(kube, metadata, task_name)
            .and_then(|session| session.try_delete(inputs))
            .await
    }

    async fn try_delete<Value>(mut self, inputs: Vec<InputField<Value>>) -> Result<TaskChannel>
    where
        Self: TaskSessionUpdateFields<Value>,
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
            .map_err(|e| anyhow!("failed to delete task {:?}: {e}", &self.metadata.name))
    }
}

pub enum TaskActorClient {
    Job(Box<self::job::TaskActorJobClient>),
}

impl TaskActorClient {
    pub async fn try_new(namespace: &str, kube: &Client, spec: &TaskActorSpec) -> Result<Self> {
        let use_prefix = true;
        match spec {
            TaskActorSpec::Job(spec) => {
                self::job::TaskActorJobClient::try_new(namespace.into(), kube, spec, use_prefix)
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

    pub async fn create<Spec>(&self, input: &SessionContext<Spec>) -> Result<TaskChannel>
    where
        Spec: Serialize,
    {
        Ok(TaskChannel {
            metadata: input.metadata.clone(),
            actor: match self {
                Self::Job(client) => client.create(input).await.map(TaskChannelKind::Job)?,
            },
        })
    }

    pub async fn delete<Spec>(&self, input: &SessionContext<Spec>) -> Result<TaskChannel>
    where
        Spec: Serialize,
    {
        Ok(TaskChannel {
            metadata: input.metadata.clone(),
            actor: match self {
                Self::Job(client) => client.delete(input).await.map(TaskChannelKind::Job)?,
            },
        })
    }
}
