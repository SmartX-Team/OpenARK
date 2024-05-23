use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use langchain_rust::{
    language_models::{llm::LLM, options::CallOptions, GenerateResult, LLMError},
    llm::{OpenAI, OpenAIConfig},
    schemas::{Message, StreamData},
};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct GenericLLM {
    openai: OpenAI<OpenAIConfig>,
}

impl Default for GenericLLM {
    fn default() -> Self {
        Self {
            openai: OpenAI::default()
                .with_config(OpenAIConfig::default())
                // .with_model(OpenAIModel::Gpt4.to_string());
                .with_model("gpt-4o")
                .with_options(default_options()),
        }
    }
}

impl GenericLLM {
    fn get_default(&self) -> &impl LLM {
        &self.openai
    }

    fn get_default_mut(&mut self) -> &mut impl LLM {
        &mut self.openai
    }
}

#[async_trait]
impl LLM for GenericLLM {
    #[instrument(level = Level::INFO, skip(self, messages))]
    async fn generate(&self, messages: &[Message]) -> Result<GenerateResult, LLMError> {
        self.get_default().generate(messages).await
    }

    #[instrument(level = Level::INFO, skip(self, prompt))]
    async fn invoke(&self, prompt: &str) -> Result<String, LLMError> {
        self.get_default().invoke(prompt).await
    }

    #[instrument(level = Level::INFO, skip(self, messages))]
    async fn stream(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamData, LLMError>> + Send>>, LLMError> {
        self.get_default().stream(messages).await
    }

    fn add_options(&mut self, options: CallOptions) {
        self.get_default_mut().add_options(options)
    }

    //This is usefull when using non chat models
    fn messages_to_string(&self, messages: &[Message]) -> String {
        self.get_default().messages_to_string(messages)
    }
}

fn default_options() -> CallOptions {
    CallOptions {
        seed: Some(980904),
        temperature: Some(0.0),
        ..Default::default()
    }
}
