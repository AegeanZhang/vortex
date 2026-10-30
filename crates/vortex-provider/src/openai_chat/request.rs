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
    AssistantToolCalls {
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

fn map_tool_calls(calls: Vec<ToolCall>, content: Option<String>) -> ChatMessage {
    ChatMessage::AssistantToolCalls {
        role: "assistant",
        content,
        tool_calls: calls
            .into_iter()
            .map(|call| ChatToolCall {
                id: call.call_id,
                kind: "function",
                function: ChatFunctionCall {
                    name: call.name,
                    arguments: call.arguments.to_string(),
                },
            })
            .collect(),
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
    items
        .into_iter()
        .map(|item| match item {
            ConversationItem::Message(message) => ChatMessage::Text {
                role: match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                },
                content: message.content,
            },
            ConversationItem::AssistantToolCalls { content, calls } => {
                map_tool_calls(calls, content)
            }
            ConversationItem::ToolResult(result) => ChatMessage::ToolResult {
                role: "tool",
                tool_call_id: result.call_id,
                content: result.output,
            },
        })
        .collect()
}

// Wire 类型
#[derive(Debug, Serialize)]
pub(super) struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(flatten)]
    thinking_fields: ThinkingRequestFields,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool>,
}

impl ChatRequest {
    pub(super) fn from_model_request(
        model: String,
        model_request: ModelRequest,
        thinking_fields: ThinkingRequestFields,
    ) -> Self {
        let ModelRequest { items, tools } = model_request;

        Self {
            model,
            messages: map_messages(items),
            tools: tools.into_iter().map(map_tool_spec).collect(),
            stream: true,
            thinking_fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai_chat::{OpenAiChatDialect, OpenAiChatOptions, ReasoningHistoryPolicy};
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
            history_policy: ReasoningHistoryPolicy::Unverified,
            thinking: ThinkingOptions {
                mode: ThinkingMode::Disabled,
            },
            thinking_capabilities: ThinkingCapabilities {
                can_disable: true,
                can_enable: false,
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
    fn enabled_request_serializes_thinking_fields_at_the_top_level() {
        let request = ChatRequest::from_model_request(
            "fixture-model".into(),
            ModelRequest {
                items: vec![],
                tools: vec![],
            },
            OpenAiChatOptions {
                dialect: OpenAiChatDialect::DashScope,
                history_policy: ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled,
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Enabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_enable: true,
                    can_disable: true,
                    supports_history_control: true,
                },
            }
            .into_request_fields()
            .unwrap(),
        );
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "model": "fixture-model", "messages": [], "stream": true,
                "enable_thinking": true, "preserve_thinking": false,
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
                ConversationItem::AssistantToolCalls {
                    content: Some("fixture prefix".into()),
                    calls: vec![ToolCall {
                        call_id: "fixture-call".into(),
                        name: "web_search".into(),
                        arguments: json!({"query": "rust"}),
                    }],
                },
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

    #[test]
    fn a_batch_maps_to_one_assistant_and_all_tool_results() {
        let calls: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|id| ToolCall {
                call_id: id.into(),
                name: "web_search".into(),
                arguments: json!({"query":id}),
            })
            .collect();
        let mut items = vec![ConversationItem::AssistantToolCalls {
            content: Some("prefix".into()),
            calls: calls.clone(),
        }];
        items.extend(calls.iter().map(|call| {
            ConversationItem::ToolResult(ToolResult {
                call_id: call.call_id.clone(),
                output: format!("result {}", call.call_id),
                is_error: false,
            })
        }));
        let request = ChatRequest::from_model_request(
            "fixture-model".into(),
            ModelRequest {
                items,
                tools: vec![],
            },
            ThinkingRequestFields::default(),
        );
        let value = serde_json::to_value(request).unwrap();
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(
            messages,
            json!([
                {
                    "role": "assistant", "content": "prefix",
                    "tool_calls": [
                        {"id": "a", "type": "function", "function": {
                            "name": "web_search", "arguments": "{\"query\":\"a\"}"
                        }},
                        {"id": "b", "type": "function", "function": {
                            "name": "web_search", "arguments": "{\"query\":\"b\"}"
                        }},
                    ],
                },
                {"role": "tool", "tool_call_id": "a", "content": "result a"},
                {"role": "tool", "tool_call_id": "b", "content": "result b"},
            ])
            .as_array()
            .unwrap()
        );
        assert!(!value.to_string().contains("reasoning_content"));
    }
}
