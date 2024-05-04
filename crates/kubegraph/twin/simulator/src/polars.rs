use std::ops::{Add, Sub};

use anyhow::Result;
use kubegraph_api::{
    graph::Graph,
    solver::{Problem, ProblemMetadata},
};
use pl::lazy::{dsl, frame::LazyFrame};

impl ::kubegraph_api::twin::LocalTwin<Graph<LazyFrame>, String> for super::Twin {
    type Output = Graph<LazyFrame>;

    fn execute(&self, graph: Graph<LazyFrame>, problem: &Problem<String>) -> Result<Self::Output> {
        let Problem {
            metadata:
                ProblemMetadata {
                    flow: key_flow,
                    src: key_src,
                    sink: key_sink,
                    name: key_name,
                    verbose: _,
                },
            capacity: key_capacity,
            constraint: _,
        } = problem;

        // Step 1. Collect graph data
        let Graph { edges, nodes } = graph;

        // Step 2. Define a problem
        let key_flow_in = "__flow_out";
        let key_flow_out = "__flow_in";

        // Step 3. Apply edge flows to node capacity
        let updated_nodes = nodes
            .left_join(
                edges
                    .clone()
                    .select([dsl::col(&key_src), dsl::col(&key_flow).alias(key_flow_out)]),
                dsl::col(&key_name),
                dsl::col(&key_src),
            )
            .left_join(
                edges
                    .clone()
                    .select([dsl::col(&key_sink), dsl::col(&key_flow).alias(key_flow_in)]),
                dsl::col(&key_name),
                dsl::col(&key_sink),
            )
            .with_column(
                dsl::col(&key_capacity)
                    .sub(dsl::col(key_flow_out).fill_null(0i64))
                    .add(dsl::col(key_flow_in).fill_null(0i64)),
            )
            .drop([key_flow_in, key_flow_out]);

        // Step 4. Assemble an optimized graph
        let updated_edges = edges.drop([&key_flow]);
        let updated_nodes = updated_nodes;

        Ok(Graph {
            edges: updated_edges,
            nodes: updated_nodes,
        })
    }
}
