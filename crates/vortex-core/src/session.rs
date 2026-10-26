use tokio::sync::watch;
use tracing::info;

use crate::{
    ConversationItem, MessageRole, ModelInfo, ModelManager, SessionMessage, ToolCall, ToolRegistry,
    ToolResult,
    agent_loop::{TurnError, TurnOutput, TurnProgress, run_turn},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Running,
    Failed,
}

#[derive(Debug)]
pub enum SessionError {
    CommandChannelClosed,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandChannelClosed => {
                write!(formatter, "agent command channel is closed")
            }
        }
    }
}

impl std::error::Error for SessionError {}

pub struct SessionConnection {
    pub snapshot: SessionSnapshot,
    pub agent: AgentHandle,
    pub events: CoreEventStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub messages: Vec<SessionMessage>,
    pub status: SessionStatus,
    pub model: ModelInfo,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCommand {
    SubmitPrompt { content: String },
    SelectModel { model_id: String },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    UserMessageAdded { content: String },
    AssistantMessageStarted,
    AssistantTextDelta { delta: String },
    TurnCompleted,
    TurnFailed { message: String },
    CommandRejected { message: String },
    ToolCallStarted { call: ToolCall },
    ToolCallFinished { name: String, result: ToolResult },
    ModelSelected { model: ModelInfo },
    ModelSelectionRejected { model_id: String, message: String },
    ModelStepStarted { step: u32 },
    AssistantReasoningDelta { delta: String },
}

#[derive(Clone)]
pub struct AgentHandle {
    sender: tokio::sync::mpsc::Sender<AgentCommand>,
    shutdown: watch::Sender<bool>,
}

impl AgentHandle {
    pub async fn send(&self, command: AgentCommand) -> Result<(), SessionError> {
        match command {
            AgentCommand::Shutdown => self
                .shutdown
                .send(true)
                .map_err(|_| SessionError::CommandChannelClosed),

            command => {
                // 已经请求关闭后，不再接受新的业务命令。
                if *self.shutdown.borrow() {
                    return Err(SessionError::CommandChannelClosed);
                }

                self.sender
                    .send(command)
                    .await
                    .map_err(|_| SessionError::CommandChannelClosed)
            }
        }
    }
}

pub struct CoreEventStream {
    receiver: tokio::sync::mpsc::Receiver<CoreEvent>,
}

impl CoreEventStream {
    pub async fn recv(&mut self) -> Option<CoreEvent> {
        self.receiver.recv().await
    }
}

struct ActiveTurn {
    receiver: tokio::sync::mpsc::Receiver<TurnProgress>,
}

enum RuntimeInput {
    Shutdown,
    Command(Option<AgentCommand>),
    Turn(Option<TurnProgress>),
    ConsumerClosed,
}

async fn publish_event(
    sender: &tokio::sync::mpsc::Sender<CoreEvent>,
    shutdown: &mut watch::Receiver<bool>,
    event: CoreEvent,
) -> bool {
    // 如果已经要求关闭，就不再发送这个事件。
    if *shutdown.borrow() {
        return false;
    }

    tokio::select! {
        biased;

        _ = shutdown.changed() => {
            false
        }

        result = sender.send(event) => {
            result.is_ok()
        }
    }
}

pub fn start_session(mut models: ModelManager, tools: ToolRegistry) -> SessionConnection {
    info!("Session runtime starting");

    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel::<AgentCommand>(32);
    let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<CoreEvent>(128);

    let (shutdown_sender, mut shutdown_receiver) = watch::channel(false);

    let initial_model = models.current().clone();
    let available_modes = models.catalog().to_vec();

    tokio::spawn(async move {
        let mut history = Vec::<ConversationItem>::new();
        let mut status = SessionStatus::Idle;
        let mut turns = tokio::task::JoinSet::<Result<TurnOutput, TurnError>>::new();
        let mut active: Option<ActiveTurn> = None;

        loop {
            // 处理进入等待之前就已经发生的关闭请求。
            if *shutdown_receiver.borrow() {
                break;
            }

            let input = tokio::select! {
                _ = shutdown_receiver.changed() => {
                    RuntimeInput::Shutdown
                }

                command = command_receiver.recv() => {
                    RuntimeInput::Command(command)
                }

                item = async {
                    active
                        .as_mut()
                        .expect("guarded active turn")
                        .receiver
                        .recv()
                        .await
                }, if active.is_some() => {
                    //RuntimeInput::Model(item)
                    RuntimeInput::Turn(item)
                }

                _ = event_sender.closed() => {
                    RuntimeInput::ConsumerClosed
                }
            };

            // select 结束，重新借用或取走active
            match input {
                RuntimeInput::Command(Some(AgentCommand::SubmitPrompt { content })) => {
                    if content.trim().is_empty() {
                        if !publish_event(
                            &event_sender,
                            &mut shutdown_receiver,
                            CoreEvent::CommandRejected {
                                message: "prompt must not be empty".to_string(),
                            },
                        )
                        .await
                        {
                            break;
                        }

                        continue;
                    }

                    if status == SessionStatus::Running {
                        if !publish_event(
                            &event_sender,
                            &mut shutdown_receiver,
                            CoreEvent::CommandRejected {
                                message: "a turn is already running".to_string(),
                            },
                        )
                        .await
                        {
                            break;
                        }

                        continue;
                    }

                    history.push(ConversationItem::Message(SessionMessage {
                        role: MessageRole::User,
                        content: content.clone(),
                    }));

                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::UserMessageAdded { content },
                    )
                    .await
                    {
                        break;
                    }

                    status = SessionStatus::Running;

                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::AssistantMessageStarted,
                    )
                    .await
                    {
                        break;
                    }

                    let (turn_sender, turn_receiver) =
                        tokio::sync::mpsc::channel::<TurnProgress>(32);

                    active = Some(ActiveTurn {
                        receiver: turn_receiver,
                    });

                    //let provider = Arc::clone(&provider);
                    let provider = models.provider();
                    let tools = tools.clone();
                    let turn_history = history.clone();

                    turns.spawn(run_turn(provider, tools, turn_history, turn_sender));
                }

                RuntimeInput::Command(Some(AgentCommand::SelectModel { model_id })) => {
                    let event = if status == SessionStatus::Running || active.is_some() {
                        CoreEvent::ModelSelectionRejected {
                            model_id,
                            message: "当前 Turn 正在运行, 完成后再切换模型".into(),
                        }
                    } else {
                        match models.select(&model_id) {
                            Ok(model) => CoreEvent::ModelSelected { model },
                            Err(error) => CoreEvent::ModelSelectionRejected {
                                model_id,
                                message: error.message,
                            },
                        }
                    };
                    if !publish_event(&event_sender, &mut shutdown_receiver, event).await {
                        break;
                    }
                }

                RuntimeInput::Shutdown
                | RuntimeInput::Command(Some(AgentCommand::Shutdown))
                | RuntimeInput::Command(None)
                | RuntimeInput::ConsumerClosed => {
                    break;
                }

                RuntimeInput::Turn(Some(TurnProgress::TextDelta { delta })) => {
                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::AssistantTextDelta { delta },
                    )
                    .await
                    {
                        break;
                    }
                }

                RuntimeInput::Turn(Some(TurnProgress::ToolStarted { call })) => {
                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::ToolCallStarted { call },
                    )
                    .await
                    {
                        break;
                    }
                }

                RuntimeInput::Turn(Some(TurnProgress::ToolFinished {
                    assistant_prefix,
                    call,
                    result,
                })) => {
                    if let Some(content) = assistant_prefix {
                        history.push(ConversationItem::Message(SessionMessage {
                            role: MessageRole::Assistant,
                            content,
                        }));
                    }

                    let name = call.name.clone();
                    history.push(ConversationItem::ToolCall(call));
                    history.push(ConversationItem::ToolResult(result.clone()));

                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::ToolCallFinished { name, result },
                    )
                    .await
                    {
                        break;
                    }
                }

                RuntimeInput::Turn(Some(TurnProgress::ModelStepStarted { step })) => {
                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::ModelStepStarted { step },
                    )
                    .await
                    {
                        break;
                    }
                }

                RuntimeInput::Turn(Some(TurnProgress::ReasoningDelta { delta })) => {
                    if !publish_event(
                        &event_sender,
                        &mut shutdown_receiver,
                        CoreEvent::AssistantReasoningDelta { delta },
                    )
                    .await
                    {
                        break;
                    }
                }

                RuntimeInput::Turn(None) => {
                    // 通道已关闭且排队事件已消费完。
                    // take 会把 active 便会 None, 并交出本轮状态。
                    active
                        .take()
                        .expect("closed model channel requires an active turn");

                    // 按上述任务结构，此时 sender 已释放，
                    // 任务不再等待网络后继续生产事件
                    if *shutdown_receiver.borrow() {
                        break;
                    }

                    let joined = tokio::select! {
                        biased;

                        _ = shutdown_receiver.changed() => {
                            break;
                        }

                        _ = event_sender.closed() => {
                            break;
                        }

                        joined = turns.join_next() => {
                            joined
                        }
                    };

                    let event = match joined {
                        Some(Ok(Ok(output))) => {
                            history.push(ConversationItem::Message(output.assistant));
                            status = SessionStatus::Idle;
                            CoreEvent::TurnCompleted
                        }
                        Some(Ok(Err(error))) => {
                            status = SessionStatus::Failed;
                            CoreEvent::TurnFailed {
                                message: error.to_string(),
                            }
                        }
                        Some(Err(_)) => {
                            status = SessionStatus::Failed;
                            CoreEvent::TurnFailed {
                                message: "turn task terminated unexpectedly".to_string(),
                            }
                        }
                        None => {
                            status = SessionStatus::Failed;
                            CoreEvent::TurnFailed {
                                message: "turn task is missing".to_string(),
                            }
                        }
                    };

                    if !publish_event(&event_sender, &mut shutdown_receiver, event).await {
                        break;
                    }
                }
            }
        }

        // 所有退出路径统一中止仍然活跃的任务。
        turns.abort_all();
    });

    SessionConnection {
        snapshot: SessionSnapshot {
            messages: Vec::new(),
            status: SessionStatus::Idle,
            model: initial_model,
            models: available_modes,
        },
        agent: AgentHandle {
            sender: command_sender,
            shutdown: shutdown_sender,
        },
        events: CoreEventStream {
            receiver: event_receiver,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll},
        time::Duration,
    };

    use futures_util::Stream;
    use tokio::sync::{mpsc, oneshot, watch};

    use crate::{
        ConversationItem, FinishReason, MessageRole, ModelEvent, ModelFactory, ModelInfo,
        ModelManager, ModelProvider, ModelRequest, ModelSelectionError, ModelStream, ProviderError,
        SessionMessage, ToolRegistry,
    };

    use super::{
        AgentCommand, AgentHandle, CoreEvent, CoreEventStream, SessionConnection, publish_event,
        start_session,
    };

    type ModelItem = Result<ModelEvent, ProviderError>;

    struct SingleModelFactory {
        provider: Arc<dyn ModelProvider>,
    }

    impl ModelFactory for SingleModelFactory {
        fn catalog(&self) -> Vec<ModelInfo> {
            vec![ModelInfo {
                id: "test-model".into(),
                display_name: "Test model".into(),
                provider_id: "test".into(),
                endpoint_host: "example.test".into(),
                supports_tools: true,
                unavailable_reason: None,
            }]
        }

        fn create(&self, id: &str) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
            if id != "test-model" {
                return Err(ModelSelectionError {
                    model_id: id.into(),
                    message: "未知测试模型".into(),
                });
            }
            Ok(Arc::clone(&self.provider))
        }
    }

    fn test_models(provider: Arc<dyn ModelProvider>) -> ModelManager {
        ModelManager::new(Arc::new(SingleModelFactory { provider }), "test-model")
            .expect("测试模型装配成功")
    }

    fn message(role: MessageRole, content: &str) -> ConversationItem {
        ConversationItem::Message(SessionMessage {
            role,
            content: content.to_string(),
        })
    }

    async fn within<T>(future: impl Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(5), future)
            .await
            .expect("测试等待超时")
    }

    async fn expect_event(events: &mut CoreEventStream, expected: CoreEvent) {
        assert_eq!(within(events.recv()).await, Some(expected));
    }

    async fn begin_turn(agent: &AgentHandle, events: &mut CoreEventStream, prompt: &str) {
        within(agent.send(AgentCommand::SubmitPrompt {
            content: prompt.to_string(),
        }))
        .await
        .expect("提交失败");

        expect_event(
            events,
            CoreEvent::UserMessageAdded {
                content: prompt.to_string(),
            },
        )
        .await;

        expect_event(events, CoreEvent::AssistantMessageStarted).await;
        expect_event(events, CoreEvent::ModelStepStarted { step: 1 }).await;
    }

    async fn close_session(agent: &AgentHandle, events: &mut CoreEventStream) {
        within(agent.send(AgentCommand::Shutdown))
            .await
            .expect("关闭通知失败");

        // 调用前应已消费完预期事件。
        // 若还有重复终态，这里也能检测出来。
        assert_eq!(within(events.recv()).await, None);
    }

    fn text(delta: &str) -> ModelItem {
        Ok(ModelEvent::TextDelta {
            delta: delta.to_string(),
        })
    }

    fn stop() -> ModelItem {
        Ok(ModelEvent::Finished {
            reason: FinishReason::Stop,
        })
    }

    fn scripted(items: Vec<ModelItem>) -> ModelStream {
        Box::pin(futures_util::stream::iter(items))
    }

    struct ThinkingFakeProvider(FakeProvider);

    impl ThinkingFakeProvider {
        fn new(streams: Vec<ModelStream>) -> Self {
            Self(FakeProvider::new(streams))
        }
    }

    impl ModelProvider for ThinkingFakeProvider {
        fn stream(&self, request: ModelRequest) -> ModelStream {
            self.0.stream(request)
        }

        fn reasoning_output_policy(&self) -> crate::ReasoningOutputPolicy {
            crate::ReasoningOutputPolicy::DisplayOnly
        }
    }

    fn thinking(delta: &str) -> ModelItem {
        Ok(ModelEvent::ReasoningDelta {
            delta: delta.into(),
        })
    }

    async fn expect_thinking(events: &mut CoreEventStream, delta: &str) {
        expect_event(
            events,
            CoreEvent::AssistantReasoningDelta {
                delta: delta.into(),
            },
        )
        .await;
    }

    #[tokio::test]
    async fn thinking_turn_publishes_ordered_events_and_only_one_terminal_event() {
        let provider = Arc::new(ThinkingFakeProvider::new(vec![scripted(vec![
            thinking("R"),
            text("A"),
            stop(),
        ])]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider), ToolRegistry::empty());
        begin_turn(&agent, &mut events, "fixture prompt").await;
        expect_thinking(&mut events, "R").await;
        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta { delta: "A".into() },
        )
        .await;
        expect_event(&mut events, CoreEvent::TurnCompleted).await;
        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn reasoning_arrives_before_the_provider_finishes() {
        let (stream, sender, started, dropped) = controlled_stream();
        let provider = Arc::new(ThinkingFakeProvider::new(vec![stream]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider), ToolRegistry::empty());
        begin_turn(&agent, &mut events, "fixture prompt").await;
        within(started).await.unwrap();
        sender.send(thinking("R")).unwrap();
        expect_thinking(&mut events, "R").await;
        assert!(matches!(
            events.receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        sender.send(text("A")).unwrap();
        sender.send(stop()).unwrap();
        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta { delta: "A".into() },
        )
        .await;
        expect_event(&mut events, CoreEvent::TurnCompleted).await;
        within(dropped).await.unwrap();
        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn shutdown_during_thinking_drops_the_active_stream() {
        let (stream, sender, started, dropped) = controlled_stream();
        let provider = Arc::new(ThinkingFakeProvider::new(vec![stream]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider), ToolRegistry::empty());
        begin_turn(&agent, &mut events, "fixture prompt").await;
        within(started).await.unwrap();
        sender.send(thinking("R")).unwrap();
        expect_thinking(&mut events, "R").await;
        within(agent.send(AgentCommand::Shutdown)).await.unwrap();
        within(dropped).await.unwrap();
        assert_eq!(within(events.recv()).await, None);
        assert!(sender.is_closed());
    }

    #[tokio::test]
    async fn next_turn_contains_the_answer_once_but_never_reasoning() {
        let provider = Arc::new(ThinkingFakeProvider::new(vec![
            scripted(vec![thinking("R1"), text("A1"), stop()]),
            scripted(vec![thinking("R2"), text("A2"), stop()]),
        ]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());
        for (prompt, reasoning, answer) in [("U1", "R1", "A1"), ("U2", "R2", "A2")] {
            begin_turn(&agent, &mut events, prompt).await;
            expect_thinking(&mut events, reasoning).await;
            expect_event(
                &mut events,
                CoreEvent::AssistantTextDelta {
                    delta: answer.into(),
                },
            )
            .await;
            expect_event(&mut events, CoreEvent::TurnCompleted).await;
        }
        let requests = provider.0.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].items, vec![message(MessageRole::User, "U1")]);
        assert_eq!(
            requests[1].items,
            vec![
                message(MessageRole::User, "U1"),
                message(MessageRole::Assistant, "A1"),
                message(MessageRole::User, "U2")
            ]
        );
        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn failed_thinking_keeps_completed_tools_but_not_partial_assistant_history() {
        let call = crate::ToolCall {
            call_id: "fixture-call".into(),
            name: "fixture_missing_tool".into(),
            arguments: serde_json::json!({}),
        };
        let result = crate::ToolResult {
            call_id: call.call_id.clone(),
            output: "unknown tool: fixture_missing_tool".into(),
            is_error: true,
        };
        let provider = Arc::new(ThinkingFakeProvider::new(vec![
            scripted(vec![
                thinking("R1"),
                Ok(ModelEvent::ToolCall { call: call.clone() }),
                Ok(ModelEvent::Finished {
                    reason: FinishReason::ToolCalls,
                }),
            ]),
            scripted(vec![
                thinking("R2"),
                Err(ProviderError::Transport("fixture failure".into())),
            ]),
            scripted(vec![text("A"), stop()]),
        ]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());
        begin_turn(&agent, &mut events, "U1").await;
        expect_thinking(&mut events, "R1").await;
        expect_event(
            &mut events,
            CoreEvent::ToolCallStarted { call: call.clone() },
        )
        .await;
        expect_event(
            &mut events,
            CoreEvent::ToolCallFinished {
                name: call.name.clone(),
                result: result.clone(),
            },
        )
        .await;
        expect_event(&mut events, CoreEvent::ModelStepStarted { step: 2 }).await;
        expect_thinking(&mut events, "R2").await;
        assert!(
            matches!(within(events.recv()).await, Some(CoreEvent::TurnFailed { message }) if message.contains("fixture failure"))
        );

        begin_turn(&agent, &mut events, "U2").await;
        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta { delta: "A".into() },
        )
        .await;
        expect_event(&mut events, CoreEvent::TurnCompleted).await;
        let requests = provider.0.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests[2].items,
            vec![
                message(MessageRole::User, "U1"),
                ConversationItem::ToolCall(call),
                ConversationItem::ToolResult(result),
                message(MessageRole::User, "U2")
            ]
        );
        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn shutdown_cancels_thinking_when_the_ui_queue_is_full() {
        let (stream, sender, started, dropped) = controlled_stream();
        let provider = Arc::new(ThinkingFakeProvider::new(vec![stream]));
        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider), ToolRegistry::empty());
        begin_turn(&agent, &mut events, "fixture prompt").await;
        within(started).await.unwrap();
        let capacity = events.receiver.max_capacity();
        for _ in 0..(capacity + 64) {
            sender.send(thinking("R")).unwrap();
        }
        within(async {
            while events.receiver.len() < capacity {
                tokio::task::yield_now().await;
            }
        })
        .await;
        within(agent.send(AgentCommand::Shutdown)).await.unwrap();
        within(dropped).await.unwrap();
        within(async { while events.recv().await.is_some() {} }).await;
        assert!(sender.is_closed());
    }

    struct FakeProvider {
        streams: Mutex<VecDeque<ModelStream>>,
        requests: Mutex<Vec<ModelRequest>>,
    }

    impl FakeProvider {
        fn new(streams: Vec<ModelStream>) -> Self {
            Self {
                streams: Mutex::new(streams.into_iter().collect()),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<ModelRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl ModelProvider for FakeProvider {
        fn stream(&self, request: ModelRequest) -> ModelStream {
            self.requests.lock().unwrap().push(request);

            self.streams
                .lock()
                .unwrap()
                .pop_front()
                .expect("发生了未预期的 Provider 调用")
        }
    }

    #[tokio::test]
    async fn submit_prompt_emits_streaming_events_in_order() {
        let provider = Arc::new(FakeProvider::new(vec![scripted(vec![
            text("Hello"),
            text(" world"),
            stop(),
        ])]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "你好").await;

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "Hello".to_string(),
            },
        )
        .await;

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: " world".to_string(),
            },
        )
        .await;

        expect_event(&mut events, CoreEvent::TurnCompleted).await;

        let requests = provider.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            /* requests[0].messages,
            vec![SessionMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],*/
            requests[0].items,
            vec![message(MessageRole::User, "你好")],
        );

        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn second_request_contains_completed_first_turn_once() {
        let provider = Arc::new(FakeProvider::new(vec![
            scripted(vec![text("Hello"), text(" world"), stop()]),
            scripted(vec![text("第二轮回答"), stop()]),
        ]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "第一问").await;

        for delta in ["Hello", " world"] {
            expect_event(
                &mut events,
                CoreEvent::AssistantTextDelta {
                    delta: delta.to_string(),
                },
            )
            .await;
        }

        expect_event(&mut events, CoreEvent::TurnCompleted).await;

        begin_turn(&agent, &mut events, "第二问").await;

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "第二轮回答".to_string(),
            },
        )
        .await;

        expect_event(&mut events, CoreEvent::TurnCompleted).await;

        let requests = provider.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            /*requests[1].messages,
            vec![
                SessionMessage {
                    role: MessageRole::User,
                    content: "第一问".to_string(),
                },
                SessionMessage {
                    role: MessageRole::Assistant,
                    content: "Hello world".to_string(),
                },
                SessionMessage {
                    role: MessageRole::User,
                    content: "第二问".to_string(),
                },
            ],*/
            requests[1].items,
            vec![
                message(MessageRole::User, "第一问"),
                message(MessageRole::Assistant, "Hello world"),
                message(MessageRole::User, "第二问"),
            ],
        );

        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn failed_partial_response_is_not_added_to_history() {
        let provider = Arc::new(FakeProvider::new(vec![
            scripted(vec![
                text("未完成的回答"),
                Err(ProviderError::Transport(
                    "test connection failure".to_string(),
                )),
            ]),
            scripted(vec![text("重试成功"), stop()]),
        ]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "第一问").await;

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "未完成的回答".to_string(),
            },
        )
        .await;

        let failure = within(events.recv()).await;
        assert!(
            matches!(failure, Some(CoreEvent::TurnFailed { .. })),
            "预期 TurnFailed，实际为 {failure:?}",
        );

        begin_turn(&agent, &mut events, "再试一次").await;

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "重试成功".to_string(),
            },
        )
        .await;

        expect_event(&mut events, CoreEvent::TurnCompleted).await;

        let requests = provider.requests();
        assert_eq!(
            /*requests[1].messages,
            vec![
                SessionMessage {
                    role: MessageRole::User,
                    content: "第一问".to_string(),
                },
                SessionMessage {
                    role: MessageRole::User,
                    content: "再试一次".to_string(),
                },
            ],*/
            requests[1].items,
            vec![
                message(MessageRole::User, "第一问"),
                message(MessageRole::User, "再试一次"),
            ],
        );

        close_session(&agent, &mut events).await;
    }

    struct ControlledStream {
        receiver: mpsc::UnboundedReceiver<ModelItem>,
        started: Option<oneshot::Sender<()>>,
        dropped: Option<oneshot::Sender<()>>,
    }

    impl Stream for ControlledStream {
        type Item = ModelItem;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();

            if let Some(started) = this.started.take() {
                let _ = started.send(());
            }

            this.receiver.poll_recv(cx)
        }
    }

    impl Drop for ControlledStream {
        fn drop(&mut self) {
            if let Some(dropped) = self.dropped.take() {
                let _ = dropped.send(());
            }
        }
    }

    fn controlled_stream() -> (
        ModelStream,
        mpsc::UnboundedSender<ModelItem>,
        oneshot::Receiver<()>,
        oneshot::Receiver<()>,
    ) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let (started_sender, started_receiver) = oneshot::channel();
        let (dropped_sender, dropped_receiver) = oneshot::channel();

        let stream = ControlledStream {
            receiver,
            started: Some(started_sender),
            dropped: Some(dropped_sender),
        };

        (Box::pin(stream), sender, started_receiver, dropped_receiver)
    }

    #[tokio::test]
    async fn first_delta_arrives_before_provider_finishes() {
        let (stream, sender, started, dropped) = controlled_stream();

        let provider = Arc::new(FakeProvider::new(vec![stream]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "你好").await;
        within(started).await.expect("模型流没有启动");

        assert!(sender.send(text("第一段")).is_ok());

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "第一段".to_string(),
            },
        )
        .await;

        // 此时还没有向模型流发送 Finished。
        assert!(matches!(
            events.receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty),
        ));

        assert!(sender.send(text("第二段")).is_ok());
        assert!(sender.send(stop()).is_ok());

        expect_event(
            &mut events,
            CoreEvent::AssistantTextDelta {
                delta: "第二段".to_string(),
            },
        )
        .await;

        expect_event(&mut events, CoreEvent::TurnCompleted).await;
        within(dropped).await.expect("模型流没有释放");

        close_session(&agent, &mut events).await;
    }

    #[tokio::test]
    async fn shutdown_drops_an_active_provider_stream() {
        let (stream, _sender, started, dropped) = controlled_stream();

        let provider = Arc::new(FakeProvider::new(vec![stream]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "你好").await;
        within(started).await.expect("模型流没有启动");

        // 保留 sender，但不发送任何内容，模拟持续等待。
        within(agent.send(AgentCommand::Shutdown))
            .await
            .expect("关闭通知失败");

        within(dropped).await.expect("Shutdown 后模型流没有释放");

        assert_eq!(within(events.recv()).await, None);
    }

    #[tokio::test]
    async fn shutdown_cancels_provider_when_ui_queue_is_full() {
        let (stream, sender, started, dropped) = controlled_stream();

        let provider = Arc::new(FakeProvider::new(vec![stream]));

        let SessionConnection {
            agent, mut events, ..
        } = start_session(test_models(provider.clone()), ToolRegistry::empty());

        begin_turn(&agent, &mut events, "你好").await;
        within(started).await.expect("模型流没有启动");

        let capacity = events.receiver.max_capacity();

        // 足够填满 UI 通道及内部模型通道。
        for _ in 0..(capacity + 64) {
            assert!(sender.send(text("x")).is_ok());
        }

        // 不再 recv，等待真实队列达到满状态。
        within(async {
            while events.receiver.len() < capacity {
                tokio::task::yield_now().await;
            }
        })
        .await;

        within(agent.send(AgentCommand::Shutdown))
            .await
            .expect("关闭通知失败");

        // 确认真正取消，而不仅是 send(Shutdown) 返回成功。
        within(dropped).await.expect("队列满时模型流没有释放");

        // 已经入队的事件可以仍然存在；排空后通道应关闭。
        within(async { while events.recv().await.is_some() {} }).await;
    }

    #[tokio::test]
    async fn shutdown_interrupts_a_blocked_publish() {
        let (sender, mut receiver) = mpsc::channel(1);
        let (shutdown_sender, mut shutdown_receiver) = watch::channel(false);

        sender
            .send(CoreEvent::AssistantMessageStarted)
            .await
            .unwrap();

        // 队列容量为 1，已经满了。
        let mut publishing = Box::pin(publish_event(
            &sender,
            &mut shutdown_receiver,
            CoreEvent::TurnCompleted,
        ));

        std::future::poll_fn(|cx| {
            assert!(publishing.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;

        shutdown_sender.send(true).unwrap();

        assert!(!within(publishing).await);

        assert_eq!(
            receiver.try_recv().unwrap(),
            CoreEvent::AssistantMessageStarted,
        );

        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty),
        ));
    }
}
