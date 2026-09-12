use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use vortex_core::{MessageRole, ModelProvider, ModelRequest, ProviderError, ProviderFuture};

pub struct OpenAiChatProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: SecretString,
}

struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
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
        }
    }
}

// Wire 类型
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

impl ModelProvider for OpenAiChatProvider {
    fn complete(&self, request: ModelRequest) -> ProviderFuture<'_> {
        Box::pin(async move {
            let messages = request
                .messages
                .into_iter()
                .map(|message| ChatMessage {
                    role: match message.role {
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::System => "system",
                    },
                    content: message.content,
                })
                .collect();

            let request = ChatRequest {
                model: self.model.clone(),
                messages,
                stream: false,
            };

            debug!(
                model = %request.model,
                messages_count = request.messages.len(),
                stream = request.stream,
                "provider request build"
            );

            trace!(
                request = ?request,
                "provider request body",
            );

            let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'),);

            let response = self
                .client
                .post(url)
                .bearer_auth(&self.api_key.0)
                .json(&request)
                .send()
                .await
                .map_err(|error| ProviderError::Transport(error.to_string()))?;

            let status = response.status();

            if !status.is_success() {
                return Err(ProviderError::HttpStatus {
                    status: status.as_u16(),
                });
            }

            let response = response
                .json::<ChatResponse>()
                .await
                .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;

            response
                .choices
                .into_iter()
                .next()
                .and_then(|choice| choice.message.content)
                .filter(|content| !content.is_empty())
                .ok_or_else(|| {
                    ProviderError::InvalidResponse(
                        "response does not contain assistant content".to_string(),
                    )
                })
        })
    }
}
