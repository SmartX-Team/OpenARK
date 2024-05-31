mod llm_model;
mod prompt;

use anyhow::{bail, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{
        Graph, GraphFilter, GraphMetadataExt, GraphMetadataRaw, GraphMetadataStandard, GraphScope,
    },
    problem::{
        llm::VirtualProblemLLMAnalyzer, NetworkProblemCrd, ProblemSpec, VirtualProblem,
        VirtualProblemAnalyzer,
    },
    resource::NetworkResourceCollectionDB,
};
use langchain_rust::language_models::llm::LLM;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkAnalyzerArgs {}

#[derive(Clone)]
pub struct NetworkAnalyzer<M = self::llm_model::GenericLLM> {
    llm: M,
    prompt: self::prompt::PromptLoader,
}

#[async_trait]
impl NetworkComponent for NetworkAnalyzer {
    type Args = NetworkAnalyzerArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        let NetworkAnalyzerArgs {} = args;
        Ok(Self {
            llm: self::llm_model::GenericLLM::default(),
            prompt: self::prompt::PromptLoader::try_default().await?,
        })
    }
}

#[async_trait]
impl<M> ::kubegraph_api::analyzer::NetworkAnalyzer for NetworkAnalyzer<M>
where
    M: LLM,
{
    #[instrument(level = Level::INFO, skip(self, resource_db))]
    async fn inspect(
        &self,
        resource_db: &dyn NetworkResourceCollectionDB,
    ) -> Result<Vec<VirtualProblem>> {
        // TODO: to be implemented (aggregating other problems)
        let problems = resource_db.list(()).await;

        match problems {
            Some(problems) if !problems.is_empty() => {
                problems
                    .into_iter()
                    .map(pin_problem)
                    .map(|problem| self.pin_virtual_problem(problem))
                    .collect::<FuturesUnordered<_>>()
                    .try_collect()
                    .await
            }
            Some(_) | None => return Ok(Vec::default()),
        }
    }

    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        let VirtualProblemLLMAnalyzer {
            map: map_from,
            original_metadata,
        } = problem.analyzer.to_llm()?;
        let map_to = problem.spec.metadata;

        // TODO: to be implemented
        let Graph {
            data,
            metadata,
            scope,
        } = graph;
        Ok(Graph {
            data: data.cast(map_from, &map_to),
            metadata: map_to,
            scope,
        })
    }
}

impl<M> NetworkAnalyzer<M>
where
    M: LLM,
{
    #[instrument(level = Level::INFO, skip(self, problem))]
    async fn pin_virtual_problem(
        &self,
        problem: VirtualProblem<(), GraphMetadataRaw>,
    ) -> Result<VirtualProblem> {
        let VirtualProblem {
            analyzer: (),
            filter,
            scope,
            spec: ProblemSpec { metadata, verbose },
        } = problem;

        let (analyzer, metadata) = self.pin_graph_metadata_raw(metadata).await?;

        Ok(VirtualProblem {
            analyzer,
            filter,
            scope,
            spec: ProblemSpec { metadata, verbose },
        })
    }

    #[instrument(level = Level::INFO, skip(self, metadata))]
    async fn pin_graph_metadata_raw(
        &self,
        metadata: GraphMetadataRaw,
    ) -> Result<(VirtualProblemAnalyzer, GraphMetadataStandard)> {
        // TODO: to be implemented
        // let prompt = self.prompt.build(&metadata)?;
        // let response = self
        //     .llm
        //     .invoke(&prompt)
        //     .await
        //     .map_err(|error| anyhow!("failed to execute LLM: {error}"))?;
        // println!("{response}");

        let analyzer = VirtualProblemAnalyzer::LLM(VirtualProblemLLMAnalyzer {
            map: metadata.to_pinned(),
            original_metadata: metadata,
        });

        let metadata = GraphMetadataStandard::default();
        Ok((analyzer, metadata))
    }
}

fn pin_problem(problem: NetworkProblemCrd) -> VirtualProblem<(), GraphMetadataRaw> {
    let scope = GraphScope::from_resource(&problem);

    VirtualProblem {
        analyzer: (),
        filter: GraphFilter::all(scope.namespace.clone()),
        scope,
        spec: problem.spec,
    }
}

trait VirtualProblemAnalyzerExt {
    fn to_llm(&self) -> Result<&VirtualProblemLLMAnalyzer>;
}

impl VirtualProblemAnalyzerExt for VirtualProblemAnalyzer {
    fn to_llm(&self) -> Result<&VirtualProblemLLMAnalyzer> {
        match self {
            Self::LLM(analyzer) => Ok(analyzer),
            analyzer => {
                let name = analyzer.name();
                bail!("unexpected analyzer: {name}")
            }
        }
    }
}
