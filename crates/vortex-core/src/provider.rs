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
    /// 显式开启公开思考输出；本版仅支持已适配的展示-only 接入。
    Enabled,
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
    pub can_enable: bool,
    pub supports_history_control: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ThinkingCapabilitiesOverride {
    pub can_disable: Option<bool>,
    pub can_enable: Option<bool>,
    pub supports_history_control: Option<bool>,
}

impl ThinkingCapabilitiesOverride {
    pub fn apply_to(self, mut base: ThinkingCapabilities) -> ThinkingCapabilities {
        if let Some(value) = self.can_enable {
            base.can_enable = value;
        }
        if let Some(value) = self.can_disable {
            base.can_disable = value;
        }
        if let Some(value) = self.supports_history_control {
            base.supports_history_control = value;
        }
        base
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReasoningOutputPolicy {
    #[default]
    Reject,
    DisplayOnly,
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
    fn reasoning_output_policy(&self) -> ReasoningOutputPolicy {
        ReasoningOutputPolicy::Reject
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_overrides_preserve_both_true_and_false_baselines() {
        for value in [false, true] {
            let base = ThinkingCapabilities {
                can_enable: value,
                can_disable: value,
                supports_history_control: value,
            };
            assert_eq!(ThinkingCapabilitiesOverride::default().apply_to(base), base);
        }
    }

    #[test]
    fn explicit_overrides_replace_only_the_selected_capability() {
        for value in [false, true] {
            let base = ThinkingCapabilities {
                can_enable: !value,
                can_disable: !value,
                supports_history_control: !value,
            };
            assert_eq!(
                ThinkingCapabilitiesOverride {
                    can_enable: Some(value),
                    ..Default::default()
                }
                .apply_to(base),
                ThinkingCapabilities {
                    can_enable: value,
                    ..base
                }
            );
            assert_eq!(
                ThinkingCapabilitiesOverride {
                    can_disable: Some(value),
                    ..Default::default()
                }
                .apply_to(base),
                ThinkingCapabilities {
                    can_disable: value,
                    ..base
                }
            );
            assert_eq!(
                ThinkingCapabilitiesOverride {
                    supports_history_control: Some(value),
                    ..Default::default()
                }
                .apply_to(base),
                ThinkingCapabilities {
                    supports_history_control: value,
                    ..base
                }
            );
        }
    }
}
