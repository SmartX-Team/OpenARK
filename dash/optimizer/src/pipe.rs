use std::fmt;

use anyhow::Result;
use dash_pipe_provider::{Name, OwnedFunctionBuilder, PipeArgs, RemoteFunction};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};

pub fn init_pipe<F>(
    function: F,
    model_in: Name,
    model_out: Name,
) -> Result<PipeArgs<OwnedFunctionBuilder<F>>>
where
    F: Send + Sync + RemoteFunction,
    <F as RemoteFunction>::Input: fmt::Debug + DeserializeOwned + JsonSchema,
    <F as RemoteFunction>::Output: fmt::Debug + Serialize + JsonSchema,
{
    PipeArgs::with_function(function).map(|args| {
        args.with_ignore_sigint(true)
            .with_model_in(Some(model_in))
            .with_model_out(Some(model_out))
            .with_storage_persistence(false)
            .with_storage_persistence_metadata(false)
    })
}
