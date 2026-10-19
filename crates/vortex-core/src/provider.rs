use std::{error::Error, fmt, pin::Pin};

use futures_util::Stream;
use serde::Deserialize;

use crate::{ConversationItem, ToolCall, ToolSpec};

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThinkingMode {
    /// 不发送思考控制参数，遵循服务端默认值。
    #[default]
    ProviderDefault,
    /// 要求适配器显式关闭；无法表达时应报错。
    Disabled,
    //Enabled,
}

/// 模型级、跨供应商的思考选项。
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ThinkingOptions {
    pub mode: ThinkingMode,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ThinkingCapabilities {
    pub can_disable: bool,
    pub supports_history_control: bool,
}

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
