use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    mem::swap,
};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    analyzer::NetworkAnalyzer,
    dependency::{NetworkDependencyPipeline, NetworkDependencySolverSpec},
    frame::LazyFrame,
    function::{
        FunctionMetadata, NetworkFunctionCrd, NetworkFunctionKind, NetworkFunctionTemplate,
    },
    graph::{GraphData, GraphEdges, GraphMetadataExt, GraphScope},
    problem::VirtualProblem,
    vm::{Instruction, Stmt},
};
use kubegraph_dependency_graph::{
    merge::{GraphPipelineMerge, GraphPipelineMergedNode, NodeIndex},
    Graph, GraphPipelineClaim, GraphPipelineClaimOptions, Node,
};
use kubegraph_vm_lazy::{
    function::{NetworkFunction, NetworkFunctionInferType},
    LazyVirtualMachine,
};
use regex::Regex;

#[derive(Clone, Default)]
pub struct NetworkDependencyGraph {}

#[async_trait]
impl ::kubegraph_api::dependency::NetworkDependencySolver for NetworkDependencyGraph {
    async fn build_pipeline<A>(
        &self,
        _analyzer: &A,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<Option<NetworkDependencyPipeline<GraphData<LazyFrame>, A>>>
    where
        A: NetworkAnalyzer,
    {
        // Step 1. Register all available functions
        let graph = spec
            .functions
            .into_iter()
            .map(|cr| Function::new(cr, problem))
            .collect::<Result<Graph<_>>>()?;

        // Step 2. Collect all pipelines per graph
        let pipelines = graph.build_pipelines(problem, spec.graphs);
        if pipelines.is_empty() {
            return Ok(None);
        }

        // Step 3. Merge duplicated pipelines
        let mut static_edges = vec![];
        let merged_pipelines = pipelines
            .into_iter()
            .map(
                |GraphPipeline {
                     graph:
                         ::kubegraph_api::graph::Graph {
                             data: GraphData { edges, nodes },
                             metadata: _,
                             scope: _,
                         },
                     inner: ::kubegraph_dependency_graph::GraphPipeline { nodes: functions },
                 }| {
                    static_edges.push(edges);

                    let mut nodes = Some(nodes);
                    let nodes = ::std::iter::from_fn(move || Some(nodes.take()));
                    functions
                        .into_iter()
                        .zip(nodes)
                        .map(|(function, nodes)| GraphPipelineNode { function, nodes })
                        .collect::<Vec<_>>()
                },
            )
            .merge_pipelines();

        // Step 4. Build the dependency pipeline graph
        let mut finalized_edges = Vec::default();
        let mut finalized_nodes = Vec::default();
        let mut stack = BTreeMap::<_, Vec<_>>::default();
        for (index, pipeline) in merged_pipelines.into_iter().enumerate().rev() {
            let mut nodes = stack.remove(&index).unwrap_or_default();

            for merged_node in pipeline {
                match merged_node {
                    GraphPipelineMergedNode::Item(neighbors) => {
                        let mut callable = None;
                        for GraphPipelineNode {
                            function,
                            nodes: maybe_nodes,
                        } in neighbors
                        {
                            // NOTE: the function should be same among the neighbors
                            if callable.is_none() {
                                callable = Some(function);
                            }
                            if let Some(static_nodes) = maybe_nodes {
                                nodes.push(static_nodes);
                            }
                        }
                        if nodes.is_empty() {
                            bail!("empty input graphs");
                        }

                        let callable = callable.ok_or_else(|| anyhow!("empty function"))?;
                        let metadata = callable.metadata();
                        let inputs: GraphEdges<_> = {
                            let mut fetched = Vec::default();
                            swap(&mut fetched, &mut nodes);
                            fetched.into_iter().map(GraphEdges::new).collect()
                        };

                        if callable.is_final {
                            finalized_nodes.push(inputs.clone().into_inner());
                        }

                        let output = callable.infer(
                            problem,
                            &metadata,
                            inputs.into_inner(),
                            callable.infer_type(),
                        )?;
                        nodes.push(output.into_inner());
                    }
                    GraphPipelineMergedNode::Next(index) => {
                        stack.entry(index).or_default().append(&mut nodes)
                    }
                }
            }
            finalized_edges.append(&mut nodes);
        }

        // Step 5. Collect all graphs
        let edges: GraphEdges<_> = finalized_edges.into_iter().map(GraphEdges::new).collect();
        let nodes: GraphEdges<_> = finalized_nodes.into_iter().map(GraphEdges::new).collect();
        let graph = GraphData {
            edges: edges.into_inner(),
            nodes: nodes.into_inner(),
        };

        if problem.spec.verbose {
            let GraphData { edges, nodes } = graph.clone().collect().await?;
            println!("Edges: {edges}");
            println!("Nodes: {nodes}");
            println!();
        }

        let static_edges = static_edges.into_iter().map(GraphEdges::new).collect();

        Ok(Some(NetworkDependencyPipeline::<GraphData<LazyFrame>, A> {
            graph,
            problem: VirtualProblem {
                // TODO: to be implemented
                // TODO: 여기부터 시작
                analyzer: BTreeMap::default(),
                filter: problem.filter.clone(),
                scope: problem.scope.clone(),
                spec: problem.spec.clone(),
            },
            static_edges: Some(static_edges),
        }))
    }
}

trait GraphPipelineBuilder {
    fn build_pipelines(
        &self,
        problem: &VirtualProblem,
        graphs: Vec<::kubegraph_api::graph::Graph<LazyFrame>>,
    ) -> Vec<GraphPipeline<'_>>;
}

impl GraphPipelineBuilder for Graph<Function> {
    fn build_pipelines(
        &self,
        problem: &VirtualProblem,
        graphs: Vec<::kubegraph_api::graph::Graph<LazyFrame>>,
    ) -> Vec<GraphPipeline<'_>> {
        graphs
            .into_iter()
            .filter_map(|graph| {
                let src = graph.metadata.all_node_inputs_raw();
                let sink: Vec<_> = problem
                    .spec
                    .metadata
                    .all_node_inputs()
                    .iter()
                    .map(|&column| column.into())
                    .collect();

                let claim = GraphPipelineClaim {
                    option: GraphPipelineClaimOptions {
                        fastest: true,
                        ..Default::default()
                    },
                    src: &src,
                    sink: &sink,
                };

                self.build_pipeline(&claim)
                    .and_then(|mut pipelines| pipelines.pop())
                    .map(|inner| GraphPipeline { graph, inner })
            })
            .collect()
    }
}

struct GraphPipeline<'a> {
    graph: ::kubegraph_api::graph::Graph<LazyFrame>,
    inner: ::kubegraph_dependency_graph::GraphPipeline<'a, Function>,
}

impl<'a> fmt::Debug for GraphPipeline<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl<'a> fmt::Display for GraphPipeline<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

struct GraphPipelineNode<'a> {
    function: &'a Function,
    nodes: Option<LazyFrame>,
}

impl<'a> fmt::Debug for GraphPipelineNode<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphPipelineNode")
            .field("function", &self.function.name())
            .field("nodes", &self.nodes.is_some())
            .finish()
    }
}

impl<'a> fmt::Display for GraphPipelineNode<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { function, nodes: _ } = self;
        function.fmt(f)
    }
}

impl<'a> NodeIndex for GraphPipelineNode<'a> {
    type Key = String;

    fn key(&self) -> String {
        self.function.name()
    }
}

#[derive(Debug)]
struct Function {
    cr: NetworkFunctionCrd,
    is_final: bool,
    provided: Vec<String>,
    requirements: Vec<String>,
    template: NetworkFunctionTemplate<LazyVirtualMachine>,
}

impl Function {
    fn new<A, M>(cr: NetworkFunctionCrd, problem: &VirtualProblem<A, M>) -> Result<Self>
    where
        M: GraphMetadataExt,
    {
        let filter = cr
            .spec
            .template
            .filter
            .as_ref()
            .map(|filter| LazyVirtualMachine::with_lazy_filter(filter))
            .transpose()?;
        let script = LazyVirtualMachine::with_lazy_script(&cr.spec.template.script)?;

        let mut provided = BTreeSet::default();
        let mut requirements = BTreeSet::default();
        for Instruction { name, stmt } in script.dump_script().code.into_iter().chain(
            filter
                .as_ref()
                .map(|vm| vm.dump_script().code)
                .unwrap_or_default(),
        ) {
            let name = match name {
                Some(ref name) => {
                    let re = Regex::new(r"^s(rc|ink)\.").unwrap();
                    re.replace(name, "").into()
                }
                None => continue,
            };

            let buf = match &stmt {
                Stmt::DefineLocalFeature { .. } | Stmt::DefineLocalValue { .. } => {
                    &mut requirements
                }
                _ => &mut provided,
            };
            buf.insert(name);
        }

        let is_final = !matches!(&cr.spec.kind, NetworkFunctionKind::Annotation(_));
        if is_final {
            provided.insert(problem.spec.metadata.function().into());
        }

        Ok(Self {
            cr,
            is_final,
            provided: provided.into_iter().collect(),
            requirements: requirements.into_iter().collect(),
            template: NetworkFunctionTemplate { filter, script },
        })
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            scope: self.scope(),
        }
    }

    fn name(&self) -> String {
        GraphScope::parse_name(&self.cr)
    }

    fn scope(&self) -> GraphScope {
        GraphScope::from_resource(&self.cr)
    }

    const fn infer_type(&self) -> NetworkFunctionInferType {
        if self.is_final {
            NetworkFunctionInferType::Edge
        } else {
            NetworkFunctionInferType::Node
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name().fmt(f)?;
        if self.is_final {
            write!(f, " -> !")?;
        }
        Ok(())
    }
}

impl Node for Function {
    type Feature = String;

    fn is_final(&self) -> bool {
        self.is_final
    }

    fn provided(&self) -> &[Self::Feature] {
        &self.provided
    }

    fn requirements(&self) -> &[Self::Feature] {
        &self.requirements
    }
}

impl NetworkFunction for Function {
    fn infer(
        &self,
        problem: &VirtualProblem,
        metadata: &FunctionMetadata,
        nodes: LazyFrame,
        infer_type: NetworkFunctionInferType,
    ) -> Result<GraphEdges<LazyFrame>> {
        self.template
            .infer(problem, metadata, nodes, infer_type)
            .map_err(|error| anyhow!("failed to execute network function script: {error}"))
    }
}
