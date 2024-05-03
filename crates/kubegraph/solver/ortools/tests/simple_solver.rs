extern crate polars as pl;

use kubegraph_api::{
    graph::Graph,
    solver::{LocalSolver, MaxFlowProblem, MinCostProblem, ProblemConstrait, ProblemMetadata},
};
use kubegraph_solver_ortools::Solver;
use pl::{
    df,
    frame::DataFrame,
    lazy::{dsl, frame::IntoLazy},
};

#[test]
fn max_flow() {
    // Step 1. Define edges
    let edges = df!(
        "src"      => [ 0,  0,  0,  1,  1,  2,  2,  3,  3],
        "sink"     => [ 1,  2,  3,  2,  4,  3,  4,  2,  4],
        "capacity" => [20, 30, 10, 40, 30, 10, 20,  5, 20],
    )
    .expect("failed to create edges dataframe");

    // Step 2. Define nodes
    let nodes = df!(
        "name"     => [  0,   1,   2,   3,   4],
    )
    .expect("failed to create nodes dataframe");

    // Step 3. Define a graph
    let graph = Graph { edges, nodes };

    // Step 4. Define a problem
    let problem = MaxFlowProblem {
        metadata: ProblemMetadata {
            verbose: true,
            ..Default::default()
        },
        capacity: "capacity".into(),
    };

    // Step 5. Define a solver
    let solver = Solver::default();

    // Step 6. Optimize the graph
    let optimized_graph: Graph<DataFrame> = solver
        .step_max_flow(graph, problem)
        .expect("failed to optimize the graph")
        .try_into()
        .expect("failed to collect graph");
    let Graph {
        edges: optimized_edges,
        nodes: optimized_nodes,
    } = optimized_graph;

    let total_flow: i64 = optimized_edges
        .clone()
        .lazy()
        .filter(dsl::col("src").eq(0))
        .select([dsl::col("flow").sum().alias("total_flow")])
        .first()
        .collect()
        .expect("failed to calculate total flow")
        .column("total_flow")
        .expect("failed to retrieve total flow column")
        .get(0)
        .expect("failed to get total flow row")
        .try_extract()
        .expect("failed to extract total flow value");

    println!("Total flow: {total_flow}");
    assert_eq!(total_flow, 60);

    println!();
    println!("{}", &optimized_nodes);
    println!("{}", &optimized_edges);
}

#[test]
fn min_cost_flow_simple() {
    // Step 1. Define edges
    let edges = df!(
        "src"       => [ 0,  0,  1,  1,  1,  2,  2,  3,  4],
        "sink"      => [ 1,  2,  2,  3,  4,  3,  4,  4,  2],
        "capacity"  => [15,  8, 20,  4, 10, 15,  4, 20,  5],
        "unit_cost" => [ 4,  4,  2,  2,  6,  1,  3,  2,  3],
    )
    .expect("failed to create edges dataframe");

    // Step 2. Define edges
    let nodes = df!(
        "name"      => [  0,   1,   2,   3,   4],
        "supply"      => [ 20,   0,   0,  -5, -15],
    )
    .expect("failed to create nodes dataframe");

    // Step 3. Define a graph
    let graph = Graph { edges, nodes };

    // Step 4. Define a problem
    let problem = MinCostProblem {
        metadata: ProblemMetadata {
            verbose: true,
            ..Default::default()
        },
        capacity: "capacity".into(),
        constraint: ProblemConstrait {
            cost: "unit_cost".into(),
            supply: "supply".into(),
        },
    };

    // Step 5. Define a solver
    let solver = Solver::default();

    // Step 6. Optimize the graph
    let optimized_graph: Graph<DataFrame> = solver
        .step_min_cost(graph, problem)
        .expect("failed to optimize the graph")
        .try_into()
        .expect("failed to collect graph");
    let Graph {
        edges: mut optimized_edges,
        nodes: optimized_nodes,
    } = optimized_graph;

    let edges_flow = optimized_edges.column("flow").unwrap();
    let edges_unit_cost = optimized_edges.column("unit_cost").unwrap();

    let edges_cost = (edges_flow * edges_unit_cost).with_name("cost");
    optimized_edges
        .with_column(edges_cost)
        .expect("failed to insert edge cost column");

    println!();
    println!("{}", &optimized_nodes);
    println!("{}", &optimized_edges);

    let get_arc_cost = |src, sink| -> u64 {
        optimized_edges
            .clone()
            .lazy()
            .filter(dsl::col("src").eq(src).and(dsl::col("sink").eq(sink)))
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
