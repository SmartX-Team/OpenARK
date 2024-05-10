#[cfg(feature = "df-polars")]
extern crate polars as pl;

mod func;
mod graph;
mod lazy;

use std::{
    borrow::Borrow,
    collections::{btree_map::Entry, BTreeMap},
    fmt,
};

use anyhow::{anyhow, Result};
use kubegraph_api::{
    frame::{IntoLazyFrame, LazyFrame},
    func::FunctionMetadata,
    graph::{Graph, GraphEdges, IntoGraph},
    solver::{LocalSolver, Problem},
    twin::LocalTwin,
    vm::Script,
};

pub use self::func::IntoFunction;
use self::{
    func::{Function, FunctionContext},
    graph::GraphContext,
};

#[derive(Default)]
pub struct VirtualMachine<K, S, T> {
    functions: BTreeMap<K, FunctionContext>,
    graphs: BTreeMap<K, GraphContext>,
    solver: S,
    twin: T,
}

impl<K, S, T> VirtualMachine<K, S, T>
where
    K: Ord,
{
    pub fn dump_function<Q>(&self, key: &Q) -> Result<Script>
    where
        K: Borrow<Q> + Ord,
        Q: ?Sized + fmt::Debug + Ord,
    {
        self.get_function(key).map(|func| func.dump_script())
    }

    pub fn get_graph<Q>(&self, key: &Q) -> Result<Graph<LazyFrame>>
    where
        K: Borrow<Q> + Ord,
        Q: ?Sized + fmt::Debug + Ord,
    {
        self.graphs
            .get(key)
            .ok_or_else(|| anyhow!("undefined graph: {key:?}"))
            .cloned()
            .and_then(IntoGraph::try_into_graph)
    }

    pub fn get_function<Q>(&self, key: &Q) -> Result<&Function>
    where
        K: Borrow<Q> + Ord,
        Q: ?Sized + fmt::Debug + Ord,
    {
        self.functions
            .get(key)
            .ok_or_else(|| anyhow!("undefined function: {key:?}"))
            .map(|ctx| &ctx.func)
    }

    pub fn insert_graph(&mut self, key: K, graph: Graph<LazyFrame>) {
        let Graph { edges, nodes } = graph;
        let graph = Graph {
            edges: Some(edges),
            nodes: Some(nodes),
        };

        match self.graphs.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph = graph,
            Entry::Vacant(ctx) => {
                ctx.insert(GraphContext {
                    graph,
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_edges(&mut self, key: K, edges: impl IntoLazyFrame) {
        let edges = Some(edges.into());
        match self.graphs.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph.edges = edges,
            Entry::Vacant(ctx) => {
                ctx.insert(GraphContext {
                    graph: Graph { edges, nodes: None },
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_nodes(&mut self, key: K, nodes: impl IntoLazyFrame) {
        let nodes = Some(nodes.into());
        match self.graphs.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().graph.nodes = nodes,
            Entry::Vacant(ctx) => {
                ctx.insert(GraphContext {
                    graph: Graph { edges: None, nodes },
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_function(&mut self, key: K, function: impl IntoFunction) -> Result<()> {
        let function = function.try_into()?;
        self.functions.insert(key, FunctionContext::new(function));
        Ok(())
    }
}

impl<K, S, T> VirtualMachine<K, S, T>
where
    K: Ord,
    S: LocalSolver<Graph<LazyFrame>, String, Output = Graph<LazyFrame>>,
    T: LocalTwin<Graph<LazyFrame>, String, Output = LazyFrame>,
{
    pub fn step<P>(&mut self, key: K, problem: &Problem<P>) -> Result<()>
    where
        K: fmt::Debug + ToString,
        P: ToString,
    {
        let Problem {
            metadata,
            capacity,
            cost,
            supply,
        } = problem;
        let problem = Problem {
            metadata: metadata.clone(),
            capacity: capacity.to_string(),
            cost: cost.to_string(),
            supply: supply.to_string(),
        };

        self.step_with_solver(&key, &problem)
            .and_then(|graph| self.twin.execute(graph, &problem))
            .map(|nodes| self.insert_nodes(key, nodes))
    }

    fn step_with_solver(&self, key: &K, problem: &Problem<String>) -> Result<Graph<LazyFrame>>
    where
        K: fmt::Debug + ToString,
    {
        // Step 1. Retrieve a proper graph
        let Graph { edges, nodes } = self
            .graphs
            .get(key)
            .ok_or_else(|| anyhow!("failed to get graph: {key:?}"))
            .cloned()
            .and_then(IntoGraph::try_into_graph)?;

        // Step 2. Predict all functions' outputs
        let edges = self
            .functions
            .iter()
            .map(|(name, ctx)| {
                let function = FunctionMetadata {
                    name: name.to_string(),
                };

                ctx.func.infer_edges(problem, &function, nodes.clone())
            })
            .chain({
                let function = FunctionMetadata {
                    name: FunctionMetadata::NAME_STATIC.into(),
                };

                match edges {
                    LazyFrame::Empty => None,
                    mut edges => Some(
                        edges
                            .alias(&problem.metadata.function, &function)
                            .map(|()| GraphEdges::new(edges)),
                    ),
                }
            })
            .collect::<Result<GraphEdges<LazyFrame>>>()?
            .into_inner();

        use pl::lazy::dsl;
        println!("{}", nodes.clone().try_into_polars()?.collect()?);
        println!(
            "{}",
            edges
                .clone()
                .try_into_polars()?
                .select([
                    dsl::col("src"),
                    dsl::col("sink"),
                    dsl::col("capacity"),
                    dsl::col("unit_cost"),
                    dsl::col("function"),
                ])
                .collect()?
        );

        // Step 3. Call a solver
        let graph = Graph { edges, nodes };
        self.solver.step(graph, problem.clone())
    }
}

#[cfg(test)]
mod tests {
    use kubegraph_api::solver::ProblemMetadata;
    use kubegraph_solver_ortools::Solver;
    use kubegraph_twin_simulator::Twin;

    use crate::func::FunctionTemplate;

    use super::*;

    #[cfg(feature = "df-polars")]
    #[test]
    fn simulate_simple_with_edges() {
        // Step 1. Define problems
        let mut vm = VirtualMachine::<_, Solver, Twin>::default();

        // Step 2. Add nodes
        let nodes = ::pl::df!(
            "name"      => [ "a",  "b"],
            "capacity"  => [ 300,  300],
            "supply"    => [ 300,    0],
            "unit_cost" => [   5,    1],
            "warehouse" => [true, true],
        )
        .expect("failed to create nodes dataframe");
        vm.insert_nodes("warehouse", nodes);

        // Step 3. Add edges
        let edges = ::pl::df!(
            "src"       => [ "a"],
            "sink"      => [ "b"],
            "capacity"  => [  50],
            "unit_cost" => [   1],
        )
        .expect("failed to create edges dataframe");
        vm.insert_edges("warehouse", edges.clone());

        // Step 4. Add cost & value function (heuristic)
        let problem = Problem {
            metadata: ProblemMetadata {
                verbose: true,
                ..Default::default()
            },
            capacity: "capacity".to_string(),
            cost: "unit_cost".into(),
            supply: "supply".into(),
        };

        // Step 5. Do optimize
        let n_step = 10;
        for _ in 0..n_step {
            vm.step("warehouse".into(), &problem)
                .expect("failed to optimize")
        }

        // Step 6. Collect the output graph
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

        // Step 7. Verify the output nodes
        assert_eq!(
            output_nodes,
            ::pl::df!(
                "name"      => [ "a",  "b"],
                "capacity"  => [ 300,  300],
                "supply"    => [   0,  300],
                "unit_cost" => [   5,    1],
                "warehouse" => [true, true],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
        assert_eq!(
            output_edges,
            ::pl::df!(
                "src"       => ["a"],
                "sink"      => ["b"],
                "capacity"  => [ 50],
                "unit_cost" => [  1],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
    }

    #[cfg(feature = "df-polars")]
    #[test]
    fn simulate_simple_with_function() {
        // Step 1. Define problems
        let mut vm = VirtualMachine::<_, Solver, Twin>::default();

        // Step 2. Add nodes
        let nodes = ::pl::df!(
            "name"      => [ "a",  "b"],
            "capacity"  => [ 300,  300],
            "supply"    => [ 300,    0],
            "unit_cost" => [   5,    1],
            "warehouse" => [true, true],
        )
        .expect("failed to create nodes dataframe");
        vm.insert_nodes("warehouse", nodes);

        // Step 3. Add functions
        let function = FunctionTemplate {
            action: r"
                capacity = 50;
                unit_cost = 1;
            ",
            filter: Some("src != sink and src.supply > 0 and src.supply > sink.supply"),
        };
        vm.insert_function("move", function)
            .expect("failed to insert function");

        // Step 4. Add cost & value function (heuristic)
        let problem = Problem {
            metadata: ProblemMetadata {
                verbose: true,
                ..Default::default()
            },
            capacity: "capacity".to_string(),
            cost: "unit_cost".into(),
            supply: "supply".into(),
        };

        // Step 5. Do optimize
        let n_step = 10;
        for _ in 0..n_step {
            vm.step("warehouse".into(), &problem)
                .expect("failed to optimize")
        }

        // Step 6. Collect the output graph
        let Graph {
            edges: _,
            nodes: output_nodes,
        } = vm.get_graph("warehouse").unwrap();
        let output_nodes = output_nodes
            .try_into_polars()
            .unwrap()
            .collect()
            .expect("failed to collect output nodes dataframe");

        println!("{output_nodes}");

        // Step 7. Verify the output nodes
        assert_eq!(
            output_nodes,
            ::pl::df!(
                "name"      => [ "a",  "b"],
                "capacity"  => [ 300,  300],
                "supply"    => [ 150,  150],
                "unit_cost" => [   5,    1],
                "warehouse" => [true, true],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
    }
}
