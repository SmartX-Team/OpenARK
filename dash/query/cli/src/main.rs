use std::{cell::RefCell, process::exit};

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Parser};
use dash_query_provider::{QueryClient, QueryClientArgs};
use futures::TryStreamExt;
use inquire::{autocompletion::Replacement, Autocomplete, CustomUserError, Text};
use serde_json::Value;
use tracing::error;

#[derive(Clone, Debug, Parser)]
pub struct QueryArgs {
    #[command(flatten)]
    client: QueryClientArgs,

    #[arg(action = ArgAction::Append)]
    sql: Option<String>,
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
    let QueryArgs { client, sql } = QueryArgs::parse();
    let client = QueryClient::try_new(&client).await?;

    let mut tables = client.list_table_names();
    let table_sample = match tables.next() {
        Some(table) => table.clone(),
        None => bail!("None of the tables are detected!"),
    };

    match sql {
        Some(sql) => run_query(&client, &sql).await,
        None => {
            try_main_interactive(client, table_sample).await;
            Ok(())
        }
    }
}

async fn try_main_interactive(client: QueryClient, table_sample: String) {
    #[derive(Clone, Default)]
    struct History(RefCell<Vec<String>>);

    impl Autocomplete for History {
        fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|history| history.starts_with(input))
                .cloned()
                .collect())
        }

        fn get_completion(
            &mut self,
            _input: &str,
            highlighted_suggestion: Option<String>,
        ) -> Result<Replacement, CustomUserError> {
            Ok(highlighted_suggestion)
        }
    }

    let placeholder = format!("SELECT * FROM {table_sample};");

    let history = History::default();
    loop {
        let sql = match Text::new("sql> ")
            .with_autocomplete(history.clone())
            .with_placeholder(&placeholder)
            .prompt()
        {
            Ok(sql) => {
                history.0.borrow_mut().push(sql.clone());
                sql
            }
            Err(error) => {
                error!("{error}");
                continue;
            }
        };

        match sql.as_str() {
            "exit" => break,
            sql => match run_query(&client, sql).await {
                Ok(()) => continue,
                Err(error) => {
                    error!("{error}");
                    continue;
                }
            },
        }
    }
}

async fn run_query(client: &QueryClient, sql: &str) -> Result<()> {
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
