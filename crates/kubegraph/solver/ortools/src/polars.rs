use anyhow::{anyhow, bail, Result};
use kubegraph_api::{
    graph::Graph,
    solver::{MaxFlowProblem, MinCostProblem, ProblemConstrait, ProblemMetadata},
};
use or_tools::graph::{
    ebert_graph::{ArcIndex, FlowQuantity, NodeIndex, StarGraph},
    max_flow::{MaxFlow, MaxFlowOutput, MaxFlowStatus},
    min_cost_flow::{MinCostFlow, MinCostFlowOutput, MinCostFlowStatus},
};
use polars::{
    datatypes::DataType,
    frame::DataFrame,
    lazy::{dsl, frame::LazyFrame},
    series::Series,
};

impl ::kubegraph_api::solver::LocalSolver<Graph<DataFrame>, String> for super::Solver {
    type Output = Graph<LazyFrame>;

    fn step_max_flow(
        &self,
        graph: Graph<DataFrame>,
        problem: MaxFlowProblem<String>,
    ) -> Result<Self::Output> {
        ::kubegraph_api::solver::LocalSolver::<Graph<LazyFrame>, String>::step_max_flow(
            self,
            graph.into(),
            problem,
        )
    }

    fn step_min_cost(
        &self,
        graph: Graph<DataFrame>,
        problem: MinCostProblem<String>,
    ) -> Result<Self::Output> {
        ::kubegraph_api::solver::LocalSolver::<Graph<LazyFrame>, String>::step_min_cost(
            self,
            graph.into(),
            problem,
        )
    }
}

impl ::kubegraph_api::solver::LocalSolver<Graph<LazyFrame>, String> for super::Solver {
    type Output = Graph<LazyFrame>;

    fn step_max_flow(
        &self,
        graph: Graph<LazyFrame>,
        problem: MaxFlowProblem<String>,
    ) -> Result<Self::Output> {
        let MaxFlowProblem {
            metadata:
                ProblemMetadata {
                    name: key_name,
                    sink: key_sink,
                    src: key_src,
                    verbose,
                },
            capacity: key_capacity,
        } = problem;

        // Step 1. Collect graph data
        let Graph {
            edges: src_edges,
            nodes: src_nodes,
        } = graph;
        let edges = src_edges
            .clone()
            .select([
                dsl::col(&key_src),
                dsl::col(&key_sink),
                dsl::col(&key_capacity),
            ])
            .collect()
            .map_err(|error| anyhow!("failed to collect edges input: {error}"))?;
        let nodes = src_nodes
            .clone()
            .select([dsl::col(&key_name)])
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes input: {error}"))?;

        // Step 2. Collect edges
        let src = get_column(&edges, "edge", "src", &key_src, &DataType::Int32)?;
        let sink = get_column(&edges, "edge", "sink", &key_sink, &DataType::Int32)?;
        let capacity = get_column(&edges, "edge", "capacity", &key_capacity, &DataType::Int64)?;

        // Step 3. Collect nodes
        let name = get_column(&nodes, "node", "name", &key_name, &DataType::Int32)?;

        // Step 4. Describe about the graph
        let num_nodes = name.len() as NodeIndex;
        let num_edges = capacity.len() as ArcIndex;

        // TODO: Add special nodes: start, end
        let problem_src = 0;
        let problem_sink = num_nodes - 1;

        // Step 5. Define a problem
        let mut solver_graph = StarGraph::new(num_nodes, num_edges);
        for (src, sink) in src.iter().zip(sink.iter()) {
            solver_graph.add_arc(src.try_extract()?, sink.try_extract()?);
        }

        let mut solver = MaxFlow::new(&solver_graph, problem_src, problem_sink);
        for (index, capacity) in capacity.iter().enumerate() {
            solver.set_arc_capacity(index as ArcIndex, capacity.try_extract()?);
        }

        if verbose {
            println!("Solving max flow with: {num_nodes} nodes, and {num_edges} edges.");
        }

        // Step 6. Find the maximum flow between node 0 and node 4.
        let output = solver
            .solve()
            .ok_or_else(|| anyhow!("failed to solve maximum flow"))?;
        if output.status() != MaxFlowStatus::Optimal {
            bail!("solving the max flow is not optimal!");
        }

        // Step 7. Collect outputs
        let flow = output.collect_flow(num_edges);

        // Step 8. Assemble an optimized graph
        let optimized_edges = src_edges
            .with_column(dsl::lit(src))
            .with_column(dsl::lit(sink))
            .with_column(dsl::lit(capacity))
            .with_column(dsl::lit(flow));
        let optimized_nodes = src_nodes.with_column(dsl::lit(name));

        Ok(Graph {
            edges: optimized_edges,
            nodes: optimized_nodes,
        })
    }

    fn step_min_cost(
        &self,
        graph: Graph<LazyFrame>,
        problem: MinCostProblem<String>,
    ) -> Result<Self::Output> {
        let MinCostProblem {
            metadata:
                ProblemMetadata {
                    name: key_name,
                    sink: key_sink,
                    src: key_src,
                    verbose,
                },
            capacity: key_capacity,
            constraint:
                ProblemConstrait {
                    cost: key_cost,
                    supply: key_supply,
                },
        } = problem;

        // Step 1. Collect graph data
        let Graph {
            edges: src_edges,
            nodes: src_nodes,
        } = graph;
        let edges = src_edges
            .clone()
            .select([
                dsl::col(&key_src),
                dsl::col(&key_sink),
                dsl::col(&key_capacity),
                dsl::col(&key_cost),
            ])
            .collect()
            .map_err(|error| anyhow!("failed to collect edges input: {error}"))?;
        let nodes = src_nodes
            .clone()
            .select([dsl::col(&key_name), dsl::col(&key_supply)])
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes input: {error}"))?;

        // Step 2. Collect edges
        let src = get_column(&edges, "edge", "src", &key_src, &DataType::Int32)?;
        let sink = get_column(&edges, "edge", "sink", &key_sink, &DataType::Int32)?;
        let capacity = get_column(&edges, "edge", "capacity", &key_capacity, &DataType::Int64)?;
        let cost = get_column(&edges, "edge", "cost", &key_cost, &DataType::Int64)?;

        // Step 3. Collect nodes
        let name = get_column(&nodes, "node", "name", &key_name, &DataType::Int32)?;
        let supply = get_column(&nodes, "node", "supply", &key_supply, &DataType::Int64)?;

        // Step 4. Describe about the graph
        let num_nodes = name.len() as NodeIndex;
        let num_edges = capacity.len() as ArcIndex;

        // Step 5. Define a problem
        let mut solver_graph = StarGraph::new(num_nodes, num_edges);
        for (src, sink) in src.iter().zip(sink.iter()) {
            solver_graph.add_arc(src.try_extract()?, sink.try_extract()?);
        }

        let mut solver = MinCostFlow::new(&solver_graph);
        for (index, (capacity, cost)) in capacity
            .iter()
            .zip(cost.iter())
            .enumerate()
            .map(|(index, value)| (index as ArcIndex, value))
        {
            solver.set_arc_capacity(index, capacity.try_extract()?);
            solver.set_arc_unit_cost(index, cost.try_extract()?);
        }
        for (index, supply) in supply
            .iter()
            .enumerate()
            .map(|(index, value)| (index as NodeIndex, value))
        {
            solver.set_node_supply(index, supply.try_extract()?);
        }

        if verbose {
            println!("Solving min cost flow with: {num_nodes} nodes, and {num_edges} edges.");
        }

        // Step 6. Find the maximum flow between node 0 and node 4.
        let output = solver
            .solve()
            .ok_or_else(|| anyhow!("failed to solve minimum cost flow"))?;
        if output.status() != MinCostFlowStatus::Optimal {
            bail!("solving the min cost flow is not optimal!");
        }

        // Step 7. Collect outputs
        let flow = output.collect_flow(num_edges);

        // Step 8. Assemble an optimized graph
        let optimized_edges = src_edges
            .with_column(dsl::lit(src))
            .with_column(dsl::lit(sink))
            .with_column(dsl::lit(capacity))
            .with_column(dsl::lit(cost))
            .with_column(dsl::lit(flow));
        let optimized_nodes = src_nodes
            .with_column(dsl::lit(name))
            .with_column(dsl::lit(supply));

        Ok(Graph {
            edges: optimized_edges,
            nodes: optimized_nodes,
        })
    }
}

fn get_column(
    df: &DataFrame,
    kind: &str,
    key: &str,
    name: &str,
    dtype: &DataType,
) -> Result<Series> {
    df.column(name)
        .map_err(|error| anyhow!("failed to get {kind} {key} column: {error}"))?
        .cast(dtype)
        .map_err(|error| anyhow!("failed to sort {kind} {key} column: {error}"))
}

trait CollectFlow {
    fn collect_flow(&self, num_edges: ArcIndex) -> Series {
        Series::from_iter((0..num_edges).map(|index| self.get_flow(index))).with_name("flow")
    }

    fn get_flow(&self, index: ArcIndex) -> FlowQuantity;
}

impl<'graph, 'solver> CollectFlow for MaxFlowOutput<'graph, 'solver> {
    fn get_flow(&self, index: ArcIndex) -> FlowQuantity {
        self.flow(index)
    }
}

impl<'graph, 'solver> CollectFlow for MinCostFlowOutput<'graph, 'solver> {
    fn get_flow(&self, index: ArcIndex) -> FlowQuantity {
        self.flow(index)
    }
}
