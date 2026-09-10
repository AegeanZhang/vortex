use std::{error::Error, fmt, future::Future, pin::Pin};

use crate::SessionMessage;

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

pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, ProviderError>> + Send + 'a>>;

pub trait ModelProvider: Send + Sync {
    fn complete(&self, request: ModelRequest) -> ProviderFuture<'_>;
}
