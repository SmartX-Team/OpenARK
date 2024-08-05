use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    mem::swap,
};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    dependency::{NetworkDependencyPipelineTemplate, NetworkDependencySolverSpec},
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
use tracing::{info, instrument, Level};

#[derive(Clone, Default)]
pub struct NetworkDependencyGraph {}

#[async_trait]
impl ::kubegraph_api::dependency::NetworkDependencySolver for NetworkDependencyGraph {
    #[instrument(level = Level::INFO, skip(self, problem, spec))]
    async fn build_pipeline(
        &self,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<NetworkDependencyPipelineTemplate<GraphData<LazyFrame>>> {
        // Step 1. Register all available functions
        let graph = spec
            .functions
            .into_values()
            .map(|cr| Function::new(cr, problem))
            .collect::<Result<Graph<_>>>()?;

        // Step 2. Disaggregate the graphs
        let mut static_edges = Vec::with_capacity(spec.graphs.len());
        let mut static_nodes = Vec::with_capacity(spec.graphs.len());
        for ::kubegraph_api::graph::Graph {
            connector: _,
            data: GraphData { edges, mut nodes },
            metadata,
            scope,
        } in spec.graphs
        {
            // Mark the connector
            nodes.alias_nodes(&problem.spec.metadata, &scope)?;

            static_edges.push(edges);
            static_nodes.push((metadata, nodes));
        }

        // Step 3. Collect all static edges
        let static_edges: GraphEdges<_> = static_edges.into_iter().map(GraphEdges::new).collect();
        let static_edges =
            static_edges.mark_as_static(&problem.spec.metadata, &problem.scope.namespace)?;

        // Step 4. Collect all pipelines per graph
        // NOTE: static edges can be used instead of pipelines
        let (pipelines, static_nodes) = graph.build_pipelines(problem, static_nodes);

        // Step 5. Merge duplicated pipelines
        let merged_pipelines = pipelines
            .into_iter()
            .map(
                |GraphPipeline {
                     inner: ::kubegraph_dependency_graph::GraphPipeline { nodes: functions },
                     nodes,
                 }| {
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

        // Step 6. Build the dependency pipeline graph
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

        // Step 7. Collect all graphs
        let edges: GraphEdges<_> = finalized_edges
            .into_iter()
            .map(GraphEdges::new)
            .chain(Some(static_edges.clone()))
            .collect();
        let nodes: GraphEdges<_> = finalized_nodes
            .into_iter()
            .chain(static_nodes)
            .map(GraphEdges::new)
            .collect();
        let graph = GraphData {
            edges: edges.into_inner(),
            nodes: nodes.into_inner(),
        };

        if problem.spec.verbose {
            let GraphData { edges, nodes } = graph.clone().collect().await?;
            info!("Nodes: {nodes}\nEdges: {edges}");
        }

        Ok(NetworkDependencyPipelineTemplate {
            graph,
            static_edges: Some(static_edges),
        })
    }
}

trait GraphPipelineBuilder {
    fn build_pipelines<M>(
        &self,
        problem: &VirtualProblem,
        nodes: Vec<(M, LazyFrame)>,
    ) -> (Vec<GraphPipeline<'_>>, Vec<LazyFrame>)
    where
        M: GraphMetadataExt;
}

impl GraphPipelineBuilder for Graph<Function> {
    fn build_pipelines<M>(
        &self,
        problem: &VirtualProblem,
        nodes: Vec<(M, LazyFrame)>,
    ) -> (Vec<GraphPipeline<'_>>, Vec<LazyFrame>)
    where
        M: GraphMetadataExt,
    {
        let mut dropped_nodes = Vec::default();
        let mut pipelines = Vec::default();

        for (metadata, nodes) in nodes {
            let src = metadata.all_node_inputs_raw();
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

            match self
                .build_pipeline(&claim)
                .and_then(|mut pipelines| pipelines.pop())
            {
                Some(inner) => pipelines.push(GraphPipeline { inner, nodes }),
                None => dropped_nodes.push(nodes),
            }
        }

        (pipelines, dropped_nodes)
    }
}

struct GraphPipeline<'a> {
    inner: ::kubegraph_dependency_graph::GraphPipeline<'a, Function>,
    nodes: LazyFrame,
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
    fn new<M>(cr: NetworkFunctionCrd, problem: &VirtualProblem<M>) -> Result<Self>
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
