#[cfg(feature = "polars")]
extern crate polars as pl;

mod ctx;
mod func;
mod lazy;

use std::collections::{btree_map::Entry, BTreeMap};

use anyhow::Result;
use kubegraph_api::frame::IntoLazyFrame;

pub use self::func::IntoFunction;
use self::{ctx::Context, func::Function};

#[derive(Default)]
pub struct VirtualMachine<K> {
    contexts: BTreeMap<K, Context>,
    functions: BTreeMap<String, Function>,
}

impl<K> VirtualMachine<K>
where
    K: Ord,
{
    pub fn insert_edges(&mut self, key: K, edges: impl IntoLazyFrame) {
        let edges = Some(edges.into());
        match self.contexts.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().edges = edges,
            Entry::Vacant(ctx) => {
                ctx.insert(Context {
                    edges,
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_nodes(&mut self, key: K, nodes: impl IntoLazyFrame) {
        let nodes = Some(nodes.into());
        match self.contexts.entry(key) {
            Entry::Occupied(ctx) => ctx.into_mut().nodes = nodes,
            Entry::Vacant(ctx) => {
                ctx.insert(Context {
                    nodes,
                    ..Default::default()
                });
            }
        }
    }

    pub fn insert_function(&mut self, name: String, function: impl IntoFunction) -> Result<()> {
        let function = function.try_into()?;
        self.functions.insert(name, function);
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

impl<K> VirtualMachine<K>
where
    K: Ord,
{
    pub fn step<T>(&mut self, problem: &Problem<T>) -> Result<()>
    where
        T: AsRef<str>,
    {
        Ok(())
    }
}

pub struct Problem<T>
where
    T: AsRef<str>,
{
    pub cost: Option<T>,
    pub value: T,
}

#[cfg(test)]
mod tests {
    use crate::func::FunctionTemplate;

    use super::*;

    #[cfg(feature = "polars")]
    #[test]
    fn simulate_simple() {
        // Step 1. Define problems
        let mut vm = VirtualMachine::default();

        // Step 2. Add nodes & edges
        let nodes = ::pl::df!(
            "name" => &["a", "b"],
            "payload" => &[300.0, 0.0],
            "warehouse" => &[true, true],
        )
        .expect("failed to create nodes dataframe");
        vm.insert_nodes("warehouse", nodes);

        // Step 3. Add functions
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
        vm.insert_function("move".into(), function)
            .expect("failed to insert function");

        // Step 4. Add cost & value function (heuristic)
        let problem = Problem {
            cost: None,
            value: "src.traffic",
        };

        // Step 5. Do optimize
        vm.step(&problem).expect("failed to optimize");
    }
}
