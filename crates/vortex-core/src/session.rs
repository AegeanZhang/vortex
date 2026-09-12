use std::sync::Arc;
use tracing::info;

use crate::{ModelProvider, ModelRequest, ProviderError};

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
}

impl AgentHandle {
    pub async fn send(&self, command: AgentCommand) -> Result<(), SessionError> {
        self.sender
            .send(command)
            .await
            .map_err(|_| SessionError::CommandChannelClosed)
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

pub fn start_session(provider: Arc<dyn ModelProvider>) -> SessionConnection {
    info!("Session runtime starting");

    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel::<AgentCommand>(32);
    let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<CoreEvent>(128);

    tokio::spawn(async move {
        let mut messages = Vec::<SessionMessage>::new();
        let mut status = SessionStatus::Idle;
        let mut turns = tokio::task::JoinSet::<Result<String, ProviderError>>::new();

        loop {
            tokio::select! {
                command = command_receiver.recv() => {
                    match command {
                        Some(AgentCommand::SubmitPrompt { content }) => {
                            if content.trim().is_empty() {
                                if event_sender
                                    .send(CoreEvent::CommandRejected {
                                        message: "prompt must not be empty".to_string(),
                                    })
                                    .await
                                    .is_err()
                                {
                                    turns.abort_all();
                                    break;
                                }

                                continue;
                            }

                            if status == SessionStatus::Running {
                                if event_sender
                                    .send(CoreEvent::CommandRejected {
                                        message: "a turn is already running".to_string(),
                                    })
                                    .await
                                    .is_err()
                                {
                                    turns.abort_all();
                                    break;
                                }

                                continue;
                            }

                            messages.push(SessionMessage {
                                role: MessageRole::User,
                                content: content.clone(),
                            });

                            if event_sender
                                .send(CoreEvent::UserMessageAdded { content })
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }

                            status = SessionStatus::Running;

                            if event_sender
                                .send(CoreEvent::AssistantMessageStarted)
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }

                            let request = ModelRequest {
                                messages: messages.clone(),
                            };

                            let provider = Arc::clone(&provider);

                            turns.spawn(async move {
                                provider.complete(request).await
                            });
                        }
                        Some(AgentCommand::Shutdown) | None => {
                            turns.abort_all();
                            break;
                        }
                    }
                }

                result = turns.join_next(),
                    if status == SessionStatus::Running =>
                {
                    match result {
                        Some(Ok(Ok(content))) => {
                            status = SessionStatus::Idle;

                            messages.push(SessionMessage {
                                role: MessageRole::Assistant,
                                content: content.clone(),
                            });

                            if event_sender
                                .send(CoreEvent::AssistantTextDelta {
                                    delta: content,
                                })
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }

                            if event_sender
                                .send(CoreEvent::TurnCompleted)
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }
                        }

                        Some(Ok(Err(error))) => {
                            status = SessionStatus::Failed;

                            if event_sender
                                .send(CoreEvent::TurnFailed {
                                    message: error.to_string(),
                                })
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }
                        }

                        Some(Err(_join_error)) => {
                            status = SessionStatus::Failed;

                            if event_sender
                                .send(CoreEvent::TurnFailed {
                                    message: "provider task terminated unexpectedly".to_string(),
                                })
                                .await
                                .is_err()
                            {
                                turns.abort_all();
                                break;
                            }
                        }

                        None => {
                            status = SessionStatus::Failed;

                            if event_sender
                                .send(CoreEvent::TurnFailed {
                                    message: "provider task ended without a result".to_string()
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    SessionConnection {
        snapshot: SessionSnapshot {
            messages: Vec::new(),
            status: SessionStatus::Idle,
        },
        agent: AgentHandle {
            sender: command_sender,
        },
        events: CoreEventStream {
            receiver: event_receiver,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{ModelProvider, ModelRequest, ProviderError, ProviderFuture};

    use super::{AgentCommand, CoreEvent, SessionConnection, start_session};

    struct FakeProvider {
        result: Result<String, ProviderError>,
    }

    impl ModelProvider for FakeProvider {
        fn complete(&self, _request: ModelRequest) -> ProviderFuture<'_> {
            let result = self.result.clone();

            Box::pin(async move { result })
        }
    }

    #[tokio::test]
    async fn submit_prompt_emits_completed_turn_events() {
        let provider = Arc::new(FakeProvider {
            result: Ok("Hello from fake provider".to_string()),
        });

        let SessionConnection {
            agent, mut events, ..
        } = start_session(provider);

        agent
            .send(AgentCommand::SubmitPrompt {
                content: "Hello".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(
            events.recv().await,
            Some(CoreEvent::UserMessageAdded {
                content: "Hello".to_string(),
            })
        );

        assert_eq!(
            events.recv().await,
            Some(CoreEvent::AssistantMessageStarted)
        );

        assert_eq!(
            events.recv().await,
            Some(CoreEvent::AssistantTextDelta {
                delta: "Hello from fake provider".to_string(),
            })
        );

        assert_eq!(events.recv().await, Some(CoreEvent::TurnCompleted));

        agent.send(AgentCommand::Shutdown).await.unwrap();
    }
}
