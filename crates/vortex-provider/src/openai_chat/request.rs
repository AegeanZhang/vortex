use super::QwenThinkingOptions;
use serde::Serialize;
use vortex_core::{ConversationItem, MessageRole, ModelRequest, ToolCall, ToolSpec};

/*#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}*/

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatMessage {
    Text {
        role: &'static str,
        content: String,
    },
    AssistantToolCall {
        role: &'static str,
        content: Option<String>,
        tool_calls: Vec<ChatToolCall>,
    },
    ToolResult {
        role: &'static str,
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct ChatToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionCall,
}

#[derive(Debug, Serialize)]
struct ChatFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct ChatTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct ChatFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/*impl From<SessionMessage> for ChatMessage {
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
}*/

fn map_tool_call(call: ToolCall, content: Option<String>) -> ChatMessage {
    ChatMessage::AssistantToolCall {
        role: "assistant",
        content,
        tool_calls: vec![ChatToolCall {
            id: call.call_id,
            kind: "function",
            function: ChatFunctionCall {
                name: call.name,
                arguments: call.arguments.to_string(),
            },
        }],
    }
}

fn map_tool_spec(spec: ToolSpec) -> ChatTool {
    ChatTool {
        kind: "function",
        function: ChatFunctionDefinition {
            name: spec.name,
            description: spec.description,
            parameters: spec.input_schema,
        },
    }
}

fn map_messages(items: Vec<ConversationItem>) -> Vec<ChatMessage> {
    let mut source = items.into_iter().peekable();
    let mut messages = Vec::new();

    while let Some(item) = source.next() {
        match item {
            ConversationItem::Message(message) => {
                let role = match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                };

                if message.role == MessageRole::Assistant
                    && matches!(source.peek(), Some(ConversationItem::ToolCall(_)))
                {
                    let Some(ConversationItem::ToolCall(call)) = source.next() else {
                        unreachable!("peek confirmed the next item is a tool call");
                    };

                    messages.push(map_tool_call(call, Some(message.content)));
                } else {
                    messages.push(ChatMessage::Text {
                        role,
                        content: message.content,
                    });
                }
            }
            ConversationItem::ToolCall(call) => {
                messages.push(map_tool_call(call, None));
            }
            ConversationItem::ToolResult(result) => {
                messages.push(ChatMessage::ToolResult {
                    role: "tool",
                    tool_call_id: result.call_id,
                    content: result.output,
                });
            }
        }
    }

    messages
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool>,
}

impl ChatRequest {
    /*pub(super) fn from_model_request(
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
    }*/

    pub(super) fn from_model_request(
        model: String,
        model_request: ModelRequest,
        thinking: Option<QwenThinkingOptions>,
    ) -> Self {
        let ModelRequest { items, tools } = model_request;

        Self {
            model,
            messages: map_messages(items),
            tools: tools.into_iter().map(map_tool_spec).collect(),
            stream: true,
            enable_thinking: thinking.map(|options| options.enabled),
            preserve_thinking: thinking.map(|options| options.preserve_history),
        }
    }
}
