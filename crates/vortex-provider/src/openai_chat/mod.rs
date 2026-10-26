mod decoder;
mod options;
mod request;
mod stream;
mod thinking_presets;

pub use options::{
    OpenAiChatDialect, OpenAiChatOptions, ReasoningHistoryPolicy, RequestOptionsError,
};
pub use thinking_presets::{ResolvedThinkingProfile, resolve_thinking_profile};

use self::{options::ThinkingRequestFields, request::ChatRequest, stream::StreamContext};

use vortex_core::{ModelProvider, ModelRequest, ModelStream, ReasoningOutputPolicy, ThinkingMode};

#[derive(Clone)]
struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
}

pub struct OpenAiChatProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: SecretString,
    //qwen_thinking: Option<QwenThinkingOptions>,
    thinking_fields: ThinkingRequestFields,
    reasoning_policy: ReasoningOutputPolicy,
}

impl OpenAiChatProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self::from_client(reqwest::Client::new(), base_url, model, api_key)
    }

    pub fn from_client(
        client: reqwest::Client,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            model: model.into(),
            api_key: SecretString(api_key.into()),
            //qwen_thinking: None,
            thinking_fields: ThinkingRequestFields::default(),
            reasoning_policy: ReasoningOutputPolicy::Reject,
        }
    }

    pub fn with_options(mut self, options: OpenAiChatOptions) -> Result<Self, RequestOptionsError> {
        let fields = options.into_request_fields()?;
        let policy = match options.thinking.mode {
            ThinkingMode::Enabled => ReasoningOutputPolicy::DisplayOnly,
            ThinkingMode::ProviderDefault | ThinkingMode::Disabled => ReasoningOutputPolicy::Reject,
        };
        self.thinking_fields = fields;
        self.reasoning_policy = policy;
        Ok(self)
    }
}

impl ModelProvider for OpenAiChatProvider {
    fn stream(&self, model_request: ModelRequest) -> ModelStream {
        let request = ChatRequest::from_model_request(
            self.model.clone(),
            model_request,
            self.thinking_fields,
        );

        let context = StreamContext {
            client: self.client.clone(),
            url: format!("{}/chat/completions", self.base_url.trim_end_matches('/'),),
            api_key: self.api_key.clone(),
            request,
        };

        stream::stream(context)
    }

    fn reasoning_output_policy(&self) -> ReasoningOutputPolicy {
        self.reasoning_policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vortex_core::{ThinkingCapabilities, ThinkingMode, ThinkingOptions};

    fn fixture_provider() -> OpenAiChatProvider {
        OpenAiChatProvider::new("https://example.test", "fixture-model", "fixture-key")
    }

    #[test]
    fn builtin_profile_still_sends_disabled_fields_after_resolving_its_preset() {
        let config = vortex_core::config::AppConfig::default();
        let model = &config.models["qwen-max"];
        let resolved = resolve_thinking_profile(
            OpenAiChatDialect::DashScope,
            model.thinking_preset.as_deref(),
            model.thinking_capabilities,
        )
        .unwrap();
        let provider = fixture_provider()
            .with_options(OpenAiChatOptions {
                dialect: OpenAiChatDialect::DashScope,
                history_policy: resolved.history_policy,
                thinking: model.thinking,
                thinking_capabilities: resolved.capabilities,
            })
            .unwrap();
        assert_eq!(
            provider.reasoning_output_policy(),
            ReasoningOutputPolicy::Reject
        );
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({"enable_thinking": false, "preserve_thinking": false})
        );
    }

    #[test]
    fn replacing_enabled_options_resets_output_policy_and_request_fields() {
        let enabled = OpenAiChatOptions {
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
        };
        for mode in [ThinkingMode::ProviderDefault, ThinkingMode::Disabled] {
            let provider = fixture_provider().with_options(enabled).unwrap();
            assert_eq!(
                provider.reasoning_output_policy(),
                ReasoningOutputPolicy::DisplayOnly
            );
            assert_eq!(
                serde_json::to_value(provider.thinking_fields).unwrap(),
                json!({"enable_thinking": true, "preserve_thinking": false})
            );
            let provider = provider
                .with_options(OpenAiChatOptions {
                    thinking: ThinkingOptions { mode },
                    ..enabled
                })
                .unwrap();
            assert_eq!(
                provider.reasoning_output_policy(),
                ReasoningOutputPolicy::Reject
            );
            let expected = if mode == ThinkingMode::ProviderDefault {
                json!({})
            } else {
                json!({"enable_thinking": false, "preserve_thinking": false})
            };
            assert_eq!(
                serde_json::to_value(provider.thinking_fields).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn default_builder_omits_fields_and_invalid_options_fail() {
        let provider = fixture_provider();
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({})
        );
        assert!(
            provider
                .with_options(OpenAiChatOptions {
                    dialect: OpenAiChatDialect::Generic,
                    history_policy: ReasoningHistoryPolicy::Unverified,
                    thinking: ThinkingOptions {
                        mode: ThinkingMode::Disabled
                    },
                    thinking_capabilities: ThinkingCapabilities {
                        can_disable: true,
                        can_enable: false,
                        supports_history_control: false,
                    },
                })
                .is_err()
        );
    }

    #[test]
    fn replacing_options_never_leaves_previous_vendor_fields() {
        let provider = fixture_provider()
            .with_options(OpenAiChatOptions {
                dialect: OpenAiChatDialect::DashScope,
                history_policy: ReasoningHistoryPolicy::Unverified,
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
                    can_enable: false,
                    supports_history_control: true,
                },
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({
                "enable_thinking": false,
                "preserve_thinking": false,
            })
        );
        let provider = provider
            .with_options(OpenAiChatOptions {
                dialect: OpenAiChatDialect::DashScope,
                history_policy: ReasoningHistoryPolicy::Unverified,
                thinking: ThinkingOptions::default(),
                thinking_capabilities: ThinkingCapabilities::default(),
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({})
        );
        let provider = provider
            .with_options(OpenAiChatOptions {
                dialect: OpenAiChatDialect::DashScope,
                history_policy: ReasoningHistoryPolicy::Unverified,
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
                    can_enable: false,
                    supports_history_control: false,
                },
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({
                "enable_thinking": false,
            })
        );
        let provider = provider
            .with_options(OpenAiChatOptions {
                dialect: OpenAiChatDialect::DeepSeek,
                history_policy: ReasoningHistoryPolicy::Unverified,
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
                    can_enable: false,
                    supports_history_control: false,
                },
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(provider.thinking_fields).unwrap(),
            json!({
                "thinking": {"type": "disabled"},
            })
        );
    }
}
