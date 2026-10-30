use futures_util::StreamExt;
use std::{collections::BTreeSet, error::Error, fmt, sync::Arc};
use tokio::sync::mpsc;

use crate::{
    ConversationItem, FinishReason, MessageRole, ModelEvent, ModelProvider, ModelRequest,
    ProviderError, ReasoningOutputPolicy, SessionMessage, ToolBudgetExceeded, ToolCall,
    ToolCallLimits, ToolRegistry, ToolResult, tool_budget::TurnToolBudget,
};

//const MAX_TOOL_ROUNDS: usize = 4;
const MAX_REASONING_BYTES_PER_MODEL_STEP: usize = 256 * 1024;

#[derive(Debug)]
pub(crate) enum TurnProgress {
    TextDelta {
        delta: String,
    },
    ToolStarted {
        call: ToolCall,
    },
    ToolBatchStarted {
        content: Option<String>,
        calls: Vec<ToolCall>,
    },
    ToolFinished {
        //assistant_prefix: Option<String>,
        call: ToolCall,
        result: ToolResult,
    },
    ModelStepStarted {
        step: u32,
    },
    ReasoningDelta {
        delta: String,
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
    //ToolRoundLimit { limit: usize },
    ToolBudgetExceeded(ToolBudgetExceeded),
    ProgressChannelClosed,
    ReasoningLimitExceeded { limit: usize },
}

impl fmt::Display for TurnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => write!(formatter, "{error}"),
            Self::InvalidModelSequence(message) => {
                write!(formatter, "invalid model event sequence: {message}")
            }
            //Self::ToolRoundLimit { limit } => {
            Self::ToolBudgetExceeded(error) => {
                write!(formatter, "{error}")
            }
            Self::ProgressChannelClosed => {
                write!(formatter, "turn progress channel is closed")
            }
            Self::ReasoningLimitExceeded { limit } => {
                write!(formatter, "reasoning output limit exceeded: {limit} bytes")
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

impl From<ToolBudgetExceeded> for TurnError {
    fn from(error: ToolBudgetExceeded) -> Self {
        Self::ToolBudgetExceeded(error)
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
    limits: ToolCallLimits,
) -> Result<TurnOutput, TurnError> {
    let mut context = history;

    // - 从 `history` 收集 ID，阻止复用上一轮用户请求中的调用 ID。
    // - 后续登记新 ID，阻止当前 Turn 的多个模型步骤复用 ID
    let mut seen_tool_call_ids: BTreeSet<String> = context
        .iter()
        .flat_map(|item| match item {
            ConversationItem::AssistantToolCalls { calls, .. } => calls.as_slice(),
            _ => &[],
        })
        .map(|call| call.call_id.clone())
        .collect();

    //let mut tool_rounds = 0;
    let mut budget = TurnToolBudget::new(limits);
    let reasoning_policy = provider.reasoning_output_policy();
    let mut model_step = 0_u32;

    loop {
        model_step += 1;
        send_progress(
            &progress,
            TurnProgress::ModelStepStarted { step: model_step },
        )
        .await?;
        let mut reasoning_bytes = 0_usize;

        let request = ModelRequest {
            items: context.clone(),
            tools: tools.specs(),
        };
        let mut stream = provider.stream(request);
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;

        while let Some(item) = stream.next().await {
            match item? {
                ModelEvent::TextDelta { delta } => {
                    content.push_str(&delta);
                    send_progress(&progress, TurnProgress::TextDelta { delta }).await?;
                }
                ModelEvent::ReasoningDelta { delta } => {
                    if reasoning_policy != ReasoningOutputPolicy::DisplayOnly {
                        return Err(TurnError::InvalidModelSequence(
                            "reasoning output is not enabled for this provider".into(),
                        ));
                    }
                    if delta.is_empty() {
                        continue;
                    }
                    reasoning_bytes = reasoning_bytes.saturating_add(delta.len());
                    if reasoning_bytes > MAX_REASONING_BYTES_PER_MODEL_STEP {
                        return Err(TurnError::ReasoningLimitExceeded {
                            limit: MAX_REASONING_BYTES_PER_MODEL_STEP,
                        });
                    }
                    send_progress(&progress, TurnProgress::ReasoningDelta { delta }).await?;
                }
                ModelEvent::ToolCall { call } => tool_calls.push(call),
                ModelEvent::Finished { reason } => {
                    finish_reason = Some(reason);
                    break;
                }
            }
        }
        drop(stream);

        let finish_reason = finish_reason.ok_or_else(|| {
            TurnError::InvalidModelSequence(
                "provider stream ended without a terminal event".to_string(),
            )
        })?;

        match finish_reason {
            FinishReason::Stop => {
                if !tool_calls.is_empty() {
                    return Err(TurnError::InvalidModelSequence(
                        "tool call ended with stop finish reason".into(),
                    ));
                }
                if content.is_empty() {
                    return Err(TurnError::InvalidModelSequence(
                        "response does not contain assistant content".into(),
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
                if tool_calls.is_empty() {
                    return Err(TurnError::InvalidModelSequence(
                        "tool_calls finish reason without a tool call".into(),
                    ));
                }

                let mut batch_ids = BTreeSet::new();
                for call in &tool_calls {
                    if call.call_id.trim().is_empty() {
                        return Err(TurnError::InvalidModelSequence(
                            "tool call id must not be empty".into(),
                        ));
                    }
                    if seen_tool_call_ids.contains(&call.call_id)
                        || !batch_ids.insert(call.call_id.clone())
                    {
                        return Err(TurnError::InvalidModelSequence(format!(
                            "duplicate tool call id: {:?}",
                            call.call_id
                        )));
                    }
                }

                budget.preflight(&tool_calls, &tools)?;
                seen_tool_call_ids.extend(batch_ids);

                let prefix = (!content.is_empty()).then_some(content);
                send_progress(
                    &progress,
                    TurnProgress::ToolBatchStarted {
                        content: prefix.clone(),
                        calls: tool_calls.clone(),
                    },
                )
                .await?;
                context.push(ConversationItem::AssistantToolCalls {
                    content: prefix,
                    calls: tool_calls.clone(),
                });

                for call in tool_calls {
                    if tools.contains(&call.name) {
                        budget.consume(&call.name)?;
                    }
                    send_progress(&progress, TurnProgress::ToolStarted { call: call.clone() })
                        .await?;
                    let result = tools.execute(&call).await;
                    send_progress(
                        &progress,
                        TurnProgress::ToolFinished {
                            call,
                            result: result.clone(),
                        },
                    )
                    .await?;
                    context.push(ConversationItem::ToolResult(result));
                }
            }
            FinishReason::Length => {
                return Err(TurnError::InvalidModelSequence(
                    "response was truncated by the output limit".into(),
                ));
            }
            FinishReason::ContentFilter => {
                return Err(TurnError::InvalidModelSequence(
                    "response was stopped by content filtering".into(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelStream, Tool, ToolFuture, ToolSpec};
    use serde_json::json;
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct FakeProvider {
        policy: ReasoningOutputPolicy,
        steps: Mutex<VecDeque<Vec<ModelEvent>>>,
        requests: Mutex<Vec<ModelRequest>>,
    }

    impl FakeProvider {
        fn new(policy: ReasoningOutputPolicy, steps: Vec<Vec<ModelEvent>>) -> Arc<Self> {
            Arc::new(Self {
                policy,
                steps: Mutex::new(steps.into()),
                requests: Mutex::new(Vec::new()),
            })
        }
    }

    impl ModelProvider for FakeProvider {
        fn stream(&self, request: ModelRequest) -> ModelStream {
            self.requests.lock().unwrap().push(request);
            let events = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .expect("发生未预期的模型步骤");
            Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
        }

        fn reasoning_output_policy(&self) -> ReasoningOutputPolicy {
            self.policy
        }
    }

    #[derive(Default)]
    struct FakeTool(AtomicUsize);

    impl Tool for FakeTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "fixture_tool".into(),
                description: "离线测试工具".into(),
                input_schema: json!({"type": "object"}),
            }
        }

        fn execute(&self, _arguments: serde_json::Value) -> ToolFuture<'_> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok("fixture result".into()) })
        }
    }

    fn user() -> ConversationItem {
        ConversationItem::Message(SessionMessage {
            role: MessageRole::User,
            content: "fixture prompt".into(),
        })
    }

    fn reasoning(delta: &str) -> ModelEvent {
        ModelEvent::ReasoningDelta {
            delta: delta.into(),
        }
    }

    fn text(delta: &str) -> ModelEvent {
        ModelEvent::TextDelta {
            delta: delta.into(),
        }
    }

    fn finished(reason: FinishReason) -> ModelEvent {
        ModelEvent::Finished { reason }
    }

    fn call(index: usize) -> ToolCall {
        ToolCall {
            call_id: format!("fixture-call-{index}"),
            name: "fixture_tool".into(),
            arguments: json!({}),
        }
    }

    async fn run(
        provider: Arc<FakeProvider>,
        tools: ToolRegistry,
    ) -> (Result<TurnOutput, TurnError>, Vec<TurnProgress>) {
        run_with_limits(provider, tools, ToolCallLimits::default()).await
    }

    async fn run_with_limits(
        provider: Arc<FakeProvider>,
        tools: ToolRegistry,
        limits: ToolCallLimits,
    ) -> (Result<TurnOutput, TurnError>, Vec<TurnProgress>) {
        let (sender, mut receiver) = mpsc::channel(32);
        let collector = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(event) = receiver.recv().await {
                events.push(event);
            }
            events
        });
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            run_turn(provider, tools, vec![user()], sender, limits),
        )
        .await
        .expect("run_turn 等待超时");
        let progress = tokio::time::timeout(Duration::from_secs(5), collector)
            .await
            .expect("事件收集超时")
            .expect("事件收集任务失败");
        (result, progress)
    }

    #[tokio::test]
    async fn display_only_forwards_reasoning_separately() {
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![vec![
                reasoning("R"),
                text("A"),
                finished(FinishReason::Stop),
            ]],
        );
        let (result, progress) = run(provider, ToolRegistry::empty()).await;
        assert_eq!(
            result.unwrap().assistant,
            SessionMessage {
                role: MessageRole::Assistant,
                content: "A".into()
            }
        );
        assert!(matches!(progress.as_slice(), [
            TurnProgress::ModelStepStarted { step: 1 },
            TurnProgress::ReasoningDelta { delta: r }, TurnProgress::TextDelta { delta: a },
        ] if r == "R" && a == "A"));
    }

    #[tokio::test]
    async fn rejected_policy_fails_before_forwarding_reasoning() {
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::Reject,
            vec![vec![
                reasoning("R"),
                text("A"),
                finished(FinishReason::Stop),
            ]],
        );
        let (result, progress) = run(provider, ToolRegistry::empty()).await;
        assert!(matches!(result, Err(TurnError::InvalidModelSequence(_))));
        assert!(matches!(
            progress.as_slice(),
            [TurnProgress::ModelStepStarted { step: 1 }]
        ));
    }

    #[tokio::test]
    async fn reasoning_only_stop_is_not_a_success() {
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![vec![reasoning("R"), finished(FinishReason::Stop)]],
        );
        let (result, progress) = run(provider, ToolRegistry::empty()).await;
        assert!(matches!(result, Err(TurnError::InvalidModelSequence(_))));
        assert!(
            matches!(progress.as_slice(), [TurnProgress::ModelStepStarted { step: 1 }, TurnProgress::ReasoningDelta { delta }] if delta == "R")
        );
    }

    #[tokio::test]
    async fn tool_steps_keep_order_and_do_not_replay_reasoning() {
        let tool = Arc::new(FakeTool::default());
        let tools = ToolRegistry::new(vec![tool.clone()]).unwrap();
        let specs = tools.specs();
        let call = call(1);
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![
                vec![
                    reasoning("R1"),
                    text("P"),
                    ModelEvent::ToolCall { call: call.clone() },
                    finished(FinishReason::ToolCalls),
                ],
                vec![reasoning("R2"), text("A"), finished(FinishReason::Stop)],
            ],
        );
        let (result, progress) = run(provider.clone(), tools).await;
        assert_eq!(result.unwrap().assistant.content, "A");
        assert_eq!(tool.0.load(Ordering::SeqCst), 1);
        let expected_result = ToolResult {
            call_id: call.call_id.clone(),
            output: "fixture result".into(),
            is_error: false,
        };
        assert_eq!(
            *provider.requests.lock().unwrap(),
            vec![
                ModelRequest {
                    items: vec![user()],
                    tools: specs.clone()
                },
                ModelRequest {
                    items: vec![
                        user(),
                        ConversationItem::AssistantToolCalls {
                            content: Some("P".into()),
                            calls: vec![call.clone()],
                        },
                        ConversationItem::ToolResult(expected_result.clone())
                    ],
                    tools: specs
                },
            ]
        );
        assert!(matches!(progress.as_slice(), [
    TurnProgress::ModelStepStarted { step: 1 },
    TurnProgress::ReasoningDelta { delta: r1 },
    TurnProgress::TextDelta { delta: prefix },
    TurnProgress::ToolBatchStarted { content: Some(saved), calls },
    TurnProgress::ToolStarted { call: started },
    TurnProgress::ToolFinished { call: completed, result },
    TurnProgress::ModelStepStarted { step: 2 },
    TurnProgress::ReasoningDelta { delta: r2 },
    TurnProgress::TextDelta { delta: answer },
] if r1 == "R1" && prefix == "P" && saved == "P" && calls == &vec![call.clone()]
    && started == &call && completed == &call && result == &expected_result
    && r2 == "R2" && answer == "A"));
    }

    #[tokio::test]
    async fn reasoning_limit_counts_utf8_bytes_and_rejects_only_excess_delta() {
        let boundary = format!(
            "{}x",
            "思".repeat((MAX_REASONING_BYTES_PER_MODEL_STEP - 1) / 3)
        );
        assert_eq!(boundary.len(), MAX_REASONING_BYTES_PER_MODEL_STEP);
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![vec![
                reasoning(&boundary),
                text("A"),
                finished(FinishReason::Stop),
            ]],
        );
        assert!(run(provider, ToolRegistry::empty()).await.0.is_ok());

        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![vec![
                reasoning(&boundary),
                reasoning("x"),
                text("A"),
                finished(FinishReason::Stop),
            ]],
        );
        let (result, progress) = run(provider, ToolRegistry::empty()).await;
        assert!(matches!(
            result,
            Err(TurnError::ReasoningLimitExceeded {
                limit: MAX_REASONING_BYTES_PER_MODEL_STEP
            })
        ));
        assert!(
            matches!(progress.as_slice(), [TurnProgress::ModelStepStarted { step: 1 }, TurnProgress::ReasoningDelta { delta }] if delta == &boundary)
        );
    }

    #[tokio::test]
    async fn reasoning_limit_resets_for_each_model_step() {
        let boundary = "x".repeat(MAX_REASONING_BYTES_PER_MODEL_STEP);
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![
                vec![
                    reasoning(&boundary),
                    ModelEvent::ToolCall { call: call(1) },
                    finished(FinishReason::ToolCalls),
                ],
                vec![
                    reasoning(&boundary),
                    text("A"),
                    finished(FinishReason::Stop),
                ],
            ],
        );
        let tools = ToolRegistry::new(vec![Arc::new(FakeTool::default())]).unwrap();
        let (result, progress) = run(provider.clone(), tools).await;
        assert!(result.is_ok());
        assert_eq!(
            provider.requests.lock().unwrap()[1].items,
            vec![
                user(),
                ConversationItem::AssistantToolCalls {
                    content: None,
                    calls: vec![call(1)]
                },
                ConversationItem::ToolResult(ToolResult {
                    call_id: "fixture-call-1".into(),
                    output: "fixture result".into(),
                    is_error: false
                }),
            ]
        );
        assert!(
            progress
                .iter()
                .any(|event| matches!(event, TurnProgress::ToolBatchStarted { content: None, .. }))
        );
        assert_eq!(
            progress
                .iter()
                .filter(|event| matches!(event, TurnProgress::ReasoningDelta { .. }))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn configured_tool_limit_allows_a_final_model_answer() {
        for request_fifth_tool in [false, true] {
            let mut steps: Vec<_> = (1..=4)
                .map(|index| {
                    vec![
                        reasoning("R"),
                        ModelEvent::ToolCall { call: call(index) },
                        finished(FinishReason::ToolCalls),
                    ]
                })
                .collect();
            steps.push(if request_fifth_tool {
                vec![
                    reasoning("R5"),
                    ModelEvent::ToolCall { call: call(5) },
                    finished(FinishReason::ToolCalls),
                ]
            } else {
                vec![reasoning("R5"), text("A"), finished(FinishReason::Stop)]
            });
            let provider = FakeProvider::new(ReasoningOutputPolicy::DisplayOnly, steps);
            let tool = Arc::new(FakeTool::default());
            let (result, progress) = run_with_limits(
                provider.clone(),
                ToolRegistry::new(vec![tool.clone()]).unwrap(),
                ToolCallLimits::new(std::collections::BTreeMap::from([(
                    "fixture_tool".into(),
                    4,
                )]))
                .unwrap(),
            )
            .await;
            if request_fifth_tool {
                assert!(matches!(
                    result,
                    Err(TurnError::ToolBudgetExceeded(ToolBudgetExceeded {
                        limit: 4,
                        used: 4,
                        requested: 1,
                        ..
                    }))
                ));
            } else {
                assert_eq!(result.unwrap().assistant.content, "A");
            }
            assert_eq!(tool.0.load(Ordering::SeqCst), 4);
            assert_eq!(provider.requests.lock().unwrap().len(), 5);
            assert_eq!(
                progress
                    .iter()
                    .filter_map(|event| match event {
                        TurnProgress::ModelStepStarted { step } => Some(*step),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                vec![1, 2, 3, 4, 5]
            );
        }
    }

    struct RecordingTool {
        events: Mutex<Vec<String>>,
    }

    impl Tool for RecordingTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "fixture_tool".into(),
                description: "fixture".into(),
                input_schema: json!({"type":"object"}),
            }
        }
        fn execute(&self, arguments: serde_json::Value) -> ToolFuture<'_> {
            Box::pin(async move {
                let n = arguments["n"].as_u64().unwrap();
                self.events.lock().unwrap().push(format!("start {n}"));
                tokio::task::yield_now().await;
                self.events.lock().unwrap().push(format!("end {n}"));
                Ok(format!("result {n}"))
            })
        }
    }

    #[tokio::test]
    async fn a_batch_executes_serially_and_replays_one_declaration() {
        let mut a = call(1);
        a.arguments = json!({"n":1});
        let mut b = call(2);
        b.arguments = json!({"n":2});
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::DisplayOnly,
            vec![
                vec![
                    reasoning("R"),
                    text("P"),
                    ModelEvent::ToolCall { call: a.clone() },
                    ModelEvent::ToolCall { call: b.clone() },
                    finished(FinishReason::ToolCalls),
                ],
                vec![text("A"), finished(FinishReason::Stop)],
            ],
        );
        let tool = Arc::new(RecordingTool {
            events: Mutex::new(Vec::new()),
        });
        let (result, _) = run(
            provider.clone(),
            ToolRegistry::new(vec![tool.clone()]).unwrap(),
        )
        .await;
        assert_eq!(result.unwrap().assistant.content, "A");
        assert_eq!(
            *tool.events.lock().unwrap(),
            ["start 1", "end 1", "start 2", "end 2"]
        );
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].items.len(), 4);
        assert_eq!(
            requests[1].items[1],
            ConversationItem::AssistantToolCalls {
                content: Some("P".into()),
                calls: vec![a, b],
            }
        );
        assert!(
            matches!(&requests[1].items[2], ConversationItem::ToolResult(r)
        if r.call_id == "fixture-call-1" && r.output == "result 1")
        );
        assert!(
            matches!(&requests[1].items[3], ConversationItem::ToolResult(r)
        if r.call_id == "fixture-call-2" && r.output == "result 2")
        );
    }

    #[tokio::test]
    async fn an_over_budget_batch_executes_nothing() {
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::Reject,
            vec![vec![
                ModelEvent::ToolCall { call: call(1) },
                ModelEvent::ToolCall { call: call(2) },
                finished(FinishReason::ToolCalls),
            ]],
        );
        let tool = Arc::new(FakeTool::default());
        let limits = ToolCallLimits::new(std::collections::BTreeMap::from([(
            "fixture_tool".into(),
            1,
        )]))
        .unwrap();
        let (result, progress) = run_with_limits(
            provider,
            ToolRegistry::new(vec![tool.clone()]).unwrap(),
            limits,
        )
        .await;
        assert!(matches!(
            result,
            Err(TurnError::ToolBudgetExceeded(ToolBudgetExceeded {
                used: 0,
                requested: 2,
                ..
            }))
        ));
        assert_eq!(tool.0.load(Ordering::SeqCst), 0);
        assert!(
            !progress
                .iter()
                .any(|e| matches!(e, TurnProgress::ToolBatchStarted { .. }))
        );
    }

    #[tokio::test]
    async fn default_tool_limit_accumulates_across_model_steps() {
        let steps = (1..=51)
            .map(|i| {
                vec![
                    ModelEvent::ToolCall { call: call(i) },
                    finished(FinishReason::ToolCalls),
                ]
            })
            .collect();
        let provider = FakeProvider::new(ReasoningOutputPolicy::Reject, steps);
        let tool = Arc::new(FakeTool::default());
        let (result, _) = run(
            provider.clone(),
            ToolRegistry::new(vec![tool.clone()]).unwrap(),
        )
        .await;
        assert!(matches!(
            result,
            Err(TurnError::ToolBudgetExceeded(ToolBudgetExceeded {
                limit: 50,
                used: 50,
                requested: 1,
                ..
            }))
        ));
        assert_eq!(tool.0.load(Ordering::SeqCst), 50);
        assert_eq!(provider.requests.lock().unwrap().len(), 51);
    }

    #[tokio::test]
    async fn duplicate_ids_in_a_batch_or_later_step_do_not_execute_again() {
        for same_batch in [true, false] {
            let a = ModelEvent::ToolCall { call: call(1) };
            let steps = if same_batch {
                vec![vec![a.clone(), a, finished(FinishReason::ToolCalls)]]
            } else {
                vec![
                    vec![a.clone(), finished(FinishReason::ToolCalls)],
                    vec![a, finished(FinishReason::ToolCalls)],
                ]
            };
            let provider = FakeProvider::new(ReasoningOutputPolicy::Reject, steps);
            let tool = Arc::new(FakeTool::default());
            let (result, _) = run(provider, ToolRegistry::new(vec![tool.clone()]).unwrap()).await;
            assert!(matches!(result, Err(TurnError::InvalidModelSequence(_))));
            assert_eq!(
                tool.0.load(Ordering::SeqCst),
                if same_batch { 0 } else { 1 }
            );
        }
    }

    struct FailingTool(AtomicUsize);
    impl Tool for FailingTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "fixture_tool".into(),
                description: "fixture".into(),
                input_schema: json!({}),
            }
        }
        fn execute(&self, _: serde_json::Value) -> ToolFuture<'_> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(crate::ToolError::Execution("fixture failure".into())) })
        }
    }

    #[tokio::test]
    async fn failed_tool_attempts_are_not_refunded() {
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::Reject,
            (1..=3)
                .map(|i| {
                    vec![
                        ModelEvent::ToolCall { call: call(i) },
                        finished(FinishReason::ToolCalls),
                    ]
                })
                .collect(),
        );
        let tool = Arc::new(FailingTool(AtomicUsize::new(0)));
        let limits = ToolCallLimits::new(std::collections::BTreeMap::from([(
            "fixture_tool".into(),
            2,
        )]))
        .unwrap();
        let (result, _) = run_with_limits(
            provider,
            ToolRegistry::new(vec![tool.clone()]).unwrap(),
            limits,
        )
        .await;
        assert!(matches!(
            result,
            Err(TurnError::ToolBudgetExceeded(ToolBudgetExceeded {
                limit: 2,
                used: 2,
                ..
            }))
        ));
        assert_eq!(tool.0.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn historical_call_ids_are_still_rejected_without_charging_a_new_turn() {
        let old = call(1);
        let provider = FakeProvider::new(
            ReasoningOutputPolicy::Reject,
            vec![vec![
                ModelEvent::ToolCall { call: old.clone() },
                finished(FinishReason::ToolCalls),
            ]],
        );
        let tool = Arc::new(FakeTool::default());
        let history = vec![
            ConversationItem::AssistantToolCalls {
                content: None,
                calls: vec![old.clone()],
            },
            ConversationItem::ToolResult(ToolResult {
                call_id: old.call_id,
                output: "old result".into(),
                is_error: false,
            }),
            user(),
        ];
        let (sender, mut receiver) = mpsc::channel(16);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            run_turn(
                provider,
                ToolRegistry::new(vec![tool.clone()]).unwrap(),
                history,
                sender,
                ToolCallLimits::default(),
            ),
        )
        .await
        .expect("历史 ID 校验超时");
        assert!(matches!(result, Err(TurnError::InvalidModelSequence(_))));
        assert_eq!(tool.0.load(Ordering::SeqCst), 0);
        let progress = tokio::time::timeout(Duration::from_secs(5), async move {
            let mut progress = Vec::new();
            while let Some(event) = receiver.recv().await {
                progress.push(event);
            }
            progress
        })
        .await
        .expect("事件收集超时");
        assert!(matches!(
            progress.as_slice(),
            [TurnProgress::ModelStepStarted { step: 1 }]
        ));
    }
}
