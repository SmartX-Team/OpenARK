use std::process::exit;

use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser};
use dash_query_provider::{Name, QueryClient, QueryClientArgs};
use futures::TryStreamExt;
use serde_json::Value;
use tracing::error;

#[derive(Clone, Debug, Parser)]
pub struct QueryArgs {
    #[command(flatten)]
    client: QueryClientArgs,

    #[arg(long, env = "PIPE_MODEL", value_name = "NAME")]
    model: Name,

    #[arg(action = ArgAction::Append)]
    sql: String,
}

#[tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();

    match try_main().await {
        Ok(()) => (),
        Err(error) => {
            error!("{error}");
            exit(1)
        }
    }
}

async fn try_main() -> Result<()> {
    let QueryArgs { client, model, sql } = QueryArgs::parse();
    let client = QueryClient::<Value>::try_new(&client, Some(&model)).await?;
    let mut rows = client.sql_and_decode(&sql).await?;
    while let Some(row) = rows.try_next().await.and_then(|row| {
        row.map(|row| {
            ::serde_json::to_string(&row)
                .map_err(|error| anyhow!("failed to serialize row to JSON format: {error}"))
        })
        .transpose()
    })? {
        println!("{row}");
    }
    Ok(())
}
