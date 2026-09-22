use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::info;

use crate::{
    FinishReason, ModelEvent, ModelProvider, ModelRequest, ProviderError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCommand {
    SubmitPrompt { content: String },
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
    receiver: tokio::sync::mpsc::Receiver<Result<ModelEvent, ProviderError>>,
    content: String,
    reasoning: String,
    outcome: Option<Result<FinishReason, ProviderError>>,
}

enum RuntimeInput {
    Shutdown,
    Command(Option<AgentCommand>),
    Model(Option<Result<ModelEvent, ProviderError>>),
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

pub fn start_session(provider: Arc<dyn ModelProvider>) -> SessionConnection {
    info!("Session runtime starting");

    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel::<AgentCommand>(32);
    let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<CoreEvent>(128);

    let (shutdown_sender, mut shutdown_receiver) = watch::channel(false);

    tokio::spawn(async move {
        let mut messages = Vec::<SessionMessage>::new();
        let mut status = SessionStatus::Idle;

        // 任务只负责向通道转发事件，不再返回完整 String.
        let mut turns = tokio::task::JoinSet::<()>::new();

        // 当前轮次的接收端、累积正文和待确认结果。
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
                    RuntimeInput::Model(item)
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

                    messages.push(SessionMessage {
                        role: MessageRole::User,
                        content: content.clone(),
                    });

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

                    let request = ModelRequest {
                        messages: messages.clone(),
                    };

                    // 每轮都创建建独立通道
                    let (turn_sender, turn_receiver) =
                        tokio::sync::mpsc::channel::<Result<ModelEvent, ProviderError>>(32);

                    active = Some(ActiveTurn {
                        receiver: turn_receiver,
                        content: String::new(),
                        reasoning: String::new(),
                        outcome: None,
                    });

                    // 必须 clone, 否则会把actor 持有的provider 移走。
                    let provider = Arc::clone(&provider);

                    turns.spawn(async move {
                        let mut stream = provider.stream(request);

                        while let Some(item) = stream.next().await {
                            let terminal =
                                matches!(&item, Ok(ModelEvent::Finished { .. }) | Err(_));

                            if turn_sender.send(item).await.is_err() {
                                return;
                            }

                            if terminal {
                                return;
                            }
                        }

                        // 退出时释放唯一 sender
                        // 如果流没有提供终态， actor 会检测出来。
                    });
                }

                RuntimeInput::Shutdown
                | RuntimeInput::Command(Some(AgentCommand::Shutdown))
                | RuntimeInput::Command(None)
                | RuntimeInput::ConsumerClosed => {
                    break;
                }

                RuntimeInput::Model(Some(item)) => {
                    let turn = active
                        .as_mut()
                        .expect("model event requires an active turn");

                    // 已经决定终态时, 不再接收后续正文。
                    // 阶段 A 拒绝 Thinking 后，通道可能还有排队的事件。
                    if turn.outcome.is_some() {
                        continue;
                    }

                    match item {
                        Ok(ModelEvent::TextDelta { delta }) => {
                            // Core 累积完整回答，供成功后写入历史。
                            turn.content.push_str(&delta);

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

                        Ok(ModelEvent::ReasoningDelta { .. }) => {
                            // 阶段 A 尚未接入 Thinking 展示。
                            turn.outcome = Some(Err(ProviderError::InvalidResponse(
                                "reasoning output is not enabled in stage A".to_string(),
                            )));

                            // 停止接收新事件，并取消当前模型任务。
                            turn.receiver.close();
                            turns.abort_all();
                        }

                        Ok(ModelEvent::Finished { reason }) => {
                            // 这里只记录结果，不立即设为 Idle。
                            turn.outcome = Some(Ok(reason));
                        }

                        Err(error) => {
                            // 同样等通道关闭后统一收尾。
                            turn.outcome = Some(Err(error));
                        }
                    }
                }

                RuntimeInput::Model(None) => {
                    // 通道已关闭且排队事件已消费完。
                    // take 会把 active 便会 None, 并交出本轮状态。
                    let turn = active
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

                        result = turns.join_next() => {
                            result
                        }
                    };

                    let result: Result<String, ProviderError> = match (joined, turn.outcome) {
                        (_, Some(Err(error))) => Err(error),

                        (Some(Ok(())), Some(Ok(FinishReason::Stop))) => {
                            if turn.content.is_empty() {
                                Err(ProviderError::InvalidResponse(
                                    "response does not contain assistant content".to_string(),
                                ))
                            } else {
                                Ok(turn.content)
                            }
                        }

                        (Some(Ok(())), Some(Ok(FinishReason::Length))) => {
                            Err(ProviderError::InvalidResponse(
                                "response was truncated by the output limit".to_string(),
                            ))
                        }

                        (Some(Ok(())), Some(Ok(FinishReason::ContentFilter))) => {
                            Err(ProviderError::InvalidResponse(
                                "response was stopped by content filtering".to_string(),
                            ))
                        }

                        (Some(Ok(())), None) => Err(ProviderError::InvalidResponse(
                            "provider stream ended without a terminal event".to_string(),
                        )),

                        (Some(Err(_)), _) => Err(ProviderError::Transport(
                            "provider task terminated unexpectedly".to_string(),
                        )),

                        (None, _) => Err(ProviderError::InvalidResponse(
                            "provider task is missing".to_string(),
                        )),
                    };

                    let event = match result {
                        Ok(content) => {
                            // 成功回答只写入历史一次
                            messages.push(SessionMessage {
                                role: MessageRole::Assistant,
                                content,
                            });

                            status = SessionStatus::Idle;
                            CoreEvent::TurnCompleted
                        }

                        Err(error) => {
                            // 失败的 Assistant 不进入请求历史。
                            // 已经发给TUI的片段由 TUI 保留。
                            status = SessionStatus::Failed;

                            CoreEvent::TurnFailed {
                                message: error.to_string(),
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
        FinishReason, ModelEvent, ModelProvider, ModelRequest, ModelStream, ProviderError,
    };

    use super::{
        AgentCommand, AgentHandle, CoreEvent, CoreEventStream, MessageRole, SessionConnection,
        SessionMessage, publish_event, start_session,
    };

    type ModelItem = Result<ModelEvent, ProviderError>;

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
        } = start_session(provider.clone());

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
            requests[0].messages,
            vec![SessionMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
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
        } = start_session(provider.clone());

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
            requests[1].messages,
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
        } = start_session(provider.clone());

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
            requests[1].messages,
            vec![
                SessionMessage {
                    role: MessageRole::User,
                    content: "第一问".to_string(),
                },
                SessionMessage {
                    role: MessageRole::User,
                    content: "再试一次".to_string(),
                },
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
        } = start_session(provider);

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
        } = start_session(provider);

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
        } = start_session(provider);

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
