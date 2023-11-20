use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use ndarray::{ArrayView2, Axis};
use or_tools::constraint_solver::{
    routing::RoutingModel,
    routing_enums::FirstSolutionStrategy,
    routing_index_manager::{
        RoutingIndexManager, RoutingIndexManagerVehiclePlan, RoutingNodeIndex,
    },
    routing_parameters::RoutingSearchParameters,
};

#[derive(Debug, Default)]
pub struct Router<'a> {
    matrix: BTreeMap<String, ArrayView2<'a, i64>>,
}

impl<'a> Router<'a> {
    pub fn add_dimension(
        &mut self,
        name: impl Into<String>,
        matrix: ArrayView2<'a, i64>,
    ) -> Result<()> {
        let name = name.into();

        if matrix.is_empty() {
            let shape = shape(&matrix);
            bail!("empty matrix: {name} is {shape}");
        }
        if !matrix.is_square() {
            let shape = shape(&matrix);
            bail!("no squared matrix: {name} is {shape}");
        }
        if matrix
            .rows()
            .into_iter()
            .enumerate()
            .any(|(index, row)| row[index] != 0)
        {
            bail!("no distance matrix: {name}");
        }

        if self.matrix.contains_key(&name) {
            bail!("duplicated matrix: {name}");
        }

        if let Some(old_matrix) = self.get_first_matrix() {
            if old_matrix.shape() != matrix.shape() {
                let old_shape = shape(old_matrix);
                let shape = shape(&matrix);
                bail!("different matrix shape: expected {old_shape} but given {shape}");
            }
        }

        self.matrix.insert(name, matrix);
        Ok(())
    }

    pub fn get_fastest_route(&self, start: usize, end: usize) -> Result<()> {
        self.assert_node_index(start)?;
        self.assert_node_index(end)?;

        // Instantiate the data problem.
        let num_nodes = self.num_nodes().unwrap().try_into()?;
        let num_vehicles = 6;

        let start_index = RoutingNodeIndex::new(start.try_into()?);
        let end_index = RoutingNodeIndex::new(end.try_into()?);

        // Create Routing Index Manager.
        let depot = RoutingNodeIndex::new(0);
        let manager = RoutingIndexManager::new(
            num_nodes,
            num_vehicles,
            RoutingIndexManagerVehiclePlan::Depot(depot),
        );

        // Create Routing Model.
        let mut routing = RoutingModel::new(&manager, None);

        // Define cost of each arc.
        let transit_callbacks: BTreeMap<_, _> = self
            .matrix
            .iter()
            .map(|(name, matrix)| {
                let callback = Box::leak(Box::new(|from_index, to_index| {
                    let from_node = manager.index_to_node(from_index).value() as usize;
                    let to_node = manager.index_to_node(to_index).value() as usize;
                    matrix[(from_node, to_node)]
                }));

                (name, callback)
            })
            .collect();

        // Register costs.
        let transit_callback_indices: BTreeMap<_, _> = transit_callbacks
            .iter()
            .map(|(name, callback)| {
                let callback_index = routing.register_transit_callback(*callback);
                routing.set_arc_cost_evaluator_of_all_vehicles(callback_index);

                (*name, callback_index)
            })
            .collect();

        // Add Distance constraints.
        for (name, transit_index) in transit_callback_indices {
            routing.add_dimension(
                transit_index,
                0,    // no slack
                3000, // vehicle maximum travel distance
                true, // start cumul to zero
                name,
            );
            let distance_dimension = routing.get_mutable_dimension(name).unwrap();
            distance_dimension.set_global_span_cost_coefficient(100);

            // Define Transportation Requests.
            let pickups_deliveries = &[[start_index, end_index]];

            let solver = routing.solver();
            for [pickup_node, delivery_node] in pickups_deliveries {
                let pickup_index = manager.node_to_index(pickup_node);
                let delivery_index = manager.node_to_index(delivery_node);
                routing.add_pickup_and_delivery(pickup_index, delivery_index);

                let pickup_var = routing.vehicle_var(pickup_index).unwrap();
                let delivery_var = routing.vehicle_var(delivery_index).unwrap();

                let constraint = solver.make_equality(pickup_var, delivery_var);
                solver.add_constraint(constraint);

                let constraint = solver.make_less_or_equal(
                    distance_dimension.cumul_var(pickup_index).unwrap(),
                    distance_dimension.cumul_var(delivery_index).unwrap(),
                );
                solver.add_constraint(constraint);
            }
        }

        // Setting first solution heuristic.
        let mut search_parameters = RoutingSearchParameters::new();
        search_parameters
            .set_first_solution_strategy(FirstSolutionStrategy::ParallelCheapestInsertion);
        search_parameters.set_time_limit(Duration::from_secs(1));

        // Solve the problem.
        let instant = Instant::now();
        let solution = routing.solve_with_parameters(&search_parameters);
        let elapsed_ms = instant.elapsed().as_millis();

        // Print solution on console.
        let mut total_distance = 0;
        for vehicle_id in 0..num_vehicles {
            let mut index = routing.start(vehicle_id);
            println!("Route for Vehicle {vehicle_id}:");

            let mut route_distance = 0;
            if solution.is_vehicle_used(vehicle_id).unwrap_or_default() {
                print!("* Route: ");
                while !routing.is_end(index) {
                    print!("{} -> ", manager.index_to_node(index).value());
                    let previous_index = index;
                    index = solution.value(routing.next_var(index).unwrap()).unwrap();
                    route_distance +=
                        routing.get_arc_cost_for_vehicle(previous_index, index, vehicle_id as i64);
                }

                println!();
                println!("* Distance of the route: {route_distance}m");
                total_distance += route_distance;
            } else {
                println!("* Unused");
            }
        }
        println!();
        println!("Total distance of all routes: {total_distance}m");
        println!("Problem solved in {elapsed_ms}ms");
        Ok(())
    }

    fn num_nodes(&self) -> Option<usize> {
        self.get_first_matrix().map(|matrix| matrix.len_of(Axis(0)))
    }

    fn assert_node_index(&self, index: usize) -> Result<()> {
        match self.get_first_matrix() {
            Some(matrix) => {
                let len = matrix.len_of(Axis(0));
                if index < len {
                    Ok(())
                } else {
                    bail!("out of index: {index} should be less than {len}")
                }
            }
            None => bail!("no dimensions"),
        }
    }

    fn get_first_matrix(&self) -> Option<&ArrayView2<'a, i64>> {
        self.matrix.first_key_value().map(|(_, value)| value)
    }
}

pub trait RouterDimension {
    fn get(&self, start: usize, end: usize) -> Option<i64>;
}

fn shape(matrix: &ArrayView2<'_, i64>) -> String {
    let rows = matrix.len_of(Axis(0));
    let cols = matrix.len_of(Axis(1));
    format!("{rows}x{cols}")
}
