use std::{error::Error, fmt, pin::Pin};

use futures_util::Stream;

use crate::{ConversationItem, ToolCall, ToolSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelEvent {
    TextDelta { delta: String },
    ReasoningDelta { delta: String },
    ToolCall { call: ToolCall },
    Finished { reason: FinishReason },
}

pub type ModelStream =
    Pin<Box<dyn Stream<Item = Result<ModelEvent, ProviderError>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    //pub messages: Vec<SessionMessage>,
    pub items: Vec<ConversationItem>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Transport(String),
    HttpStatus { status: u16 },
    InvalidResponse(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(message) => {
                write!(formatter, "provider transport error: {message}")
            }
            Self::HttpStatus { status } => {
                write!(formatter, "provider returned HTTP {status}")
            }
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid provider response: {message}")
            }
        }
    }
}

impl Error for ProviderError {}

pub trait ModelProvider: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStream;
}
