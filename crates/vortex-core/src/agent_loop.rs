use futures_util::StreamExt;
use std::{collections::BTreeSet, error::Error, fmt, sync::Arc};
use tokio::sync::mpsc;

use crate::{
    ConversationItem, FinishReason, MessageRole, ModelEvent, ModelProvider, ModelRequest,
    ProviderError, SessionMessage, ToolCall, ToolRegistry, ToolResult,
};

const MAX_TOOL_ROUNDS: usize = 4;

#[derive(Debug)]
pub(crate) enum TurnProgress {
    TextDelta {
        delta: String,
    },
    ToolStarted {
        call: ToolCall,
    },
    ToolFinished {
        assistant_prefix: Option<String>,
        call: ToolCall,
        result: ToolResult,
    },
}

#[derive(Debug)]
pub(crate) struct TurnOutput {
    pub(crate) assistant: SessionMessage,
}

#[derive(Debug)]
pub(crate) enum TurnError {
    Provider(ProviderError),
    InvalidModelSequence(String),
    ToolRoundLimit { limit: usize },
    ProgressChannelClosed,
}

impl fmt::Display for TurnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => write!(formatter, "{error}"),
            Self::InvalidModelSequence(message) => {
                write!(formatter, "invalid model event sequence: {message}")
            }
            Self::ToolRoundLimit { limit } => {
                write!(formatter, "tool round limit exceeded: {limit}")
            }
            Self::ProgressChannelClosed => {
                write!(formatter, "turn progress channel is closed")
            }
        }
    }
}

impl Error for TurnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ProviderError> for TurnError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

async fn send_progress(
    sender: &mpsc::Sender<TurnProgress>,
    progress: TurnProgress,
) -> Result<(), TurnError> {
    sender
        .send(progress)
        .await
        .map_err(|_| TurnError::ProgressChannelClosed)
}

pub(crate) async fn run_turn(
    provider: Arc<dyn ModelProvider>,
    tools: ToolRegistry,
    history: Vec<ConversationItem>,
    progress: mpsc::Sender<TurnProgress>,
) -> Result<TurnOutput, TurnError> {
    let mut context = history;

    // - 从 `history` 收集 ID，阻止复用上一轮用户请求中的调用 ID。
    // - 后续登记新 ID，阻止当前 Turn 的多个模型步骤复用 ID
    let mut seen_tool_call_ids: BTreeSet<String> = context
        .iter()
        .filter_map(|item| match item {
            ConversationItem::ToolCall(call) => Some(call.call_id.clone()),
            _ => None,
        })
        .collect();

    let mut tool_rounds = 0;

    loop {
        let request = ModelRequest {
            items: context.clone(),
            tools: tools.specs(),
        };
        let mut stream = provider.stream(request);
        let mut content = String::new();
        let mut tool_call = None;
        let mut finish_reason = None;

        while let Some(item) = stream.next().await {
            match item? {
                ModelEvent::TextDelta { delta } => {
                    content.push_str(&delta);
                    send_progress(&progress, TurnProgress::TextDelta { delta }).await?;
                }
                ModelEvent::ReasoningDelta { .. } => {
                    return Err(TurnError::InvalidModelSequence(
                        "reasoning output is not enabled in stage A".to_string(),
                    ));
                }
                ModelEvent::ToolCall { call } => {
                    if tool_call.replace(call).is_some() {
                        return Err(TurnError::InvalidModelSequence(
                            "multiple tool calls are not supported in v1".to_string(),
                        ));
                    }
                }
                ModelEvent::Finished { reason } => {
                    finish_reason = Some(reason);
                    break;
                }
            }
        }

        let finish_reason = finish_reason.ok_or_else(|| {
            TurnError::InvalidModelSequence(
                "provider stream ended without a terminal event".to_string(),
            )
        })?;

        match finish_reason {
            FinishReason::Stop => {
                if tool_call.is_some() {
                    return Err(TurnError::InvalidModelSequence(
                        "tool call ended with stop finish reason".to_string(),
                    ));
                }
                if content.is_empty() {
                    return Err(TurnError::InvalidModelSequence(
                        "response does not contain assistant content".to_string(),
                    ));
                }

                return Ok(TurnOutput {
                    assistant: SessionMessage {
                        role: MessageRole::Assistant,
                        content,
                    },
                });
            }
            FinishReason::ToolCalls => {
                let call = tool_call.ok_or_else(|| {
                    TurnError::InvalidModelSequence(
                        "tool_calls finish reason without a tool call".to_string(),
                    )
                })?;

                if tool_rounds >= MAX_TOOL_ROUNDS {
                    return Err(TurnError::ToolRoundLimit {
                        limit: MAX_TOOL_ROUNDS,
                    });
                }

                // 只检查是否为空，不修改Provider 返回的原始 ID.
                if call.call_id.trim().is_empty() {
                    return Err(TurnError::InvalidModelSequence(
                        "tool call id must not be empty".to_string(),
                    ));
                }

                // insert 返回 false，表示集合中已经存在这个ID.
                if !seen_tool_call_ids.insert(call.call_id.clone()) {
                    return Err(TurnError::InvalidModelSequence(format!(
                        "duplicate tool call id: {:?}",
                        call.call_id
                    )));
                }

                tool_rounds += 1;

                send_progress(&progress, TurnProgress::ToolStarted { call: call.clone() }).await?;

                let result = tools.execute(&call).await;
                let assistant_prefix = (!content.is_empty()).then_some(content);

                send_progress(
                    &progress,
                    TurnProgress::ToolFinished {
                        assistant_prefix: assistant_prefix.clone(),
                        call: call.clone(),
                        result: result.clone(),
                    },
                )
                .await?;

                if let Some(prefix) = assistant_prefix {
                    context.push(ConversationItem::Message(SessionMessage {
                        role: MessageRole::Assistant,
                        content: prefix,
                    }));
                }
                context.push(ConversationItem::ToolCall(call));
                context.push(ConversationItem::ToolResult(result));
            }
            FinishReason::Length => {
                return Err(TurnError::InvalidModelSequence(
                    "response was truncated by the output limit".to_string(),
                ));
            }
            FinishReason::ContentFilter => {
                return Err(TurnError::InvalidModelSequence(
                    "response was stopped by content filtering".to_string(),
                ));
            }
        }
    }
}
