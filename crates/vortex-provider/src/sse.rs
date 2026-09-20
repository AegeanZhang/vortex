use serde::Deserialize;

use vortex_core::{FinishReason, ModelEvent, ProviderError};

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
    tool_calls: Option<serde_json::Value>,
    function_call: Option<serde_json::Value>,
}

#[derive(Default)]
pub(crate) struct StreamDecoder {
    finish_reason: Option<FinishReason>,
    done: bool,
}

impl StreamDecoder {
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

            self.done = true;

            return Ok(vec![ModelEvent::Finished { reason }]);
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

        if choice.delta.tool_calls.is_some() || choice.delta.function_call.is_some() {
            return Err(ProviderError::InvalidResponse(
                "tool call streaming is not supported".to_string(),
            ));
        }

        let finish_reason = match choice.finish_reason.as_deref() {
            None => None,
            Some("stop") => Some(FinishReason::Stop),
            Some("length") => Some(FinishReason::Length),
            Some("content_filter") => Some(FinishReason::ContentFilter),
            Some(_) => {
                return Err(ProviderError::InvalidResponse(
                    "unsupported finish_reason".to_string(),
                ));
            }
        };

        let mut events = Vec::new();

        // 两个字段分别处理，不能使用 else if。
        if let Some(delta) = choice.delta.reasoning_content {
            if !delta.is_empty() {
                events.push(ModelEvent::ReasoningDelta { delta });
            }
        }

        if let Some(delta) = choice.delta.content {
            if !delta.is_empty() {
                events.push(ModelEvent::TextDelta { delta });
            }
        }

        // 当前包可能既有最后一段正文，也有结束原因。
        // 正文照常返回；Finished 留到收到 [DONE] 时再产生。
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
}
