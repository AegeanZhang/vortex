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
pub enum ReasoningHistoryPolicy {
    #[default]
    Unverified,
    DisplayOnlyWithHistoryDisabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenAiChatOptions {
    pub dialect: OpenAiChatDialect,
    pub history_policy: ReasoningHistoryPolicy,
    pub thinking: ThinkingOptions,
    pub thinking_capabilities: ThinkingCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOptionsError {
    ThinkingDisableNotDeclared {
        dialect: OpenAiChatDialect,
    },
    ThinkingEnableNotDeclared {
        dialect: OpenAiChatDialect,
    },
    DisplayOnlyHistoryUnsupported {
        dialect: OpenAiChatDialect,
    },
    DisplayOnlyHistoryUnverified {
        dialect: OpenAiChatDialect,
    },
    UnknownThinkingPreset,
    ThinkingPresetDialectMismatch {
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
            Self::ThinkingEnableNotDeclared { dialect } => write!(
                formatter,
                "当前模型未声明支持在接口方言 {dialect:?} 下开启思考",
            ),
            Self::DisplayOnlyHistoryUnsupported { dialect } => write!(
                formatter,
                "接口方言 {dialect:?} 的展示-only 模型要求声明支持关闭思考历史",
            ),
            Self::DisplayOnlyHistoryUnverified { dialect } => write!(
                formatter,
                "接口方言 {dialect:?} 的当前接入未声明展示-only 历史契约",
            ),
            Self::UnknownThinkingPreset => formatter.write_str("未知思考能力预设"),
            Self::ThinkingPresetDialectMismatch { dialect } => {
                write!(formatter, "思考能力预设与接口方言 {dialect:?} 不匹配",)
            }
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
            ThinkingMode::ProviderDefault => Ok(ThinkingRequestFields::default()),
            ThinkingMode::Enabled => {
                if self.dialect != OpenAiChatDialect::DashScope {
                    return Err(RequestOptionsError::UnsupportedThinkingMode {
                        dialect: self.dialect,
                        mode: self.thinking.mode,
                    });
                }
                if !self.thinking_capabilities.can_enable {
                    return Err(RequestOptionsError::ThinkingEnableNotDeclared {
                        dialect: self.dialect,
                    });
                }
                if !self.thinking_capabilities.supports_history_control {
                    return Err(RequestOptionsError::DisplayOnlyHistoryUnsupported {
                        dialect: self.dialect,
                    });
                }
                if self.history_policy != ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled {
                    return Err(RequestOptionsError::DisplayOnlyHistoryUnverified {
                        dialect: self.dialect,
                    });
                }
                Ok(ThinkingRequestFields {
                    enable_thinking: Some(true),
                    preserve_thinking: Some(false),
                    thinking: None,
                })
            }
            ThinkingMode::Disabled => {
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
                    OpenAiChatDialect::Generic => {
                        Err(RequestOptionsError::UnsupportedThinkingMode {
                            dialect: self.dialect,
                            mode: self.thinking.mode,
                        })
                    }
                }
            }
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
            history_policy: ReasoningHistoryPolicy::Unverified,
            thinking: ThinkingOptions { mode },
            thinking_capabilities: ThinkingCapabilities {
                can_disable,
                can_enable: false,
                supports_history_control,
            },
        }
    }

    fn enabled_options() -> OpenAiChatOptions {
        OpenAiChatOptions {
            dialect: OpenAiChatDialect::DashScope,
            history_policy: ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled,
            thinking: ThinkingOptions {
                mode: ThinkingMode::Enabled,
            },
            thinking_capabilities: ThinkingCapabilities {
                can_enable: true,
                can_disable: true,
                supports_history_control: true,
            },
        }
    }

    #[test]
    fn dashscope_enabled_disables_reasoning_history() {
        assert_eq!(
            serde_json::to_value(enabled_options().into_request_fields().unwrap()).unwrap(),
            json!({"enable_thinking": true, "preserve_thinking": false}),
        );
    }

    #[test]
    fn enabled_requires_enable_capability() {
        let mut options = enabled_options();
        options.thinking_capabilities.can_enable = false;
        assert_eq!(
            options.validate(),
            Err(RequestOptionsError::ThinkingEnableNotDeclared {
                dialect: OpenAiChatDialect::DashScope,
            })
        );
    }

    #[test]
    fn enabled_requires_history_control() {
        let mut options = enabled_options();
        options.thinking_capabilities.supports_history_control = false;
        assert_eq!(
            options.validate(),
            Err(RequestOptionsError::DisplayOnlyHistoryUnsupported {
                dialect: OpenAiChatDialect::DashScope,
            })
        );
    }

    #[test]
    fn enabled_requires_verified_history_contract_even_with_all_capabilities() {
        let mut options = enabled_options();
        options.history_policy = ReasoningHistoryPolicy::Unverified;
        assert_eq!(
            options.validate(),
            Err(RequestOptionsError::DisplayOnlyHistoryUnverified {
                dialect: OpenAiChatDialect::DashScope,
            })
        );
    }

    #[test]
    fn enabled_rejects_other_dialects_even_with_all_capabilities() {
        for dialect in [OpenAiChatDialect::Generic, OpenAiChatDialect::DeepSeek] {
            let mut options = enabled_options();
            options.dialect = dialect;
            assert_eq!(
                options.validate(),
                Err(RequestOptionsError::UnsupportedThinkingMode {
                    dialect,
                    mode: ThinkingMode::Enabled,
                })
            );
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
