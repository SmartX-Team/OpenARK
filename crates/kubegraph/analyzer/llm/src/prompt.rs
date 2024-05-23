use anyhow::Result;
use kubegraph_api::graph::GraphMetadataRaw;

#[derive(Clone)]
pub(crate) struct PromptLoader {}

impl PromptLoader {
    pub(crate) async fn try_default() -> Result<Self> {
        Ok(Self {})
    }

    pub(crate) fn build(&self, metadata: &GraphMetadataRaw) -> Result<String> {
        Ok(format!("hello world"))
    }
}
