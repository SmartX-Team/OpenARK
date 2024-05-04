#[cfg(feature = "polars")]
extern crate polars as pl;

mod ctx;
mod func;
mod lazy;

use std::{
    borrow::Borrow,
    collections::{btree_map::Entry, BTreeMap},
    fmt,
};

use anyhow::{anyhow, Result};
use kubegraph_api::{
    frame::{IntoLazyFrame, LazyFrame},
    graph::Graph,
    solver::{LocalSolver, Problem, ProblemConstrait},
    twin::LocalTwin,
};

pub use self::func::IntoFunction;
use self::{ctx::Context, func::Function};

#[derive(Default)]
pub struct VirtualMachine<K, S, T> {
    contexts: BTreeMap<K, Context>,
    functions: BTreeMap<K, Function>,
    solver: S,
    twin: T,
}

impl<K, S, T> VirtualMachine<K, S, T>
where
    K: Ord,
{
    pub fn get_graph<Q>(&self, key: &Q) -> Result<Graph<LazyFrame>>
    where
        K: Borrow<Q> + Ord,
        Q: ?Sized + fmt::Debug + Ord,
    {
        self.contexts
            .get(key)
            .ok_or_else(|| anyhow!("undefined graph: {key:?}"))
            .and_then(|context| context.to_graph())
    }

    pub fn insert_graph(&mut self, key: K, graph: Graph<LazyFrame>) {
        let Graph { edges, nodes } = graph;
        let graph = Graph {
            edges: Some(edges),
            nodes: Some(nodes),
        };

        match self.contexts.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph = graph,
            Entry::Vacant(ctx) => {
                ctx.insert(Context {
                    graph,
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_edges(&mut self, key: K, edges: impl IntoLazyFrame) {
        let edges = Some(edges.into());
        match self.contexts.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph.edges = edges,
            Entry::Vacant(ctx) => {
                ctx.insert(Context {
                    graph: Graph { edges, nodes: None },
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_nodes(&mut self, key: K, nodes: impl IntoLazyFrame) {
        let nodes = Some(nodes.into());
        match self.contexts.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph.nodes = nodes,
            Entry::Vacant(ctx) => {
                ctx.insert(Context {
                    graph: Graph { edges: None, nodes },
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_function(&mut self, key: K, function: impl IntoFunction) -> Result<()> {
        let function = function.try_into()?;
        self.functions.insert(key, function);
        Ok(())
    }

    pub fn insert_script(&mut self, key: K, script: &str) -> Result<()> {
        self.contexts
            .entry(key)
            .or_insert_with(Default::default)
            .vm
            .execute_script(script)
    }
}

impl<K, S, T> VirtualMachine<K, S, T>
where
    K: Ord,
    S: LocalSolver<Graph<LazyFrame>, String, Output = Graph<LazyFrame>>,
    T: LocalTwin<Graph<LazyFrame>, String, Output = Graph<LazyFrame>>,
{
    pub fn step<P>(&mut self, key: K, problem: &Problem<P>) -> Result<()>
    where
        K: fmt::Debug,
        P: ToString,
    {
        let Problem {
            metadata,
            capacity,
            constraint,
        } = problem;
        let problem = Problem {
            metadata: metadata.clone(),
            capacity: capacity.to_string(),
            constraint: constraint
                .as_ref()
                .map(|ProblemConstrait { cost, supply }| ProblemConstrait {
                    cost: cost.to_string(),
                    supply: supply.to_string(),
                }),
        };

        self.step_with_solver(&key, &problem)
            .and_then(|graph| self.twin.execute(graph, &problem))
            .map(|graph| self.insert_graph(key, graph))
    }

    fn step_with_solver(&self, key: &K, problem: &Problem<String>) -> Result<Graph<LazyFrame>>
    where
        K: fmt::Debug,
    {
        let context = self
            .contexts
            .get(key)
            .ok_or_else(|| anyhow!("failed to get context: {key:?}"))?;

        // TODO: use functions as markov blankets

        let graph = context.to_graph()?;
        let optimized_graph = self.solver.step(graph, problem.clone())?;
        Ok(optimized_graph)
    }
}

#[cfg(test)]
mod tests {
    use kubegraph_api::solver::ProblemMetadata;
    use kubegraph_solver_ortools::Solver;
    use kubegraph_twin_simulator::Twin;

    use crate::func::FunctionTemplate;

    use super::*;

    #[cfg(feature = "polars")]
    #[test]
    fn simulate_simple() {
        use pl::{prelude::NamedFromOwned, series::Series};

        // Step 1. Define problems
        let mut vm = VirtualMachine::<_, Solver, Twin>::default();

        // Step 2. Add nodes
        let nodes = ::pl::df!(
            "name" => &["a", "b"],
            "payload" => &[300, 0],
            "warehouse" => &[true, true],
        )
        .expect("failed to create nodes dataframe");
        vm.insert_nodes("warehouse", nodes);

        // Step 3. Add nodes
        let edges = ::pl::df!(
            "src" => &["a"],
            "sink" => &["b"],
            "payload" => &[100],
        )
        .expect("failed to create edges dataframe");
        vm.insert_edges("warehouse", edges.clone());

        // Step 4. Add functions
        let function = FunctionTemplate {
            action: r"
                src.payload = -3;
                sink.payload = +3;

                src.traffic = 3;
                src.traffic_out = 3;
                sink.traffic = 3;
                sink.traffic_in = 3;
            ",
            filter: Some("src.payload >= 3"),
        };
        vm.insert_function("move", function)
            .expect("failed to insert function");

        // Step 5. Add cost & value function (heuristic)
        let problem = Problem {
            metadata: ProblemMetadata {
                verbose: true,
                ..Default::default()
            },
            capacity: "payload".to_string(),
            constraint: None,
        };

        // Step 6. Do optimize
        let n_step = 3;
        for _ in 0..n_step {
            vm.step("warehouse".into(), &problem)
                .expect("failed to optimize")
        }

        // Step 7. Collect the output graph
        let Graph {
            edges: output_edges,
            nodes: output_nodes,
        } = vm.get_graph("warehouse").unwrap();
        let output_edges = output_edges
            .try_into_polars()
            .unwrap()
            .collect()
            .expect("failed to collect output edges dataframe");
        let output_nodes = output_nodes
            .try_into_polars()
            .unwrap()
            .collect()
            .expect("failed to collect output nodes dataframe");

        println!("{output_nodes}");
        println!("{output_edges}");

        // Step 8. Verify the output nodes
        assert_eq!(
            output_nodes,
            ::pl::df!(
                "name" => &["a", "b"],
                "payload" => &[0, 300],
                "warehouse" => &[true, true],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
        assert_eq!(output_edges, edges);
    }
}
