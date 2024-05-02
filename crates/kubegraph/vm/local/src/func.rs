use anyhow::{Error, Result};
use kubegraph_api::graph::Graph;

use crate::{df::DataFrame, lazy::LazyVirtualMachine};

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
    pub(crate) fn call(&self, graph: &Graph<DataFrame>) -> Result<DataFrame> {
        match self {
            Function::Script(inner) => inner.call(graph),
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
    fn call(&self, graph: &Graph<DataFrame>) -> Result<DataFrame> {
        // TODO: add filter support
        self.action.call(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "polars")]
    #[test]
    fn expand_polars_dataframe_simple() {
        // Step 1. Add nodes & edges

        let nodes: DataFrame = ::pl::df!(
            "name" => &["a", "b"],
            "payload" => &[300.0, 0.0],
        )
        .expect("failed to create nodes dataframe")
        .into();

        let edges: DataFrame = ::pl::df!(
            "src" => &["a"],
            "sink" => &["b"],
        )
        .expect("failed to create edges dataframe")
        .into();

        let graph = Graph { edges, nodes };

        // Step 2. Add functions
        let function_template = FunctionTemplate {
            action: r"
                src.payload = src.payload - 3;
                sink.payload = sink.payload + 3;

                src.moved_out = 3;
                sink.moved_in = 3;
            ",
            filter: None,
        };
        let function: Function = function_template
            .try_into()
            .expect("failed to build a function");

        // Step 3. Call a function
        let next_graph = function.call(&graph).expect("failed to call a function");

        // Step 4. Test outputs
        let next_graph = match next_graph {
            DataFrame::PolarsLazy(df) => df.collect().expect("failed to collect polars LazyFrame"),
            _ => panic!("failed to unwrap polars dataframe"),
        };
        assert_eq!(
            next_graph.column("src.payload").unwrap(),
            &::pl::series::Series::from_iter(&[297.0]).with_name("src.payload"),
        );
        assert_eq!(
            next_graph.column("sink.payload").unwrap(),
            &::pl::series::Series::from_iter(&[3.0]).with_name("sink.payload"),
        );
        assert_eq!(
            next_graph.column("src.moved_out").unwrap(),
            &::pl::series::Series::from_iter(&[3.0]).with_name("src.moved_out"),
        );
        assert_eq!(
            next_graph.column("sink.moved_in").unwrap(),
            &::pl::series::Series::from_iter(&[3.0]).with_name("sink.moved_in"),
        );
    }
}
