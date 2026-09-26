use super::QwenThinkingOptions;
use serde::Serialize;
use vortex_core::{MessageRole, ModelRequest, SessionMessage};

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

impl From<SessionMessage> for ChatMessage {
    fn from(message: SessionMessage) -> Self {
        Self {
            role: match message.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
            },
            content: message.content,
        }
    }
}

// Wire 类型
#[derive(Debug, Serialize)]
pub(super) struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preserve_thinking: Option<bool>,
}

impl ChatRequest {
    pub(super) fn from_model_request(
        model: String,
        model_request: ModelRequest,
        thinking: Option<QwenThinkingOptions>,
    ) -> Self {
        let messages = model_request
            .messages
            .into_iter()
            .map(ChatMessage::from)
            .collect();

        Self {
            model,
            messages,
            stream: true,
            enable_thinking: thinking.map(|options| options.enabled),
            preserve_thinking: thinking.map(|options| options.preserve_history),
        }
    }
}
