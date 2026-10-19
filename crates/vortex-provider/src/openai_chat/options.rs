use std::{error::Error, fmt};

use serde::Serialize;

use vortex_core::{ThinkingCapabilities, ThinkingMode, ThinkingOptions};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenAiChatDialect {
    #[default]
    Generic,
    DashScope,
    DeepSeek,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenAiChatOptions {
    pub dialect: OpenAiChatDialect,
    pub thinking: ThinkingOptions,
    pub thinking_capabilities: ThinkingCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOptionsError {
    ThinkingDisableNotDeclared {
        dialect: OpenAiChatDialect,
    },
    UnsupportedThinkingMode {
        dialect: OpenAiChatDialect,
        mode: ThinkingMode,
    },
    UnsupportedHistoryControl {
        dialect: OpenAiChatDialect,
    },
}

impl fmt::Display for RequestOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ThinkingDisableNotDeclared { dialect } => write!(
                formatter,
                "当前模型未声明支持在接口方言 {dialect:?} 下关闭思考",
            ),
            Self::UnsupportedThinkingMode { dialect, mode } => {
                write!(formatter, "接口方言 {dialect:?} 不支持思考策略 {mode:?}",)
            }
            Self::UnsupportedHistoryControl { dialect } => write!(
                formatter,
                "模型声明的历史思考控制能力无法由接口方言 {dialect:?} 表达",
            ),
        }
    }
}

impl Error for RequestOptionsError {}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub(crate) struct ThinkingRequestFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preserve_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<DeepSeekThinking>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct DeepSeekThinking {
    #[serde(rename = "type")]
    kind: DeepSeekThinkingType,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum DeepSeekThinkingType {
    Disabled,
}

impl OpenAiChatOptions {
    /// 不执行 I/O；供工厂在读取凭据之前预检查。
    pub fn validate(self) -> Result<(), RequestOptionsError> {
        self.into_request_fields().map(|_| ())
    }

    pub(super) fn into_request_fields(self) -> Result<ThinkingRequestFields, RequestOptionsError> {
        match self.thinking.mode {
            ThinkingMode::ProviderDefault => return Ok(ThinkingRequestFields::default()),
            ThinkingMode::Disabled => {}
        }

        if !self.thinking_capabilities.can_disable {
            return Err(RequestOptionsError::ThinkingDisableNotDeclared {
                dialect: self.dialect,
            });
        }

        match self.dialect {
            OpenAiChatDialect::DashScope => Ok(ThinkingRequestFields {
                enable_thinking: Some(false),
                preserve_thinking: self
                    .thinking_capabilities
                    .supports_history_control
                    .then_some(false),
                thinking: None,
            }),
            OpenAiChatDialect::DeepSeek => {
                if self.thinking_capabilities.supports_history_control {
                    return Err(RequestOptionsError::UnsupportedHistoryControl {
                        dialect: self.dialect,
                    });
                }
                Ok(ThinkingRequestFields {
                    enable_thinking: None,
                    preserve_thinking: None,
                    thinking: Some(DeepSeekThinking {
                        kind: DeepSeekThinkingType::Disabled,
                    }),
                })
            }
            OpenAiChatDialect::Generic => Err(RequestOptionsError::UnsupportedThinkingMode {
                dialect: self.dialect,
                mode: self.thinking.mode,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn selected(
        dialect: OpenAiChatDialect,
        mode: ThinkingMode,
        can_disable: bool,
        supports_history_control: bool,
    ) -> OpenAiChatOptions {
        OpenAiChatOptions {
            dialect,
            thinking: ThinkingOptions { mode },
            thinking_capabilities: ThinkingCapabilities {
                can_disable,
                supports_history_control,
            },
        }
    }

    #[test]
    fn provider_default_omits_all_thinking_fields_for_every_dialect() {
        for dialect in [
            OpenAiChatDialect::Generic,
            OpenAiChatDialect::DashScope,
            OpenAiChatDialect::DeepSeek,
        ] {
            for can_disable in [false, true] {
                for history in [false, true] {
                    let options =
                        selected(dialect, ThinkingMode::ProviderDefault, can_disable, history);
                    options.validate().unwrap();
                    assert_eq!(
                        serde_json::to_value(options.into_request_fields().unwrap()).unwrap(),
                        json!({})
                    );
                }
            }
        }
    }

    #[test]
    fn dashscope_sends_history_parameter_only_when_supported() {
        let options = selected(
            OpenAiChatDialect::DashScope,
            ThinkingMode::Disabled,
            true,
            true,
        );
        assert_eq!(
            serde_json::to_value(options.into_request_fields().unwrap()).unwrap(),
            json!({"enable_thinking": false, "preserve_thinking": false}),
        );
        let hosted = selected(
            OpenAiChatDialect::DashScope,
            ThinkingMode::Disabled,
            true,
            false,
        );
        assert_eq!(
            serde_json::to_value(hosted.into_request_fields().unwrap()).unwrap(),
            json!({"enable_thinking": false}),
        );
    }

    #[test]
    fn deepseek_disabled_uses_only_deepseek_fields() {
        let options = selected(
            OpenAiChatDialect::DeepSeek,
            ThinkingMode::Disabled,
            true,
            false,
        );
        assert_eq!(
            serde_json::to_value(options.into_request_fields().unwrap()).unwrap(),
            json!({"thinking": {"type": "disabled"}}),
        );
    }

    #[test]
    fn generic_disabled_fails_instead_of_falling_back() {
        let options = selected(
            OpenAiChatDialect::Generic,
            ThinkingMode::Disabled,
            true,
            false,
        );
        assert_eq!(
            options.validate(),
            Err(RequestOptionsError::UnsupportedThinkingMode {
                dialect: OpenAiChatDialect::Generic,
                mode: ThinkingMode::Disabled,
            }),
        );
        assert!(options.into_request_fields().is_err());
    }

    #[test]
    fn disabled_requires_explicit_model_capability() {
        for dialect in [
            OpenAiChatDialect::Generic,
            OpenAiChatDialect::DashScope,
            OpenAiChatDialect::DeepSeek,
        ] {
            for history in [false, true] {
                let options = selected(dialect, ThinkingMode::Disabled, false, history);
                assert_eq!(
                    options.validate(),
                    Err(RequestOptionsError::ThinkingDisableNotDeclared { dialect })
                );
                assert!(options.into_request_fields().is_err());
            }
        }
    }

    #[test]
    fn native_deepseek_rejects_unmappable_history_control() {
        let options = selected(
            OpenAiChatDialect::DeepSeek,
            ThinkingMode::Disabled,
            true,
            true,
        );
        assert_eq!(
            options.validate(),
            Err(RequestOptionsError::UnsupportedHistoryControl {
                dialect: OpenAiChatDialect::DeepSeek,
            })
        );
        assert!(options.into_request_fields().is_err());
    }
}
