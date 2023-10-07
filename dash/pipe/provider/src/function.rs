use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use clap::Args;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};

use crate::{PipeMessages, StorageIO};

#[async_trait(?Send)]
pub trait Function {
    type Args: Clone + fmt::Debug + Serialize + DeserializeOwned + Args;
    type Input: 'static + Send + Sync + Default + DeserializeOwned + JsonSchema;
    type Output: 'static + Send + Sync + Default + Serialize + JsonSchema;

    async fn try_new(
        args: &<Self as Function>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self>
    where
        Self: Sized;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as Function>::Input>,
    ) -> Result<PipeMessages<<Self as Function>::Output>>;
}

#[derive(Clone, Debug, Default)]
pub struct FunctionContext {
    is_disabled_write_metadata: bool,
    is_terminating: Arc<AtomicBool>,
}

impl FunctionContext {
    pub fn disable_store_metadata(&mut self) {
        self.is_disabled_write_metadata = true;
    }

    pub(crate) const fn is_disabled_store_metadata(&self) -> bool {
        self.is_disabled_write_metadata
    }
}

impl FunctionContext {
    pub(crate) fn trap_on_sigint(self) -> Result<()> {
        ::ctrlc::set_handler(move || self.terminate())
            .map_err(|error| anyhow!("failed to set SIGINT handler: {error}"))
    }

    pub(crate) fn terminate(&self) {
        self.is_terminating.store(true, Ordering::SeqCst)
    }

    pub fn terminate_ok<T>(&self) -> Result<PipeMessages<T>>
    where
        T: Default,
    {
        self.terminate();
        Ok(PipeMessages::None)
    }

    pub fn terminate_err<T>(&self, error: impl Into<Error>) -> Result<PipeMessages<T>>
    where
        T: Default,
    {
        self.terminate();
        Err(error.into())
    }

    pub(crate) fn is_terminating(&self) -> bool {
        self.is_terminating.load(Ordering::SeqCst)
    }
}
