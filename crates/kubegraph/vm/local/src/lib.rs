#[cfg(feature = "df-polars")]
extern crate polars as pl;

mod analyzer;
mod function;
mod graph;
mod reloader;
mod resource;
mod runner;
mod solver;
mod visualizer;

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    function::{FunctionMetadata, NetworkFunctionCrd},
    graph::{GraphEdges, GraphScope},
    problem::VirtualProblem,
    resource::NetworkResourceDB,
    visualizer::NetworkVisualizer,
    vm::NetworkVirtualMachineExt,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::{instrument, Level};

use crate::function::NetworkFunctionExt;

#[derive(Clone)]
pub struct NetworkVirtualMachine {
    analyzer: self::analyzer::NetworkAnalyzer,
    graph_db: self::graph::NetworkGraphDB,
    resource_db: self::resource::NetworkResourceDB,
    resource_worker: Arc<Mutex<Option<self::resource::NetworkResourceWorker>>>,
    runner: self::runner::NetworkRunner,
    solver: self::solver::NetworkSolver,
    visualizer: self::visualizer::NetworkVisualizer,
    vm_runner: Arc<Mutex<Option<NetworkVirtualMachineRunner>>>,
}

#[async_trait]
impl ::kubegraph_api::vm::NetworkVirtualMachine for NetworkVirtualMachine {
    type Analyzer = self::analyzer::NetworkAnalyzer;
    type ResourceDB = self::resource::NetworkResourceDB;
    type GraphDB = self::graph::NetworkGraphDB;
    type Runner = self::runner::NetworkRunner;
    type Solver = self::solver::NetworkSolver;
    type Visualizer = self::visualizer::NetworkVisualizer;

    fn analyzer(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::Analyzer {
        &self.analyzer
    }

    fn resource_db(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::ResourceDB {
        &self.resource_db
    }

    fn graph_db(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::GraphDB {
        &self.graph_db
    }

    fn runner(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::Runner {
        &self.runner
    }

    fn solver(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::Solver {
        &self.solver
    }

    fn visualizer(&self) -> &<Self as ::kubegraph_api::vm::NetworkVirtualMachine>::Visualizer {
        &self.visualizer
    }

    fn interval(&self) -> Option<Duration> {
        // TODO: use args instead
        Some(Duration::from_secs(5))
    }

    #[instrument(level = Level::INFO)]
    async fn try_default() -> Result<Self> {
        // Step 1. Initialize components
        let vm = Self {
            analyzer: self::analyzer::NetworkAnalyzer::try_default().await?,
            graph_db: self::graph::NetworkGraphDB::try_default().await?,
            resource_db: self::resource::NetworkResourceDB::default(),
            resource_worker: Arc::new(Mutex::new(None)),
            runner: self::runner::NetworkRunner::try_default().await?,
            solver: self::solver::NetworkSolver::try_default().await?,
            visualizer: self::visualizer::NetworkVisualizer::try_default().await?,
            vm_runner: Arc::new(Mutex::new(None)),
        };

        // Step 2. Spawn workers
        vm.resource_worker
            .lock()
            .await
            .replace(self::resource::NetworkResourceWorker::try_spawn(&vm).await?);
        vm.vm_runner
            .lock()
            .await
            .replace(NetworkVirtualMachineRunner::spawn(vm.clone()));
        Ok(vm)
    }

    #[instrument(level = Level::INFO, skip(self, problem, nodes))]
    async fn infer_edges(
        &self,
        problem: &VirtualProblem,
        nodes: LazyFrame,
    ) -> Result<Option<GraphEdges<LazyFrame>>> {
        // Step 1. Collect all functions
        let functions = self.resource_db.list(()).await.unwrap_or_default();
        if functions.is_empty() {
            return Ok(None);
        }

        // Step 2. Predict all functions' outputs
        let edges = functions
            .into_iter()
            .map(|object: NetworkFunctionCrd| {
                let function = FunctionMetadata {
                    scope: GraphScope::from_resource(&object),
                };

                object.infer_edges(problem, &function, nodes.clone())
            })
            .collect::<Result<GraphEdges<LazyFrame>>>()?;

        // TODO: remove when test is ended
        #[cfg(feature = "df-polars")]
        if problem.spec.verbose {
            use pl::lazy::dsl;

            println!("{}", nodes.clone().try_into_polars()?.collect()?);
            println!(
                "{}",
                edges
                    .clone()
                    .into_inner()
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
        }
        Ok(Some(edges))
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close_workers(&self) -> Result<()> {
        if let Some(worker) = self.resource_worker.lock().await.take() {
            worker.abort();
        }
        if let Some(worker) = self.vm_runner.lock().await.take() {
            worker.abort();
        }
        Ok(())
    }
}

struct NetworkVirtualMachineRunner {
    inner: JoinHandle<()>,
}

impl NetworkVirtualMachineRunner {
    pub(crate) fn spawn(vm: impl 'static + NetworkVirtualMachineExt) -> Self {
        Self {
            inner: ::tokio::spawn(async move { vm.loop_forever().await }),
        }
    }

    pub(crate) fn abort(&self) {
        self.inner.abort()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "df-polars")]
    #[::tokio::test]
    async fn simulate_simple_with_edges() {
        use kubegraph_api::{
            graph::{Graph, GraphData, GraphFilter, GraphMetadata, GraphScope, NetworkGraphDB},
            problem::{ProblemSpec, VirtualProblemAnalyzer},
        };

        // Step 1. Define problems
        let vm = NetworkVirtualMachine::try_default()
            .await
            .expect("failed to init vm");

        // Step 2. Define nodes
        let nodes = ::pl::df!(
            "name"      => [    "a",     "b"],
            "capacity"  => [ 300i64,  300i64],
            "supply"    => [ 300i64,    0i64],
            "unit_cost" => [   5i64,    1i64],
            "warehouse" => [   true,    true],
        )
        .expect("failed to create nodes dataframe");

        // Step 3. Define edges
        let edges = ::pl::df!(
            "src"       => [    "a"],
            "sink"      => [    "b"],
            "capacity"  => [  50i64],
            "unit_cost" => [   1i64],
        )
        .expect("failed to create edges dataframe");

        // Step 4. Register the initial graph
        let scope = GraphScope {
            namespace: "default".into(),
            name: "warehouse".into(),
        };
        let graph = Graph {
            data: GraphData {
                edges: edges.into(),
                nodes: nodes.into(),
            },
            metadata: GraphMetadata::default(),
            scope: scope.clone(),
        };
        vm.graph_db.insert(graph).await.unwrap();

        // Step 4. Add cost & value function (heuristic)
        let problem = VirtualProblem {
            analyzer: VirtualProblemAnalyzer::Empty,
            filter: GraphFilter::all("default".into()),
            scope: GraphScope {
                namespace: "default".into(),
                name: "optimize-warehouses".into(),
            },
            spec: ProblemSpec {
                verbose: true,
                ..Default::default()
            },
        };

        // Step 5. Do optimize
        let n_step = 10;
        for _ in 0..n_step {
            vm.step_with_custom_problem(problem.clone())
                .await
                .expect("failed to optimize")
        }

        // Step 6. Collect the output graph
        let output_graph_scope = GraphScope {
            namespace: "default".into(),
            name: GraphScope::NAME_GLOBAL.into(),
        };
        let Graph {
            data:
                GraphData {
                    edges: output_edges,
                    nodes: output_nodes,
                },
            ..
        } = vm.graph_db.get(&output_graph_scope).await.unwrap().unwrap();
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

        // Step 7. Verify the output graph
        assert_eq!(
            output_nodes,
            ::pl::df!(
                "name"      => [    "a",     "b"],
                "capacity"  => [ 300i64,  300i64],
                "supply"    => [   0i64,  300i64],
                "unit_cost" => [   5i64,    1i64],
                "warehouse" => [   true,    true],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
        assert_eq!(
            output_edges,
            ::pl::df!(
                "src"       => [     "a"],
                "sink"      => [     "b"],
                "capacity"  => [   50i64],
                "unit_cost" => [    1i64],
                "function"  => ["static"],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
    }

    #[cfg(all(feature = "df-polars", feature = "function-dummy"))]
    #[::tokio::test]
    async fn simulate_simple_with_function() {
        use kube::api::ObjectMeta;
        use kubegraph_api::{
            frame::DataFrame,
            function::{
                dummy::NetworkFunctionDummySpec, NetworkFunctionCrd, NetworkFunctionKind,
                NetworkFunctionMetadata, NetworkFunctionSpec,
            },
            graph::{Graph, GraphData, GraphFilter, GraphMetadata, GraphScope, NetworkGraphDB},
            problem::{ProblemSpec, VirtualProblemAnalyzer},
        };

        // Step 1. Define problems
        let vm = NetworkVirtualMachine::try_default()
            .await
            .expect("failed to init vm");

        // Step 2. Define nodes
        let nodes = ::pl::df!(
            "name"      => [    "a",     "b"],
            "capacity"  => [ 300i64,  300i64],
            "supply"    => [ 300i64,    0i64],
            "unit_cost" => [   5i64,    1i64],
            "warehouse" => [   true,    true],
        )
        .expect("failed to create nodes dataframe");

        // Step 3. Register the initial graph
        let scope = GraphScope {
            namespace: "default".into(),
            name: "warehouse".into(),
        };
        let graph = Graph {
            data: GraphData {
                edges: LazyFrame::default(),
                nodes: nodes.into(),
            },
            metadata: GraphMetadata::default(),
            scope: scope.clone(),
        };
        vm.graph_db.insert(graph).await.unwrap();

        // Step 4. Define functions
        let function = NetworkFunctionCrd {
            metadata: ObjectMeta {
                namespace: Some("default".into()),
                name: Some("move".into()),
                ..Default::default()
            },
            spec: NetworkFunctionSpec {
                kind: NetworkFunctionKind::Dummy(NetworkFunctionDummySpec {}),
                metadata: NetworkFunctionMetadata {
                    filter: Some(
                        "src != sink and src.supply > 0 and src.supply > sink.supply".into(),
                    ),
                    script: r"
                    capacity = 50;
                    unit_cost = 1;
                "
                    .into(),
                },
            },
        };
        vm.resource_db.insert(function).await;

        // Step 5. Add cost & value function (heuristic)
        let problem = VirtualProblem {
            analyzer: VirtualProblemAnalyzer::Empty,
            filter: GraphFilter::all("default".into()),
            scope: GraphScope {
                namespace: "default".into(),
                name: "optimize-warehouses".into(),
            },
            spec: ProblemSpec {
                verbose: true,
                ..Default::default()
            },
        };

        // Step 6. Do optimize
        let n_step = 10;
        for _ in 0..n_step {
            vm.step_with_custom_problem(problem.clone())
                .await
                .expect("failed to optimize")
        }

        // Step 7. Collect the output graph
        let output_graph_scope = GraphScope {
            namespace: "default".into(),
            name: GraphScope::NAME_GLOBAL.into(),
        };
        let Graph {
            data:
                GraphData {
                    edges: output_edges,
                    nodes: output_nodes,
                },
            ..
        } = vm.graph_db.get(&output_graph_scope).await.unwrap().unwrap();
        let output_nodes = output_nodes
            .try_into_polars()
            .unwrap()
            .collect()
            .expect("failed to collect output nodes dataframe");

        println!("{output_nodes}");

        // Step 7. Verify the output graph
        assert_eq!(
            output_nodes,
            ::pl::df!(
                "name"      => [    "a",     "b"],
                "capacity"  => [ 300i64,  300i64],
                "supply"    => [ 150i64,  150i64],
                "unit_cost" => [   5i64,    1i64],
                "warehouse" => [   true,    true],
            )
            .expect("failed to create ground-truth nodes dataframe"),
        );
        assert_eq!(output_edges.collect().await.unwrap(), DataFrame::Empty);
    }
}
