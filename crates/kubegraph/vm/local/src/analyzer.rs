use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    analyzer::{VirtualProblemAnalyzer, VirtualProblemAnalyzerType},
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, GraphMetadataStandard},
    problem::VirtualProblem,
};
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
pub struct NetworkAnalyzerArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_ANALYZER",
        value_enum,
        value_name = "IMPL",
        default_value_t = NetworkAnalyzerType::default(),
    )]
    #[serde(default)]
    pub analyzer: NetworkAnalyzerType,

    #[cfg(feature = "analyzer-llm")]
    #[command(flatten)]
    #[serde(default)]
    pub llm: <::kubegraph_analyzer_llm::NetworkAnalyzer as NetworkComponent>::Args,
}

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
    ValueEnum,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum NetworkAnalyzerType {
    #[cfg_attr(not(feature = "analyzer-llm"), default)]
    Disabled,
    #[cfg(feature = "analyzer-llm")]
    #[default]
    LLM,
}

#[derive(Clone)]
pub enum NetworkAnalyzer {
    Disabled,
    #[cfg(feature = "analyzer-llm")]
    LLM(::kubegraph_analyzer_llm::NetworkAnalyzer),
}

#[async_trait]
impl NetworkComponent for NetworkAnalyzer {
    type Args = NetworkAnalyzerArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkAnalyzerArgs {
            analyzer,
            #[cfg(feature = "analyzer-llm")]
            llm,
        } = args;

        match analyzer {
            NetworkAnalyzerType::Disabled => {
                let _ = signal;
                Ok(Self::Disabled)
            }
            #[cfg(feature = "analyzer-llm")]
            NetworkAnalyzerType::LLM => Ok(Self::LLM(
                ::kubegraph_analyzer_llm::NetworkAnalyzer::try_new(llm, signal).await?,
            )),
        }
    }
}

#[async_trait]
impl ::kubegraph_api::analyzer::NetworkAnalyzer for NetworkAnalyzer {
    type Spec = VirtualProblemAnalyzer;

    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        match self {
            Self::Disabled => {
                let Graph {
                    data,
                    metadata: _,
                    scope,
                } = graph;
                let map_from = &problem.analyzer.original_metadata;
                let map_to = problem.spec.metadata;

                Ok(Graph {
                    data: data.cast(map_from, &map_to),
                    metadata: map_to,
                    scope,
                })
            }
            #[cfg(feature = "analyzer-llm")]
            Self::LLM(runtime) => runtime.pin_graph_raw(problem, graph).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self, metadata))]
    async fn pin_graph_metadata_raw(
        &self,
        metadata: GraphMetadataRaw,
    ) -> Result<(VirtualProblemAnalyzer, GraphMetadataStandard)> {
        match self {
            Self::Disabled => {
                let analyzer = VirtualProblemAnalyzer {
                    original_metadata: metadata,
                    r#type: VirtualProblemAnalyzerType::Empty,
                };
                let metadata = {
                    let _ = metadata;
                    GraphMetadataStandard::default()
                };
                Ok((analyzer, metadata))
            }
            #[cfg(feature = "analyzer-llm")]
            Self::LLM(runtime) => runtime.pin_graph_metadata_raw(metadata).await,
        }
    }
}
