extern crate polars as pl;

use or_tools::graph::{
    ebert_graph::{ArcIndex, NodeIndex, StarGraph},
    max_flow::{MaxFlow, MaxFlowStatus},
    min_cost_flow::{MinCostFlow, MinCostFlowStatus},
};
use pl::{
    chunked_array::ops::SortOptions,
    datatypes::DataType,
    df,
    lazy::{self, frame::IntoLazy},
    series::Series,
};

#[test]
fn max_flow() {
    // Step 1. Add edges
    let edges = df!(
        "src"      => [ 0,  0,  0,  1,  1,  2,  2,  3,  3],
        "sink"     => [ 1,  2,  3,  2,  4,  3,  4,  2,  4],
        "capacity" => [20, 30, 10, 40, 30, 10, 20,  5, 20],
    )
    .expect("failed to create edges dataframe");

    let edges_src = edges
        .column("src")
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap()
        .take_materialized_series();
    let edges_sink = edges
        .column("sink")
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap()
        .take_materialized_series();
    let edges_capacity = edges
        .column("capacity")
        .unwrap()
        .cast(&DataType::Int64)
        .unwrap()
        .take_materialized_series();

    // Step 2. Add nodes
    let nodes_name = edges_src
        .clone()
        .extend(&edges_sink)
        .expect("failed to merge src nodes and sink nodes")
        .unique()
        .expect("failed to select unique node names")
        .sort(SortOptions::default())
        .expect("failed to sort node names");
    // let nodes = df!(
    //     "name" => &nodes_name,
    // )
    // .expect("failed to create nodes dataframe");

    let num_nodes = nodes_name.len() as NodeIndex;
    let num_edges = edges_capacity.len() as ArcIndex;

    // Step 3. Define a problem
    let mut graph = StarGraph::new(num_nodes, num_edges);
    for (src, sink) in edges_src.iter().zip(edges_sink.iter()) {
        graph.add_arc(src.try_extract().unwrap(), sink.try_extract().unwrap());
    }

    let mut max_flow = MaxFlow::new(&graph, 0 /* node 0 */, num_nodes - 1 /* node 4 */);
    for (edge, capacity) in edges_capacity.iter().enumerate() {
        max_flow.set_arc_capacity(edge as ArcIndex, capacity.try_extract().unwrap());
    }

    println!(
        "Solving max flow with: {num_nodes} nodes, and {num_edges} edges.",
        num_nodes = graph.num_nodes(),
        num_edges = graph.num_arcs(),
    );

    // Step 4. Find the maximum flow between node 0 and node 4.
    let output = max_flow.solve().expect("failed to solve maximum flow");
    if output.status() != MaxFlowStatus::Optimal {
        eprintln!("Solving the max flow is not optimal!");
    }

    let total_flow = output.get_optimal_flow();
    println!("Total flow: {total_flow}");
    assert_eq!(total_flow, 60);

    let edges_flow =
        Series::from_iter((0..edges_capacity.len()).map(|edge| output.flow(edge as ArcIndex)))
            .with_name("flow".into());

    let mut optimized_edges = edges.clone();
    optimized_edges
        .with_column(edges_flow)
        .expect("failed to attach flow column");

    println!();
    println!("{optimized_edges}");
}

#[test]
fn min_cost_flow() {
    // Step 1. Add edges
    let edges = df!(
        "src"       => [ 0,  0,  1,  1,  1,  2,  2,  3,  4],
        "sink"      => [ 1,  2,  2,  3,  4,  3,  4,  4,  2],
        "capacity"  => [15,  8, 20,  4, 10, 15,  4, 20,  5],
        "unit_cost" => [ 4,  4,  2,  2,  6,  1,  3,  2,  3],
    )
    .expect("failed to create edges dataframe");

    let edges_src = edges
        .column("src")
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap()
        .take_materialized_series();
    let edges_sink = edges
        .column("sink")
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap()
        .take_materialized_series();
    let edges_capacity = edges
        .column("capacity")
        .unwrap()
        .cast(&DataType::Int64)
        .unwrap()
        .take_materialized_series();
    let edges_unit_cost = edges
        .column("unit_cost")
        .unwrap()
        .cast(&DataType::Int64)
        .unwrap()
        .take_materialized_series();

    // Step 2. Add nodes
    let nodes_name = edges_src
        .clone()
        .extend(&edges_sink)
        .expect("failed to merge src nodes and sink nodes")
        .unique()
        .expect("failed to select unique node names")
        .sort(SortOptions::default())
        .expect("failed to sort node names");
    let nodes = df!(
        "name"   => &nodes_name,
        "supply" => [20, 0, 0, -5, -15],
    )
    .expect("failed to create nodes dataframe");

    let nodes_supply = nodes
        .column("supply")
        .unwrap()
        .cast(&DataType::Int64)
        .unwrap()
        .take_materialized_series();

    let num_nodes = nodes_name.len() as NodeIndex;
    let num_edges = edges_capacity.len() as ArcIndex;

    // Step 3. Define a problem
    let mut graph = StarGraph::new(num_nodes, num_edges);
    for (src, sink) in edges_src.iter().zip(edges_sink.iter()) {
        graph.add_arc(src.try_extract().unwrap(), sink.try_extract().unwrap());
    }

    let mut min_cost_flow = MinCostFlow::new(&graph);
    for (edge, (capacity, unit_cost)) in edges_capacity
        .iter()
        .zip(edges_unit_cost.iter())
        .enumerate()
    {
        min_cost_flow.set_arc_capacity(edge as ArcIndex, capacity.try_extract().unwrap());
        min_cost_flow.set_arc_unit_cost(edge as ArcIndex, unit_cost.try_extract().unwrap());
    }
    for (node, supply) in nodes_supply.iter().enumerate() {
        min_cost_flow.set_node_supply(node as NodeIndex, supply.try_extract().unwrap());
    }

    println!(
        "Solving min cost flow with: {num_nodes} nodes, and {num_edges} edges.",
        num_nodes = graph.num_nodes(),
        num_edges = graph.num_arcs(),
    );

    // Step 4. Find the minimum cost flow.
    let mut output = min_cost_flow
        .solve()
        .expect("failed to solve minimum cost flow");
    if output.status() != MinCostFlowStatus::Optimal {
        eprintln!("Solving the min cost flow is not optimal!");
    }

    let total_flow_cost = output.get_optimal_cost();
    println!("Minimum cost flow: {total_flow_cost}");

    let edges_flow =
        Series::from_iter((0..edges_capacity.len()).map(|edge| output.flow(edge as ArcIndex)))
            .with_name("flow".into());
    let edges_cost = (edges_flow.clone() * edges_unit_cost)
        .expect("failed to get edges cost")
        .with_name("cost".into());

    let mut optimized_edges = edges.clone();
    optimized_edges
        .with_column(edges_flow)
        .expect("failed to attach flow column")
        .with_column(edges_cost)
        .expect("failed to attach cost column");

    println!();
    println!("{optimized_edges}");

    let optimized_edges = optimized_edges.lazy();
    let get_arc_cost = |src, sink| -> u64 {
        optimized_edges
            .clone()
            .filter(
                lazy::dsl::col("src")
                    .eq(src)
                    .and(lazy::dsl::col("sink").eq(sink)),
            )
            .collect()
            .expect("failed to search an edge")
            .column("cost")
            .unwrap()
            .get(0)
            .expect("no such edge")
            .try_extract()
            .expect("failed to extract edge cost value")
    };

    assert_eq!(get_arc_cost(1, 4), 0);
    assert_eq!(get_arc_cost(2, 3), 15);
    assert_eq!(get_arc_cost(3, 4), 28);
    assert_eq!(get_arc_cost(4, 2), 0);
}
