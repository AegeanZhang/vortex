use serde::Deserialize;
use std::collections::BTreeMap;

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
    //pending_tool_call: Option<PendingToolCall>,
    pending_tool_calls: BTreeMap<u32, PendingToolCall>,
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
                ProviderError::InvalidResponse("[DONE] received without finish_reason".into())
            })?;

            let mut events = Vec::new();

            match reason {
                FinishReason::ToolCalls => {
                    if self.pending_tool_calls.is_empty() {
                        return Err(ProviderError::InvalidResponse(
                            "tool_calls finish reason without a tool call".into(),
                        ));
                    }

                    let mut calls = Vec::new();
                    for pending in std::mem::take(&mut self.pending_tool_calls).into_values() {
                        let call_id =
                            pending
                                .id
                                .filter(|id| !id.trim().is_empty())
                                .ok_or_else(|| {
                                    ProviderError::InvalidResponse("tool call is missing id".into())
                                })?;
                        if pending.name.trim().is_empty() {
                            return Err(ProviderError::InvalidResponse(
                                "tool call is missing function name".into(),
                            ));
                        }
                        let arguments = serde_json::from_str(&pending.arguments).map_err(|_| {
                            ProviderError::InvalidResponse(
                                "tool call arguments are not valid JSON".into(),
                            )
                        })?;
                        calls.push(ToolCall {
                            call_id,
                            name: pending.name,
                            arguments,
                        });
                    }
                    // 全部验证成功才把结果交给调用方，不能一边校验一边向外 yield。
                    events.extend(calls.into_iter().map(|call| ModelEvent::ToolCall { call }));
                }
                _ if !self.pending_tool_calls.is_empty() => {
                    return Err(ProviderError::InvalidResponse(
                        "tool call data ended with a non-tool finish reason".into(),
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
        if delta.kind.as_deref().is_some_and(|kind| kind != "function") {
            return Err(ProviderError::InvalidResponse(
                "unsupported tool call type".into(),
            ));
        }

        let pending = self.pending_tool_calls.entry(delta.index).or_default();

        if let Some(id) = delta.id.filter(|id| !id.is_empty()) {
            match &pending.id {
                Some(current) if current != &id => {
                    return Err(ProviderError::InvalidResponse(
                        "tool call id changed during streaming".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn finish(decoder: &mut OpenAiChatDecoder, reason: &str) -> Vec<ModelEvent> {
        assert!(
            decoder
                .push_data(
                    &json!({"choices": [{"index": 0,
            "delta": {}, "finish_reason": reason}]})
                    .to_string()
                )
                .unwrap()
                .is_empty()
        );
        let events = decoder.push_data("[DONE]").unwrap();
        decoder.finish_eof().unwrap();
        events
    }

    #[test]
    fn emits_reasoning_and_text_from_the_same_chunk() {
        let mut decoder = OpenAiChatDecoder::default();
        assert_eq!(decoder.push_data(r#"{"choices":[{"index":0,"delta":{"reasoning_content":"分析","content":"回答"},"finish_reason":null}]}"#).unwrap(), vec![
            ModelEvent::ReasoningDelta { delta: "分析".into() },
            ModelEvent::TextDelta { delta: "回答".into() },
        ]);
        assert_eq!(
            finish(&mut decoder, "stop"),
            vec![ModelEvent::Finished {
                reason: FinishReason::Stop
            }]
        );
    }

    #[test]
    fn empty_and_null_reasoning_do_not_emit_events() {
        let mut decoder = OpenAiChatDecoder::default();
        for delta in [
            json!({}),
            json!({"reasoning_content": ""}),
            json!({"reasoning_content": null}),
        ] {
            assert!(
                decoder
                    .push_data(&json!({"choices": [{"index": 0, "delta": delta}]}).to_string())
                    .unwrap()
                    .is_empty()
            );
        }
        finish(&mut decoder, "stop");
    }

    #[test]
    fn interleaved_reasoning_and_tool_arguments_keep_separate_buffers() {
        let mut decoder = OpenAiChatDecoder::default();
        for (reasoning, call) in [
            (
                "先查",
                json!({"index": 0, "id": "fixture-call", "type": "function",
                "function": {"name": "web_search", "arguments": "{\"query\":"}}),
            ),
            (
                "再答",
                json!({"index": 0, "id": "", "function": {"arguments": "\"rust\"}"}}),
            ),
        ] {
            assert_eq!(
                decoder
                    .push_data(
                        &json!({"choices": [{"index": 0, "delta": {
                "reasoning_content": reasoning, "tool_calls": [call]}}]})
                        .to_string()
                    )
                    .unwrap(),
                vec![ModelEvent::ReasoningDelta {
                    delta: reasoning.into()
                }]
            );
        }
        assert_eq!(
            finish(&mut decoder, "tool_calls"),
            vec![
                ModelEvent::ToolCall {
                    call: ToolCall {
                        call_id: "fixture-call".into(),
                        name: "web_search".into(),
                        arguments: json!({"query": "rust"})
                    }
                },
                ModelEvent::Finished {
                    reason: FinishReason::ToolCalls
                },
            ]
        );
    }

    #[test]
    fn reasoning_does_not_make_missing_done_a_success() {
        let mut decoder = OpenAiChatDecoder::default();
        decoder.push_data(r#"{"choices":[{"index":0,"delta":{"reasoning_content":"分析"},"finish_reason":"stop"}]}"#).unwrap();
        assert!(matches!(
            decoder.finish_eof(),
            Err(ProviderError::InvalidResponse(_))
        ));
        assert!(matches!(
            decoder.push_data("[DONE]").unwrap().as_slice(),
            [ModelEvent::Finished {
                reason: FinishReason::Stop
            }]
        ));
    }

    #[test]
    fn reasoning_does_not_relax_tool_call_validation() {
        let mut decoder = OpenAiChatDecoder::default();
        decoder
            .push_data(
                &json!({"choices":[{"index":0,"delta":{
                    "reasoning_content":"R",
                    "tool_calls":[{"index":0,"id":"fixture-call","function":{
                        "name":"web_search","arguments":"{}"
                    }}]
                }}]})
                .to_string(),
            )
            .unwrap();
        assert!(matches!(
            decoder.push_data(
                &json!({"choices":[{"index":0,"delta":{
                    "tool_calls":[{"index":0,"id":"changed-id"}]
                }}]})
                .to_string()
            ),
            Err(ProviderError::InvalidResponse(_))
        ));
    }

    #[test]
    fn multiple_calls_accumulate_separately_and_emit_in_index_order() {
        let mut decoder = OpenAiChatDecoder::default();
        for call in [
            json!({"index":1,"id":"b","type":"function","function":{"name":"web_search","arguments":""}}),
            json!({"index":0,"id":"a","type":"function","function":{"name":"web_search","arguments":""}}),
            json!({"index":1,"function":{"arguments":"{\"query\":"}}),
            json!({"index":0,"id":"","function":{"arguments":"{\"query\":\"A\"}"}}),
            json!({"index":1,"function":{"arguments":"\"B\"}"}}),
        ] {
            assert!(
                decoder
                    .push_data(
                        &json!({"choices":[{"index":0,"delta":{
                            "tool_calls":[call]
                        }}]})
                        .to_string()
                    )
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(
            finish(&mut decoder, "tool_calls"),
            vec![
                ModelEvent::ToolCall {
                    call: ToolCall {
                        call_id: "a".into(),
                        name: "web_search".into(),
                        arguments: json!({"query":"A"}),
                    }
                },
                ModelEvent::ToolCall {
                    call: ToolCall {
                        call_id: "b".into(),
                        name: "web_search".into(),
                        arguments: json!({"query":"B"}),
                    }
                },
                ModelEvent::Finished {
                    reason: FinishReason::ToolCalls
                },
            ]
        );
    }

    #[test]
    fn an_invalid_second_call_does_not_publish_the_first_call() {
        let mut decoder = OpenAiChatDecoder::default();
        let events = decoder
            .push_data(
                &json!({"choices":[{"index":0,"delta":{
        "reasoning_content":"R",
        "tool_calls":[
            {"index":0,"id":"a","function":{"name":"web_search","arguments":"{}"}},
            {"index":1,"id":"b","function":{"name":"web_search","arguments":"bad-json"}}
        ]
    },"finish_reason":"tool_calls"}]})
                .to_string(),
            )
            .unwrap();
        assert_eq!(
            events,
            vec![ModelEvent::ReasoningDelta { delta: "R".into() }]
        );
        assert!(decoder.push_data("[DONE]").is_err());
        assert!(!decoder.is_done());
    }
}
