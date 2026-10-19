mod decoder;
mod options;
mod request;
mod stream;

pub use options::{OpenAiChatDialect, OpenAiChatOptions, RequestOptionsError};

use self::{options::ThinkingRequestFields, request::ChatRequest, stream::StreamContext};

use vortex_core::{ModelProvider, ModelRequest, ModelStream};

#[derive(Clone)]
struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
}

/*
#[derive(Debug, Clone, Copy)]
pub struct QwenThinkingOptions {
    pub enabled: bool,
    pub preserve_history: bool,
}
*/

pub struct OpenAiChatProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: SecretString,
    //qwen_thinking: Option<QwenThinkingOptions>,
    thinking_fields: ThinkingRequestFields,
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
        }
    }

    /*
    pub fn with_qwen_thinking(mut self, options: QwenThinkingOptions) -> Self {
        self.qwen_thinking = Some(options);
        self
    }
    */

    pub fn with_options(mut self, options: OpenAiChatOptions) -> Result<Self, RequestOptionsError> {
        self.thinking_fields = options.into_request_fields()?;
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
                    thinking: ThinkingOptions {
                        mode: ThinkingMode::Disabled
                    },
                    thinking_capabilities: ThinkingCapabilities {
                        can_disable: true,
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
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
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
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
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
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
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
