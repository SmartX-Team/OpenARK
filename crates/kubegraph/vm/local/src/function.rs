use anyhow::{anyhow, Result};
use kubegraph_api::{
    frame::LazyFrame,
    function::{
        FunctionMetadata, NetworkFunctionCrd, NetworkFunctionSpec, NetworkFunctionTemplate,
    },
    graph::{GraphEdges, GraphScope},
    problem::VirtualProblem,
};
use kubegraph_vm_lazy::LazyVirtualMachine;

pub trait NetworkFunctionExt {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>>;
}

impl NetworkFunctionExt for NetworkFunctionCrd {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        self.spec.infer_edges(problem, metadata, nodes)
    }
}

impl NetworkFunctionExt for NetworkFunctionSpec {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        self.template.infer_edges(problem, metadata, nodes)
    }
}

impl NetworkFunctionExt for NetworkFunctionTemplate {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        parse_metadata(metadata, self)?.infer_edges(problem, metadata, nodes)
    }
}

impl<'a> NetworkFunctionExt for NetworkFunctionTemplate<&'a str> {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        parse_metadata(metadata, self)?.infer_edges(problem, metadata, nodes)
    }
}

impl NetworkFunctionExt for NetworkFunctionTemplate<LazyVirtualMachine> {
    fn infer_edges(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
    ) -> Result<GraphEdges<LazyFrame>> {
        let Self { filter, script } = self;

        let filter = filter
            .as_ref()
            .map(|filter| filter.call_filter(problem, nodes.clone()))
            .transpose()?;

        script.call(problem, metadata, nodes, filter)
    }
}

fn parse_metadata<T>(
    function: &FunctionMetadata,
    metadata: &NetworkFunctionTemplate<T>,
) -> Result<NetworkFunctionTemplate<LazyVirtualMachine>>
where
    T: AsRef<str>,
{
    let FunctionMetadata {
        scope: GraphScope { namespace, name },
    } = function;
    let NetworkFunctionTemplate { filter, script } = metadata;

    Ok(NetworkFunctionTemplate {
        filter: filter
            .as_ref()
            .map(|input| LazyVirtualMachine::with_lazy_filter(input.as_ref()))
            .transpose()
            .map_err(|error| {
                anyhow!("failed to parse function filter ({namespace}/{name}): {error}")
            })?,
        script: LazyVirtualMachine::with_lazy_script(script.as_ref()).map_err(|error| {
            anyhow!("failed to parse function script ({namespace}/{name}): {error}")
        })?,
    })
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
        let function_template = NetworkFunctionTemplate {
            filter: None,
            script: r"
                capacity = 50;
                unit_cost = 1;
            ",
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
        let function_template = NetworkFunctionTemplate {
            filter: Some("src != sink and src.supply >= 50 and sink.capacity >= 50"),
            script: r"
                capacity = 50;
                unit_cost = 1;
            ",
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
        function: NetworkFunctionTemplate<&'static str>,
    ) -> ::pl::frame::DataFrame {
        use kubegraph_api::{
            analyzer::{VirtualProblemAnalyzer, VirtualProblemAnalyzerType},
            graph::{GraphFilter, GraphMetadataRaw, GraphScope},
            problem::ProblemSpec,
        };

        // Step 1. Define a function metadata
        let function_metadata = FunctionMetadata {
            scope: GraphScope {
                namespace: "default".into(),
                name: function_name.into(),
            },
        };

        // Step 2. Define a problem
        let problem = VirtualProblem {
            analyzer: VirtualProblemAnalyzer {
                original_metadata: GraphMetadataRaw::default(),
                r#type: VirtualProblemAnalyzerType::Empty,
            },
            filter: GraphFilter::all("default".into()),
            scope: GraphScope {
                namespace: "default".into(),
                name: "optimize-warehouses".into(),
            },
            spec: ProblemSpec::default(),
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
