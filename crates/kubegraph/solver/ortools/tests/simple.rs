extern crate polars as pl;

use itertools::Itertools;
use or_tools::graph::{
    ebert_graph::{ArcIndex, NodeIndex, StarGraph},
    max_flow::{MaxFlow, MaxFlowStatus},
};
use pl::{chunked_array::ops::SortOptions, datatypes::DataType, df};

#[test]
fn max_flow_simple() {
    // Step 1. Add edges
    let edges = df!(
        "src" =>     [ 0,  0,  0,  1,  1,  2,  2,  3,  3],
        "sink" =>    [ 1,  2,  3,  2,  4,  3,  4,  2,  4],
        "capacity" => [20, 30, 10, 40, 30, 10, 20,  5, 20],
    )
    .expect("failed to create edges dataframe");

    let edges_src = edges.column("src").unwrap().cast(&DataType::Int32).unwrap();
    let edges_sink = edges
        .column("sink")
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap();
    let edges_capacity = edges
        .column("capacity")
        .unwrap()
        .cast(&DataType::Int64)
        .unwrap();

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
        "name" => &nodes_name,
    )
    .expect("failed to create nodes dataframe");

    let num_nodes = nodes_name.len() as NodeIndex;
    let num_arcs = edges_capacity.len() as ArcIndex;

    // Step 3. Define a problem
    let mut graph = StarGraph::new(num_nodes, num_arcs);
    for (src, sink) in edges_src.iter().zip(edges_sink.iter()) {
        graph.add_arc(src.try_extract().unwrap(), sink.try_extract().unwrap());
    }

    let mut max_flow = MaxFlow::new(&graph, 0 /* node 0 */, num_nodes - 1 /* node 4 */);
    for (arc, capacity) in edges_capacity.iter().enumerate() {
        max_flow.set_arc_capacity(arc as ArcIndex, capacity.try_extract().unwrap());
    }

    println!(
        "Solving max flow with: {num_nodes} nodes, and {num_arcs} arcs.",
        num_nodes = graph.num_nodes(),
        num_arcs = graph.num_arcs(),
    );

    // 4. Find the maximum flow between node 0 and node 4.
    let output = max_flow.solve().expect("failed to solve maximum flow");
    if output.status() != MaxFlowStatus::Optimal {
        eprintln!("Solving the max flow is not optimal!");
    }
    let total_flow = output.get_optimal_flow();
    println!("Maximum flow: {total_flow}");
    println!();
    println!(" Arc  : Flow / Capacity");
    for arc in (0..edges_capacity.len()).map(|arc| arc as ArcIndex) {
        println!(
            "{tail} -> {head}: {flow} / {capacity}",
            tail = graph.tail(arc),
            head = graph.head(arc),
            flow = output.flow(arc),
            capacity = output.capacity(arc),
        );
    }

    assert_eq!(total_flow, 60);
}
