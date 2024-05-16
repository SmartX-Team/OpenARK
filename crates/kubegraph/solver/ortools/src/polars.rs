use anyhow::{anyhow, bail, Result};
use kubegraph_api::{
    frame::polars::{find_indices, get_column},
    graph::{Graph, NetworkGraphMetadata},
    problem::ProblemSpec,
};
use or_tools::graph::{
    ebert_graph::{ArcIndex, FlowQuantity, NodeIndex, StarGraph},
    min_cost_flow::{MinCostFlow, MinCostFlowOutput, MinCostFlowStatus},
};
use pl::{
    datatypes::DataType,
    frame::DataFrame,
    lazy::{dsl, frame::LazyFrame},
    series::Series,
};

impl ::kubegraph_api::solver::LocalSolver<Graph<DataFrame>> for super::Solver {
    type Output = Graph<LazyFrame>;

    fn step(&self, graph: Graph<DataFrame>, problem: ProblemSpec) -> Result<Self::Output> {
        ::kubegraph_api::solver::LocalSolver::<Graph<LazyFrame>>::step(self, graph.into(), problem)
    }
}

impl ::kubegraph_api::solver::LocalSolver<Graph<LazyFrame>> for super::Solver {
    type Output = Graph<LazyFrame>;

    fn step(&self, graph: Graph<LazyFrame>, problem: ProblemSpec) -> Result<Self::Output> {
        let ProblemSpec {
            metadata:
                NetworkGraphMetadata {
                    capacity: key_capacity,
                    flow: key_flow,
                    function: _,
                    name: key_name,
                    sink: key_sink,
                    src: key_src,
                    supply: key_supply,
                    unit_cost: key_unit_cost,
                },
            verbose,
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
                dsl::col(&key_unit_cost),
            ])
            .collect()
            .map_err(|error| anyhow!("failed to collect edges input: {error}"))?;
        let nodes = src_nodes
            .clone()
            .select([
                dsl::col(&key_name),
                dsl::col(&key_capacity),
                dsl::col(&key_unit_cost),
                dsl::col(&key_supply),
            ])
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes input: {error}"))?;

        // Step 2. Collect edges
        let src = get_column(&edges, "edge", "src", &key_src, None)?;
        let sink = get_column(&edges, "edge", "sink", &key_sink, None)?;
        let edge_capacity = get_column(
            &edges,
            "edge",
            "capacity",
            &key_capacity,
            Some(&DataType::Int64),
        )?;
        let edge_cost = get_column(
            &edges,
            "edge",
            "cost",
            &key_unit_cost,
            Some(&DataType::Int64),
        )?;

        // Step 3. Collect nodes
        let name = get_column(&nodes, "node", "name", &key_name, None)?;
        let node_capacity = get_column(
            &nodes,
            "node",
            "capacity",
            &key_capacity,
            Some(&DataType::Int64),
        )?;
        let node_cost = get_column(
            &nodes,
            "node",
            "cost",
            &key_unit_cost,
            Some(&DataType::Int64),
        )?;
        let node_supply = get_column(
            &nodes,
            "node",
            "supply",
            &key_supply,
            Some(&DataType::Int64),
        )?;
        let node_supply_sum = node_supply
            .sum()
            .map_err(|error| anyhow!("failed to collect node supplies: {error}"))?;

        // Step 4. Map name indices: src, sink
        let src_map = find_indices(&key_name, &name, &src)?;
        let sink_map = find_indices(&key_name, &name, &sink)?;

        let src_map_fallback = src_map.clone().unwrap_or_else(|| src.clone());
        let sink_map_fallback = sink_map.clone().unwrap_or_else(|| sink.clone());

        // Step 5. Describe about the graph
        let num_nodes = name.len() as NodeIndex;
        let num_edges = edge_capacity.len() as ArcIndex;

        // Do not optimize empty graph
        if num_nodes == 0 || num_edges == 0 {
            // Step 9. Assemble an optimized graph
            let unoptimized_edges = src_edges;
            let unoptimized_edges = match (src_map, sink_map) {
                (None, None) => unoptimized_edges
                    .with_column(dsl::lit(src))
                    .with_column(dsl::lit(sink)),
                _ => unoptimized_edges,
            };
            let optimized_edges = unoptimized_edges.with_columns([
                dsl::lit(edge_capacity),
                dsl::lit(edge_cost),
                dsl::lit(0i64).alias(&key_flow),
            ]);
            let optimized_nodes = src_nodes.with_columns([
                dsl::lit(name),
                dsl::lit(node_capacity),
                dsl::lit(node_cost),
                dsl::lit(node_supply),
            ]);

            return Ok(Graph {
                edges: optimized_edges,
                nodes: optimized_nodes,
            });
        }

        let num_nodes_special = 2;
        let num_nodes_with_special = num_nodes + num_nodes_special;
        let num_edges_with_special = num_edges + num_nodes * 2;

        // Step 6. Define a problem
        let mut solver_graph = StarGraph::new(num_nodes_with_special, num_edges_with_special);
        for (src, sink) in src_map_fallback.iter().zip(sink_map_fallback.iter()) {
            solver_graph.add_arc(src.try_extract()?, sink.try_extract()?);
        }
        for node in 0..num_nodes {
            solver_graph.add_arc(num_nodes, node);
            solver_graph.add_arc(node, num_nodes + 1);
        }

        let mut solver = MinCostFlow::new(&solver_graph);
        for (index, (capacity, cost)) in edge_capacity
            .iter()
            .zip(edge_cost.iter())
            .enumerate()
            .map(|(index, value)| (index as ArcIndex, value))
        {
            solver.set_arc_capacity(index, capacity.try_extract()?);
            solver.set_arc_unit_cost(index, cost.try_extract()?);
        }

        if verbose {
            println!("Solving min cost flow with: {num_nodes} nodes, and {num_edges} edges.");
        }

        // Step 7. Add special nodes
        let node_index_src = num_nodes;
        let node_index_sink = num_nodes + 1;
        solver.set_node_supply(node_index_src, node_supply_sum);
        solver.set_node_supply(node_index_sink, -node_supply_sum);

        // Step 8. Add special edges
        for (offset, ((cost, capacity), supply)) in node_cost
            .iter()
            .zip(node_capacity.iter())
            .zip(node_supply.iter())
            .enumerate()
            .map(|(node, value)| ((2 * node) as ArcIndex, value))
        {
            solver.set_arc_capacity(num_edges + offset, supply.try_extract()?);
            solver.set_arc_capacity(num_edges + offset + 1, capacity.try_extract()?);
            solver.set_arc_unit_cost(num_edges + offset + 1, cost.try_extract()?);
        }

        // Step 9. Find the maximum flow between node 0 and node 4.
        let output = solver
            .solve()
            .ok_or_else(|| anyhow!("failed to solve minimum cost flow"))?;
        if output.status() != MinCostFlowStatus::Optimal {
            bail!("solving the min cost flow is not optimal!");
        }

        // Step 8. Collect outputs
        let flow = output.collect_flow(&key_flow, num_edges);

        // Step 9. Assemble an optimized graph
        let optimized_edges = src_edges;
        let optimized_edges = match (src_map, sink_map) {
            (None, None) => optimized_edges
                .with_column(dsl::lit(src))
                .with_column(dsl::lit(sink)),
            _ => optimized_edges,
        };
        let optimized_edges = optimized_edges.with_columns([
            dsl::lit(edge_capacity),
            dsl::lit(edge_cost),
            dsl::lit(flow),
        ]);
        let optimized_nodes = src_nodes.with_columns([
            dsl::lit(name),
            dsl::lit(node_capacity),
            dsl::lit(node_cost),
            dsl::lit(node_supply),
        ]);

        Ok(Graph {
            edges: optimized_edges,
            nodes: optimized_nodes,
        })
    }
}

trait CollectFlow {
    fn collect_flow(&self, name: &str, num_edges: ArcIndex) -> Series {
        Series::from_iter((0..num_edges).map(|index| self.get_flow(index))).with_name(name)
    }

    fn get_flow(&self, index: ArcIndex) -> FlowQuantity;
}

impl<'graph, 'solver> CollectFlow for MinCostFlowOutput<'graph, 'solver> {
    fn get_flow(&self, index: ArcIndex) -> FlowQuantity {
        self.flow(index)
    }
}
