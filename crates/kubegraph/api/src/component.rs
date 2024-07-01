use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use tracing::{instrument, Level};

#[async_trait]
pub trait NetworkComponentExt
where
    Self: NetworkComponent,
    <Self as NetworkComponent>::Args: Parser,
{
    #[instrument(level = Level::INFO, skip(signal))]
    async fn try_default(signal: &FunctionSignal) -> Result<Self>
    where
        Self: Sized,
    {
        let args = <Self as NetworkComponent>::Args::try_parse()?;
        <Self as NetworkComponent>::try_new(args, signal).await
    }
}

#[async_trait]
impl<T> NetworkComponentExt for T
where
    Self: NetworkComponent,
    <Self as NetworkComponent>::Args: Parser,
{
}

#[async_trait]
pub trait NetworkComponent {
    type Args;

    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait]
impl<T> NetworkComponent for T
where
    Self: Default,
{
    type Args = self::sealed::NetworkComponentEmptyArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        let self::sealed::NetworkComponentEmptyArgs {} = args;
        Ok(Self::default())
    }
}

mod sealed {
    use clap::Parser;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(
        Copy,
        Clone,
        Debug,
        Default,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Serialize,
        Deserialize,
        JsonSchema,
        Parser,
    )]
    #[clap(rename_all = "kebab-case")]
    #[serde(rename_all = "camelCase")]
    pub struct NetworkComponentEmptyArgs {}
}
