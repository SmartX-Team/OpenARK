use anyhow::{Error, Result};
use kubegraph_api::graph::Graph;

use crate::{df::LazyFrame, lazy::LazyVirtualMachine};

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
    pub(crate) fn call(&self, graph: &Graph<LazyFrame>) -> Result<Graph<LazyFrame>> {
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
    fn call(&self, graph: &Graph<LazyFrame>) -> Result<Graph<LazyFrame>> {
        let filter = self
            .filter
            .as_ref()
            .map(|filter| filter.call_filter(graph))
            .transpose()?;

        self.action.call(graph, filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "polars")]
    #[test]
    fn expand_polars_dataframe_simple() {
        use pl::prelude::NamedFrom;

        // Step 1. Add nodes & edges
        let nodes: LazyFrame = ::pl::df!(
            "name" => &["a", "b"],
            "payload" => &[300.0, 0.0],
        )
        .expect("failed to create nodes dataframe")
        .into();

        let edges: LazyFrame = ::pl::df!(
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
        let next_nodes = match next_graph.nodes {
            LazyFrame::Polars(df) => df
                .collect()
                .expect("failed to collect polars edges LazyFrame"),
            #[allow(unreachable_patterns)]
            _ => panic!("failed to unwrap polars edges LazyFrame"),
        };
        assert_eq!(
            next_nodes.column("name").unwrap(),
            &::pl::series::Series::new("name", vec!["a".to_string(), "b".to_string()]),
        );
        assert_eq!(
            next_nodes.column("payload").unwrap(),
            &::pl::series::Series::new("payload", vec![297.0, 3.0]),
        );
        assert_eq!(
            next_nodes.column("moved_out").unwrap(),
            &::pl::series::Series::new("moved_out", vec![Some(3.0), None]),
        );
        assert_eq!(
            next_nodes.column("moved_in").unwrap(),
            &::pl::series::Series::new("moved_in", vec![None, Some(3.0)]),
        );
    }

    #[cfg(feature = "polars")]
    #[test]
    fn expand_polars_dataframe_simple_with_filter() {
        use pl::prelude::NamedFrom;

        // Step 1. Add nodes & edges
        let nodes: LazyFrame = ::pl::df!(
            "name" => &["a", "b"],
            "payload" => &[300.0, 0.0],
        )
        .expect("failed to create nodes dataframe")
        .into();

        let edges: LazyFrame = ::pl::df!(
            "src" => &["a", "b"],
            "sink" => &["b", "a"],
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
            filter: Some("src.payload >= 3"),
        };
        let function: Function = function_template
            .try_into()
            .expect("failed to build a function");

        // Step 3. Call a function
        let next_graph = function.call(&graph).expect("failed to call a function");

        // Step 4. Test outputs
        let next_nodes = match next_graph.nodes {
            LazyFrame::Polars(df) => df
                .collect()
                .expect("failed to collect polars edges LazyFrame"),
            #[allow(unreachable_patterns)]
            _ => panic!("failed to unwrap polars edges LazyFrame"),
        };
        assert_eq!(
            next_nodes.column("name").unwrap(),
            &::pl::series::Series::new("name", vec!["a".to_string(), "b".to_string()]),
        );
        assert_eq!(
            next_nodes.column("payload").unwrap(),
            &::pl::series::Series::new("payload", vec![297.0, 3.0]),
        );
        assert_eq!(
            next_nodes.column("moved_out").unwrap(),
            &::pl::series::Series::new("moved_out", vec![Some(3.0), None]),
        );
        assert_eq!(
            next_nodes.column("moved_in").unwrap(),
            &::pl::series::Series::new("moved_in", vec![None, Some(3.0)]),
        );
    }
}
