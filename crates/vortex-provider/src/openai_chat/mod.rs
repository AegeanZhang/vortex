mod decoder;
mod request;
mod stream;

use self::{request::ChatRequest, stream::StreamContext};

use vortex_core::{ModelProvider, ModelRequest, ModelStream};

#[derive(Clone)]
struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QwenThinkingOptions {
    pub enabled: bool,
    pub preserve_history: bool,
}

pub struct OpenAiChatProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: SecretString,
    qwen_thinking: Option<QwenThinkingOptions>,
}

impl OpenAiChatProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            model: model.into(),
            api_key: SecretString(api_key.into()),
            qwen_thinking: None,
        }
    }

    pub fn with_qwen_thinking(mut self, options: QwenThinkingOptions) -> Self {
        self.qwen_thinking = Some(options);
        self
    }
}

impl ModelProvider for OpenAiChatProvider {
    fn stream(&self, model_request: ModelRequest) -> ModelStream {
        let request =
            ChatRequest::from_model_request(self.model.clone(), model_request, self.qwen_thinking);

        let context = StreamContext {
            client: self.client.clone(),
            url: format!("{}/chat/completions", self.base_url.trim_end_matches('/'),),
            api_key: self.api_key.clone(),
            request,
        };

        stream::stream(context)
    }
}
