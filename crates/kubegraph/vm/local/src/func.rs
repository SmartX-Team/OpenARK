use anyhow::{Error, Result};
use kubegraph_api::{
    frame::LazyFrame, function::FunctionMetadata, graph::GraphEdges, problem::ProblemSpec,
    vm::Script,
};

use crate::lazy::LazyVirtualMachine;

pub struct FunctionContext {
    pub(crate) func: Function,
}

impl FunctionContext {
    pub fn new(func: Function) -> Self {
        Self { func }
    }
}

pub trait IntoFunction
where
    Self: TryInto<Function, Error = Error>,
{
}

impl<T> IntoFunction for T where T: TryInto<Function, Error = Error> {}

pub enum Function {
    Script(FunctionTemplate<LazyVirtualMachine>),
}

impl Function {
    pub(crate) fn infer_edges(
        &self,
        problem: &ProblemSpec,
        function: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        match self {
            Function::Script(inner) => inner.infer_edges(problem, function, nodes),
        }
    }

    pub(crate) fn dump_script(&self) -> Script {
        match self {
            Function::Script(inner) => inner.dump_script(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FunctionTemplate<T> {
    pub action: T,
    pub filter: Option<T>,
}

impl<T> TryFrom<FunctionTemplate<T>> for Function
where
    T: AsRef<str>,
{
    type Error = Error;

    fn try_from(
        value: FunctionTemplate<T>,
    ) -> Result<Self, <Self as TryFrom<FunctionTemplate<T>>>::Error> {
        let FunctionTemplate { action, filter } = value;

        Ok(Self::Script(FunctionTemplate {
            action: LazyVirtualMachine::with_lazy_script(action.as_ref())?,
            filter: filter
                .map(|input| LazyVirtualMachine::with_lazy_filter(input.as_ref()))
                .transpose()?,
        }))
    }
}

impl FunctionTemplate<LazyVirtualMachine> {
    fn infer_edges(
        &self,
        problem: &ProblemSpec,
        function: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        let filter = self
            .filter
            .as_ref()
            .map(|filter| filter.call_filter(problem, nodes.clone()))
            .transpose()?;

        self.action.call(problem, function, nodes, filter)
    }

    fn dump_script(&self) -> Script {
        self.action.dump_script()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "df-polars")]
    #[test]
    fn expand_polars_dataframe_simple() {
        // Step 1. Add nodes
        let nodes: LazyFrame = ::pl::df!(
            "name"      => [  "a",   "b"],
            "capacity"  => [300.0,   0.0],
            "supply"    => [300.0, 300.0],
            "unit_cost" => [    5,     1],
        )
        .expect("failed to create nodes dataframe")
        .into();

        // Step 2. Add a function
        let function_template = FunctionTemplate {
            action: r"
                capacity = 50;
                unit_cost = 1;
            ",
            filter: None,
        };

        // Step 3. Call a function
        let edges = expand_polars_dataframe(nodes, "move", function_template);

        // Step 4. Test outputs
        assert_eq!(
            edges,
            ::pl::df!(
                "src"            => [   "a",    "a",    "b",    "b"],
                "src.capacity"   => [ 300.0,  300.0,    0.0,    0.0],
                "src.supply"     => [ 300.0,  300.0,  300.0,  300.0],
                "src.unit_cost"  => [     5,      5,      1,      1],
                "sink"           => [   "a",    "b",    "a",    "b"],
                "sink.capacity"  => [ 300.0,    0.0,  300.0,    0.0],
                "sink.supply"    => [ 300.0,  300.0,  300.0,  300.0],
                "sink.unit_cost" => [     5,      1,      5,      1],
                "capacity"       => [  50.0,   50.0,   50.0,   50.0],
                "unit_cost"      => [   1.0,    1.0,    1.0,    1.0],
                "function"       => ["move", "move", "move", "move"],
            )
            .expect("failed to create ground-truth edges dataframe")
            .into(),
        );
    }

    #[cfg(feature = "df-polars")]
    #[test]
    fn expand_polars_dataframe_simple_with_filter() {
        // Step 1. Add nodes
        let nodes: LazyFrame = ::pl::df!(
            "name"      => [  "a",   "b"],
            "capacity"  => [300.0, 300.0],
            "supply"    => [300.0,   0.0],
            "unit_cost" => [    5,     1],
        )
        .expect("failed to create nodes dataframe")
        .into();

        // Step 2. Add a function
        let function_template = FunctionTemplate {
            action: r"
                capacity = 50;
                unit_cost = 1;
            ",
            filter: Some("src != sink and src.supply >= 50 and sink.capacity >= 50"),
        };

        // Step 3. Call a function
        let edges = expand_polars_dataframe(nodes, "move", function_template);

        // Step 4. Test outputs
        assert_eq!(
            edges,
            ::pl::df!(
                "src"            => [   "a"],
                "src.capacity"   => [ 300.0],
                "src.supply"     => [ 300.0],
                "src.unit_cost"  => [     5],
                "sink"           => [   "b"],
                "sink.capacity"  => [ 300.0],
                "sink.supply"    => [   0.0],
                "sink.unit_cost" => [     1],
                "capacity"       => [    50],
                "unit_cost"      => [     1],
                "function"       => ["move"],
            )
            .expect("failed to create ground-truth edges dataframe")
            .into(),
        );
    }

    #[cfg(feature = "df-polars")]
    fn expand_polars_dataframe(
        nodes: LazyFrame,
        function_name: &str,
        function_template: FunctionTemplate<&'static str>,
    ) -> ::pl::frame::DataFrame {
        use kubegraph_api::problem::ProblemMetadata;

        // Step 1. Add a function
        let function: Function = function_template
            .try_into()
            .expect("failed to build a function");
        let function_metadata = FunctionMetadata {
            name: function_name.into(),
        };

        // Step 2. Define a problem
        let problem = ProblemSpec {
            metadata: ProblemMetadata::default(),
            capacity: "capacity".into(),
            supply: "supply".into(),
            unit_cost: "unit_cost".into(),
        };

        // Step 3. Call a function
        function
            .infer_edges(&problem, &function_metadata, nodes)
            .expect("failed to call a function")
            .into_inner()
            .try_into_polars()
            .unwrap()
            .collect()
            .expect("failed to collect output graph edges")
    }
}
