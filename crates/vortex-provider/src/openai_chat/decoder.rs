use serde::Deserialize;

use vortex_core::{FinishReason, ModelEvent, ProviderError, ToolCall};

#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    index: u32,
    #[serde(default)]
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct ChunkDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    //tool_calls: Option<serde_json::Value>,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
    function_call: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ToolCallDelta {
    index: u32,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Default, Deserialize)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Default)]
struct PendingToolCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub(crate) struct OpenAiChatDecoder {
    finish_reason: Option<FinishReason>,
    pending_tool_call: Option<PendingToolCall>,
    done: bool,
}

impl OpenAiChatDecoder {
    pub(crate) fn push_data(&mut self, data: &str) -> Result<Vec<ModelEvent>, ProviderError> {
        if self.done {
            return Err(ProviderError::InvalidResponse(
                "received data after [DONE]".to_string(),
            ));
        }

        if data.trim().is_empty() {
            return Ok(Vec::new());
        }

        // SSE 的结束标记不是 JSON，必须先单独处理。
        if data.trim() == "[DONE]" {
            let reason = self.finish_reason.take().ok_or_else(|| {
                ProviderError::InvalidResponse("[DONE] received without finish_reason".to_string())
            })?;

            let mut events = Vec::new();

            match reason {
                FinishReason::ToolCalls => {
                    let pending = self.pending_tool_call.take().ok_or_else(|| {
                        ProviderError::InvalidResponse(
                            "tool_calls finish reason without a tool call".to_string(),
                        )
                    })?;

                    let call_id = pending.id.ok_or_else(|| {
                        ProviderError::InvalidResponse("tool call is missing id".to_string())
                    })?;

                    if pending.name.is_empty() {
                        return Err(ProviderError::InvalidResponse(
                            "tool call is missing function name".to_string(),
                        ));
                    }

                    let arguments = serde_json::from_str(&pending.arguments).map_err(|_| {
                        ProviderError::InvalidResponse(
                            "tool call arguments are not valid Json".to_string(),
                        )
                    })?;

                    events.push(ModelEvent::ToolCall {
                        call: ToolCall {
                            call_id,
                            name: pending.name,
                            arguments,
                        },
                    });
                }
                _ if self.pending_tool_call.is_some() => {
                    return Err(ProviderError::InvalidResponse(
                        "tool call data ended with a non-tool finish reason".to_string(),
                    ));
                }
                _ => {}
            }

            self.done = true;
            events.push(ModelEvent::Finished { reason });
            return Ok(events);
        }

        let chunk: ChatChunk = serde_json::from_str(data)
            .map_err(|_| ProviderError::InvalidResponse("invalid JSON in SSE data".to_string()))?;

        if chunk.error.is_some() {
            return Err(ProviderError::InvalidResponse(
                "provider returned an error event".to_string(),
            ));
        }

        // 例如 usage 统计包，没有 choice，不产生模型事件。
        if chunk.choices.is_empty() {
            return Ok(Vec::new());
        }

        if chunk.choices.len() != 1 {
            return Err(ProviderError::InvalidResponse(
                "multiple choices are not supported".to_string(),
            ));
        }

        let choice = chunk
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::InvalidResponse("missing choice".to_string()))?;

        if choice.index != 0 {
            return Err(ProviderError::InvalidResponse(
                "only choice index 0 is supported".to_string(),
            ));
        }

        // 已有结束原因后，只允许统计包或 [DONE]。
        if self.finish_reason.is_some() {
            return Err(ProviderError::InvalidResponse(
                "received choice data after finish_reason".to_string(),
            ));
        }

        let ChunkDelta {
            content,
            reasoning_content,
            tool_calls,
            function_call,
        } = choice.delta;

        if function_call.is_some() {
            return Err(ProviderError::InvalidResponse(
                "legacy function_call streaming is not supported".to_string(),
            ));
        }

        for tool_call in tool_calls {
            self.push_tool_call_delta(tool_call)?;
        }

        let finish_reason = match choice.finish_reason.as_deref() {
            None => None,
            Some("stop") => Some(FinishReason::Stop),
            Some("tool_calls") => Some(FinishReason::ToolCalls),
            Some("length") => Some(FinishReason::Length),
            Some("content_filter") => Some(FinishReason::ContentFilter),
            Some(_) => {
                return Err(ProviderError::InvalidResponse(
                    "unsupported finish_reason".to_string(),
                ));
            }
        };

        let mut events = Vec::new();

        if let Some(delta) = reasoning_content
            && !delta.is_empty()
        {
            events.push(ModelEvent::ReasoningDelta { delta });
        }

        if let Some(delta) = content
            && !delta.is_empty()
        {
            events.push(ModelEvent::TextDelta { delta });
        }

        self.finish_reason = finish_reason;
        Ok(events)
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }

    pub(crate) fn finish_eof(&self) -> Result<(), ProviderError> {
        if self.done {
            Ok(())
        } else {
            Err(ProviderError::InvalidResponse(
                "stream ended before [DONE]".to_string(),
            ))
        }
    }

    // 累积函数，把多个SSE chunk 中零散的Tool call信息累积到 OpenAiChatDecoder.pending_tool_call
    fn push_tool_call_delta(&mut self, delta: ToolCallDelta) -> Result<(), ProviderError> {
        if delta.index != 0 {
            return Err(ProviderError::InvalidResponse(
                "multiple tool calls are not supported in v1".to_string(),
            ));
        }

        if delta.kind.as_deref().is_some_and(|kind| kind != "function") {
            return Err(ProviderError::InvalidResponse(
                "unsupported tool call type".to_string(),
            ));
        }

        let pending = self.pending_tool_call.get_or_insert_with(Default::default);

        //if let Some(id) = delta.id {
        if let Some(id) = delta.id.filter(|id| !id.is_empty()) {
            match &pending.id {
                Some(current) if current != &id => {
                    return Err(ProviderError::InvalidResponse(
                        "tool call id changed during streaming".to_string(),
                    ));
                }
                None => pending.id = Some(id),
                Some(_) => {}
            }
        }

        if let Some(function) = delta.function {
            if let Some(name) = function.name {
                pending.name.push_str(&name);
            }
            if let Some(arguments) = function.arguments {
                pending.arguments.push_str(&arguments);
            }
        }

        Ok(())
    }
}
