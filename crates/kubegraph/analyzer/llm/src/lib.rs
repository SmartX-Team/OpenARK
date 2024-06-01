mod llm_model;
mod prompt;

use anyhow::{bail, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    analyzer::{
        llm::VirtualProblemAnalyzerLLM, VirtualProblemAnalyzer, VirtualProblemAnalyzerType,
    },
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, GraphMetadataStandard},
    problem::VirtualProblem,
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
    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        let VirtualProblemAnalyzer {
            original_metadata: map_from,
            r#type: VirtualProblemAnalyzerLLM {},
        } = problem.analyzer.clone().try_into_llm()?;
        let map_to = problem.spec.metadata;

        // TODO: to be implemented
        let Graph {
            data,
            metadata,
            scope,
        } = graph;
        Ok(Graph {
            data: data.cast(&map_from, &map_to),
            metadata: map_to,
            scope,
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

        let analyzer = VirtualProblemAnalyzer {
            original_metadata: metadata,
            r#type: VirtualProblemAnalyzerType::LLM(VirtualProblemAnalyzerLLM {}),
        };

        let metadata = GraphMetadataStandard::default();
        Ok((analyzer, metadata))
    }
}

trait VirtualProblemAnalyzerExt {
    fn try_into_llm(self) -> Result<VirtualProblemAnalyzer<VirtualProblemAnalyzerLLM>>;
}

impl VirtualProblemAnalyzerExt for VirtualProblemAnalyzer {
    fn try_into_llm(self) -> Result<VirtualProblemAnalyzer<VirtualProblemAnalyzerLLM>> {
        let Self {
            original_metadata,
            r#type,
        } = self;

        match r#type {
            VirtualProblemAnalyzerType::LLM(r#type) => Ok(VirtualProblemAnalyzer {
                original_metadata,
                r#type,
            }),
            analyzer => {
                let name = analyzer.name();
                bail!("unexpected analyzer: {name}")
            }
        }
    }
}
