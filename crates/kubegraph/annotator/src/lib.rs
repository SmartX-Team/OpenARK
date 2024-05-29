use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{annotator::NetworkAnnotationSpec, vm::Script};

pub struct NetworkAnnotator {}

#[async_trait]
impl<G> ::kubegraph_api::annotator::NetworkAnnotator<G> for NetworkAnnotator
where
    G: Send,
{
    async fn annotate(
        &self,
        graph: G,
        spec: &NetworkAnnotationSpec,
    ) -> Result<NetworkAnnotationSpec<Script>>
    where
        G: 'async_trait,
    {
        todo!()
    }
}
