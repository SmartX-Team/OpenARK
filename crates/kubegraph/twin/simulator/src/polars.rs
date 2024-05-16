use std::ops::{Add, Sub};

use anyhow::{anyhow, Result};
use kubegraph_api::{
    graph::{Graph, NetworkGraphMetadata},
    problem::ProblemSpec,
};
use pl::lazy::{
    dsl,
    frame::{IntoLazy, LazyFrame},
};

impl ::kubegraph_api::twin::LocalTwin<Graph<LazyFrame>> for super::Twin {
    type Output = LazyFrame;

    fn execute(&self, graph: Graph<LazyFrame>, problem: &ProblemSpec) -> Result<Self::Output> {
        let ProblemSpec {
            metadata:
                NetworkGraphMetadata {
                    capacity: _,
                    flow: key_flow,
                    function: _,
                    src: key_src,
                    sink: key_sink,
                    name: key_name,
                    supply: key_supply,
                    unit_cost: _,
                },
            verbose: _,
        } = problem;

        // Step 1. Collect graph data
        let Graph { edges, nodes } = graph;

        // Step 2. Define a problem
        let key_flow_in = format!("{key_sink}.{key_flow}");
        let key_flow_out = format!("{key_src}.{key_flow}");

        // Step 3. Apply edge flows to node supply
        let updated_nodes = nodes
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(&key_flow).gt(0i64))
                    .select([dsl::col(&key_src), dsl::col(&key_flow).alias(&key_flow_out)]),
                dsl::col(&key_name),
                dsl::col(&key_src),
            )
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(&key_flow).gt(0i64))
                    .select([dsl::col(&key_sink), dsl::col(&key_flow).alias(&key_flow_in)]),
                dsl::col(&key_name),
                dsl::col(&key_sink),
            )
            .with_column(
                dsl::col(&key_supply)
                    .sub(dsl::col(&key_flow_out).fill_null(0i64))
                    .add(dsl::col(&key_flow_in).fill_null(0i64)),
            )
            .drop([key_flow_in, key_flow_out]);

        // Step 4. Collect once
        let updated_nodes = updated_nodes
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes: {error}"))?
            .lazy();

        Ok(updated_nodes)
    }
}
