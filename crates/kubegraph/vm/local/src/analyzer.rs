use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, GraphMetadataStandard},
    problem::VirtualProblem,
    resource::NetworkResourceCollectionDB,
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
    #[instrument(level = Level::INFO, skip(self, resource_db))]
    async fn inspect(
        &self,
        resource_db: &dyn NetworkResourceCollectionDB,
    ) -> Result<Vec<VirtualProblem>> {
        match self {
            Self::Disabled => {
                let _ = resource_db;
                Ok(Vec::default())
            }
            #[cfg(feature = "analyzer-llm")]
            Self::LLM(runtime) => runtime.inspect(resource_db).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        match self {
            Self::Disabled => {
                let _ = problem;
                let Graph {
                    data,
                    metadata: _,
                    scope,
                } = graph;
                Ok(Graph {
                    data,
                    metadata: GraphMetadataStandard::default(),
                    scope,
                })
            }
            #[cfg(feature = "analyzer-llm")]
            Self::LLM(runtime) => runtime.pin_graph_raw(problem, graph).await,
        }
    }
}
