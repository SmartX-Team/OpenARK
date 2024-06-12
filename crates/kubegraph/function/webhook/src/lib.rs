#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{anyhow, Result};
use ark_core::result::Result as WebhookResult;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    function::{
        call::FunctionCallRequest, spawn::FunctionSpawnContext, webhook::NetworkFunctionWebhookSpec,
    },
    graph::GraphEdges,
};
use reqwest::Client;
use serde::Serialize;
use tracing::{instrument, Level};

#[async_trait]
pub trait NetworkFunctionWebhook<DB, T, M>
where
    DB: Sync,
    M: Serialize,
{
    async fn spawn(&self, ctx: FunctionSpawnContext<'async_trait, DB, T, M>) -> Result<()>
    where
        DB: 'async_trait + Send,
        M: 'async_trait + Send;
}

#[async_trait]
impl<DB, M> NetworkFunctionWebhook<DB, LazyFrame, M> for NetworkFunctionWebhookSpec
where
    DB: Sync,
    M: Serialize,
{
    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn spawn(&self, ctx: FunctionSpawnContext<'async_trait, DB, LazyFrame, M>) -> Result<()>
    where
        DB: 'async_trait + Send,
        M: 'async_trait + Send,
    {
        let Self { endpoint } = self;
        let FunctionSpawnContext {
            graph,
            graph_db: _,
            kube: _,
            metadata,
            static_edges,
            template,
        } = ctx;

        let client = Client::builder()
            .build()
            .map_err(|error| anyhow!("failed to create a webhook client: {error}"))?;

        let ctx = FunctionCallRequest {
            graph: graph.collect().await?,
            metadata,
            static_edges: match static_edges {
                Some(static_edges) => static_edges
                    .into_inner()
                    .collect()
                    .await
                    .map(GraphEdges::new)
                    .map(Some)?,
                None => None,
            },
            template,
        };
        let response = client
            .post(endpoint.0.clone())
            .json(&ctx)
            .send()
            .await
            .map_err(|error| anyhow!("failed to call webhook: {error}"))?;
        let status = response.status();

        response
            .text()
            .await
            .map_err(|error| anyhow!("failed to get a response from webhook: {error}"))
            .map(|text| {
                ::serde_json::from_str(&text).unwrap_or_else(|_| match text.as_str() {
                    "" | "null" if status.is_success() => WebhookResult::Ok(()),
                    _ => WebhookResult::Err(text),
                })
            })
            .and_then(|result| match result {
                WebhookResult::Ok(()) => Ok(()),
                WebhookResult::Err(error) => Err(anyhow!("failed to call webhook: {error}")),
            })
    }
}
