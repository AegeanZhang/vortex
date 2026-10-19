//use super::QwenThinkingOptions;
use super::options::ThinkingRequestFields;
use serde::Serialize;
use vortex_core::{ConversationItem, MessageRole, ModelRequest, ToolCall, ToolSpec};

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
    /*
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preserve_thinking: Option<bool>,
    */
    #[serde(flatten)]
    thinking_fields: ThinkingRequestFields,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool>,
}

impl ChatRequest {
    pub(super) fn from_model_request(
        model: String,
        model_request: ModelRequest,
        //thinking: Option<QwenThinkingOptions>,
        thinking_fields: ThinkingRequestFields,
    ) -> Self {
        let ModelRequest { items, tools } = model_request;

        Self {
            model,
            messages: map_messages(items),
            tools: tools.into_iter().map(map_tool_spec).collect(),
            stream: true,
            //enable_thinking: thinking.map(|options| options.enabled),
            //preserve_thinking: thinking.map(|options| options.preserve_history),
            thinking_fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai_chat::{OpenAiChatDialect, OpenAiChatOptions};
    use serde_json::{Value, json};
    use vortex_core::{
        SessionMessage, ThinkingCapabilities, ThinkingMode, ThinkingOptions, ToolResult,
    };

    fn thinking_fields(
        dialect: OpenAiChatDialect,
        supports_history_control: bool,
    ) -> ThinkingRequestFields {
        OpenAiChatOptions {
            dialect,
            thinking: ThinkingOptions {
                mode: ThinkingMode::Disabled,
            },
            thinking_capabilities: ThinkingCapabilities {
                can_disable: true,
                supports_history_control,
            },
        }
        .into_request_fields()
        .unwrap()
    }

    #[test]
    fn default_request_preserves_minimal_shape() {
        let request = ChatRequest::from_model_request(
            "fixture-model".into(),
            ModelRequest {
                items: vec![ConversationItem::Message(SessionMessage {
                    role: MessageRole::User,
                    content: "fixture prompt".into(),
                })],
                tools: vec![],
            },
            ThinkingRequestFields::default(),
        );
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "model": "fixture-model",
                "messages": [{"role": "user", "content": "fixture prompt"}],
                "stream": true,
            })
        );
    }

    #[test]
    fn changing_profile_only_changes_thinking_fields() {
        let fixture = ModelRequest {
            items: vec![
                ConversationItem::Message(SessionMessage {
                    role: MessageRole::User,
                    content: "fixture prompt".into(),
                }),
                ConversationItem::Message(SessionMessage {
                    role: MessageRole::Assistant,
                    content: "fixture prefix".into(),
                }),
                ConversationItem::ToolCall(ToolCall {
                    call_id: "fixture-call".into(),
                    name: "web_search".into(),
                    arguments: json!({"query": "rust"}),
                }),
                ConversationItem::ToolResult(ToolResult {
                    call_id: "fixture-call".into(),
                    output: "fixture result".into(),
                    is_error: false,
                }),
            ],
            tools: vec![ToolSpec {
                name: "web_search".into(),
                description: "fixture tool".into(),
                input_schema: json!({"type": "object"}),
            }],
        };
        let base = json!({
            "model": "fixture-model",
            "stream": true,
            "messages": [
                {"role": "user", "content": "fixture prompt"},
                {
                    "role": "assistant",
                    "content": "fixture prefix",
                    "tool_calls": [{
                        "id": "fixture-call",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": "{\"query\":\"rust\"}"},
                    }],
                },
                {"role": "tool", "tool_call_id": "fixture-call", "content": "fixture result"},
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "web_search",
                    "description": "fixture tool",
                    "parameters": {"type": "object"},
                },
            }],
        });

        for (dialect, history) in [
            (OpenAiChatDialect::DashScope, true),
            (OpenAiChatDialect::DeepSeek, false),
            (OpenAiChatDialect::DashScope, false),
        ] {
            let request = ChatRequest::from_model_request(
                "fixture-model".into(),
                fixture.clone(),
                thinking_fields(dialect, history),
            );
            let mut expected: Value = base.clone();
            match dialect {
                OpenAiChatDialect::DashScope => {
                    expected["enable_thinking"] = json!(false);
                    if history {
                        expected["preserve_thinking"] = json!(false);
                    }
                }
                OpenAiChatDialect::DeepSeek => {
                    expected["thinking"] = json!({"type": "disabled"});
                }
                OpenAiChatDialect::Generic => unreachable!("fixture only uses supported dialects"),
            }
            assert_eq!(serde_json::to_value(request).unwrap(), expected);
        }
    }
}
