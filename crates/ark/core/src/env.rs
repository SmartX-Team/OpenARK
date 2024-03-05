use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tracing::{instrument, Level};

#[async_trait]
pub trait Infer<'a> {
    type GenesisArgs: Send + ?Sized;
    type GenesisResult: Send;

    #[instrument(level = Level::INFO, skip_all)]
    async fn infer() -> Self
    where
        Self: Sized,
    {
        // init tracer
        crate::tracer::init_once();

        match <Self as Infer<'a>>::try_infer().await {
            Ok(this) => this,
            Err(e) => {
                ::tracing::error!("failed to infer: {e}");
                panic!("failed to infer: {e}");
            }
        }
    }

    async fn try_infer() -> Result<Self>
    where
        Self: Sized;

    async fn genesis(
        args: <Self as Infer<'a>>::GenesisArgs,
    ) -> Result<<Self as Infer<'a>>::GenesisResult>;
}

pub fn infer<K, R>(key: K) -> Result<R>
where
    K: AsRef<str>,
    R: ::core::str::FromStr,
    <R as ::core::str::FromStr>::Err: 'static + Send + Sync + ::core::fmt::Display,
{
    let key = key.as_ref();

    infer_string(key).and_then(|e| {
        e.parse()
            .map_err(|error| anyhow!("failed to parse the environment variable ({key}): {error}"))
    })
}

pub fn infer_string<K>(key: K) -> Result<String>
where
    K: AsRef<str>,
{
    let key = key.as_ref();

    ::std::env::var(key).map_err(|_| anyhow!("failed to find the environment variable: {key}"))
}
