//use std::{error::Error, fmt, future::Future, pin::Pin};
use crate::SessionMessage;
use futures_util::Stream;
use std::{error::Error, fmt, pin::Pin};

pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}

pub enum ModelEvent {
    TextDelta { delta: String },
    ReasoningDelta { delta: String },
    Finished { reason: FinishReason },
}

pub type ModelStream =
    Pin<Box<dyn Stream<Item = Result<ModelEvent, ProviderError>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub messages: Vec<SessionMessage>,
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

/*
    pub trait ModelProvider: Send + Sync {
    fn complete(&self, request: ModelRequest) -> ProviderFuture<'_>;
}
*/

pub trait ModelProvider: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStream;
}
