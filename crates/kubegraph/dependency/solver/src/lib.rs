use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    analyzer::NetworkAnalyzer,
    dependency::{NetworkDependencyPipeline, NetworkDependencySolverSpec},
    frame::LazyFrame,
    graph::GraphData,
    problem::VirtualProblem,
};

#[derive(Clone, Default)]
pub struct NetworkDependencyGraph {}

#[async_trait]
impl ::kubegraph_api::dependency::NetworkDependencySolver for NetworkDependencyGraph {
    async fn build_pipeline<A>(
        &self,
        analyzer: &A,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<NetworkDependencyPipeline<GraphData<LazyFrame>, A>>
    where
        A: NetworkAnalyzer,
    {
        dbg!(problem);
        // let nodes = match nodes {
        //     LazyFrame::Polars(df) => df,
        //     _ => todo!(),
        // };
        // nodes.
        // dbg!(nodes);
        // todo!();

        // Ok(functions)
        todo!()
    }
}
