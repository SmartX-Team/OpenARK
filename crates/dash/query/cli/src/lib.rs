use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser};
use dash_query_provider::{QueryClient, QueryClientArgs};
use futures::TryStreamExt;
use serde_json::Value;
use tracing::{instrument, Level};

#[derive(Clone, Debug, Parser)]
pub struct QueryArgs {
    #[command(flatten)]
    client: QueryClientArgs,

    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,

    #[arg(action = ArgAction::Append, value_name = "SQL")]
    sql: String,
}

impl QueryArgs {
    #[instrument(level = Level::INFO, skip_all, fields(sql = %self.sql), err(Display))]
    pub async fn run(self) -> Result<()> {
        let Self { client, json, sql } = self;
        let client = QueryClient::try_new(&client).await?;

        if json {
            run_query_and_write_json(&client, &sql).await
        } else {
            run_query_and_print(&client, &sql).await
        }
    }
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
async fn run_query_and_print(client: &QueryClient, sql: &str) -> Result<()> {
    let df = client.sql(sql).await?;
    df.show().await?;
    Ok(())
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
async fn run_query_and_write_json(client: &QueryClient, sql: &str) -> Result<()> {
    let mut rows = client.sql_and_decode::<Value>(sql).await?;
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
