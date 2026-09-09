pub struct SessionMessage {
    pub role: MessageRole,
    pub content: String,
}

pub enum MessageRole {
    User,
    Assistant,
    System,
}

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

pub fn start_session() -> SessionConnection {
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel::<AgentCommand>(32);
    let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<CoreEvent>(128);

    tokio::spawn(async move {
        while let Some(command) = command_receiver.recv().await {
            match command {
                AgentCommand::SubmitPrompt { content } => {
                    if event_sender
                        .send(CoreEvent::UserMessageAdded { content })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                AgentCommand::Shutdown => break,
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
