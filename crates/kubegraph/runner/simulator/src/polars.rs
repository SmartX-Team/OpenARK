use std::ops::{Add, Sub};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kubegraph_api::{
    graph::{GraphData, GraphMetadata, ScopedNetworkGraphDB},
    problem::ProblemSpec,
};
use pl::lazy::{
    dsl,
    frame::{IntoLazy, LazyFrame},
};

#[async_trait]
impl ::kubegraph_api::runner::NetworkRunner<GraphData<LazyFrame>> for super::NetworkRunner {
    async fn execute(
        &self,
        graph_db: &dyn ScopedNetworkGraphDB,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec,
    ) -> Result<()> {
        let ProblemSpec {
            metadata,
            verbose: _,
        } = problem;
        let GraphMetadata {
            capacity: _,
            flow: key_flow,
            function: _,
            src: key_src,
            sink: key_sink,
            name: key_name,
            supply: key_supply,
            unit_cost: _,
        } = &metadata;

        // Step 1. Collect graph data
        let GraphData { edges, nodes } = graph;

        // Step 2. Define a problem
        let key_flow_in = format!("{key_sink}.{key_flow}");
        let key_flow_out = format!("{key_src}.{key_flow}");

        // Step 3. Apply edge flows to node supply
        let updated_nodes = nodes
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(key_flow).gt(0i64))
                    .select([dsl::col(key_src), dsl::col(key_flow).alias(&key_flow_out)]),
                dsl::col(key_name),
                dsl::col(key_src),
            )
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(key_flow).gt(0i64))
                    .select([dsl::col(key_sink), dsl::col(key_flow).alias(&key_flow_in)]),
                dsl::col(key_name),
                dsl::col(key_sink),
            )
            .with_column(
                dsl::col(key_supply)
                    .sub(dsl::col(&key_flow_out).fill_null(0i64))
                    .add(dsl::col(&key_flow_in).fill_null(0i64)),
            )
            .drop([key_flow_in, key_flow_out]);

        // Step 4. Collect once
        let collected_nodes = updated_nodes
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes: {error}"))?
            .lazy();

        // Step 5. Upload to the DB
        graph_db.insert(collected_nodes.into()).await
    }
}
