use std::fmt;

use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use serde::{de::DeserializeOwned, Serialize};

use crate::PipeMessages;

#[async_trait]
pub trait Function
where
    Self: Send,
{
    type Args: Clone + fmt::Debug + Serialize + DeserializeOwned + Args;
    type Input: DeserializeOwned;
    type Output: 'static + Send + Serialize;

    async fn try_new(args: &<Self as Function>::Args) -> Result<Self>
    where
        Self: Sized;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as Function>::Input>,
    ) -> Result<PipeMessages<<Self as Function>::Output>>;
}
