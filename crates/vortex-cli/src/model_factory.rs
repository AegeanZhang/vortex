use std::{env, sync::Arc};

use vortex_core::{
    ModelFactory, ModelInfo, ModelProvider, ModelSelectionError, ThinkingCapabilities,
    ThinkingOptions,
    config::{ApiFlavor, AppConfig, ConfigError, ProviderKind},
};
use vortex_provider::{
    OpenAiChatDialect, OpenAiChatOptions, OpenAiChatProvider, RequestOptionsError,
};

#[derive(Debug)]
pub(crate) struct CredentialError(pub(crate) String);
impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CredentialError {}

pub(crate) fn read_api_key(name: &str) -> Result<String, CredentialError> {
    let value = env::var(name).map_err(|error| {
        CredentialError(match error {
            env::VarError::NotPresent => format!("缺少环境变量 {name}, 请在启动前导出"),
            env::VarError::NotUnicode(_) => format!("环境变量 {name} 不是有效 Unicode 字符串"),
        })
    })?;
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(CredentialError(format!(
            "环境变量 {name} 为空或包含空白字符"
        )));
    }
    reqwest::header::HeaderValue::from_str(&format!("Bearer {value}"))
        .map_err(|_| CredentialError(format!("环境变量 {name} 的认证格式无效")))?;
    Ok(value.into())
}

type KeyReader = dyn Fn(&str) -> Result<String, CredentialError> + Send + Sync;

pub(crate) struct ConfiguredModelFactory {
    config: AppConfig,
    client: reqwest::Client,
    read_key: Arc<KeyReader>,
    catalog: Vec<ModelInfo>,
    requires_tools: bool,
}

impl ConfiguredModelFactory {
    pub(crate) fn new(
        config: AppConfig,
        client: reqwest::Client,
        requires_tools: bool,
    ) -> Result<Self, ConfigError> {
        Self::with_key_reader(config, client, requires_tools, Arc::new(read_api_key))
    }

    fn with_key_reader(
        config: AppConfig,
        client: reqwest::Client,
        requires_tools: bool,
        read_key: Arc<KeyReader>,
    ) -> Result<Self, ConfigError> {
        config.validate()?;
        let catalog = config
            .models
            .iter()
            .map(|(id, model)| {
                let provider = config
                    .providers
                    .get(&model.provider)
                    .ok_or(ConfigError::Invalid("模型 Provider 引用无效"))?;
                let url = url::Url::parse(&provider.base_url)
                    .map_err(|_| ConfigError::Invalid("Provider URL 无效"))?;
                let host = url
                    .host_str()
                    .ok_or(ConfigError::Invalid("Provider URL 缺少 host"))?;
                Ok(ModelInfo {
                    id: id.clone(),
                    display_name: model.display_name.clone(),
                    provider_id: model.provider.clone(),
                    endpoint_host: host.into(),
                    supports_tools: model.supports_tools,
                    unavailable_reason: (requires_tools && !model.supports_tools)
                        .then(|| "当前已注册工具，该模型未声明工具调用支持".into()),
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok(Self {
            config,
            client,
            read_key,
            catalog,
            requires_tools,
        })
    }
}

fn chat_options(
    api_flavor: ApiFlavor,
    thinking: ThinkingOptions,
    thinking_capabilities: ThinkingCapabilities,
) -> Result<OpenAiChatOptions, RequestOptionsError> {
    let dialect = match api_flavor {
        ApiFlavor::Generic => OpenAiChatDialect::Generic,
        ApiFlavor::Dashscope => OpenAiChatDialect::DashScope,
        ApiFlavor::Deepseek => OpenAiChatDialect::DeepSeek,
    };
    let options = OpenAiChatOptions {
        dialect,
        thinking,
        thinking_capabilities,
    };
    options.validate()?;
    Ok(options)
}

impl ModelFactory for ConfiguredModelFactory {
    fn catalog(&self) -> Vec<ModelInfo> {
        self.catalog.clone()
    }

    fn create(&self, id: &str) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        let fail = |message: String| ModelSelectionError {
            model_id: id.into(),
            message,
        };
        let model = self
            .config
            .models
            .get(id)
            .ok_or_else(|| fail("不存在这个模型 profile".into()))?;
        if self.requires_tools && !model.supports_tools {
            return Err(fail("当前已注册工具，该模型未声明工具调用支持".into()));
        }
        let profile = self
            .config
            .providers
            .get(&model.provider)
            .ok_or_else(|| fail("模型引用的 Provider 不存在".into()))?;

        let provider = match profile.kind {
            ProviderKind::OpenaiChat => {
                let options = chat_options(
                    profile.api_flavor,
                    model.thinking,
                    model.thinking_capabilities,
                )
                .map_err(|error| fail(error.to_string()))?;
                let key = (self.read_key)(&profile.api_key_env).map_err(|error| fail(error.0))?;
                OpenAiChatProvider::from_client(
                    self.client.clone(),
                    &profile.base_url,
                    &model.model,
                    key,
                )
                .with_options(options)
                .map_err(|error| fail(error.to_string()))?
            }
        };
        Ok(Arc::new(provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vortex_core::{
        ModelManager, ThinkingMode,
        config::{ModelConfig, ProviderConfig},
    };

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[test]
    fn invalid_thinking_combination_is_rejected_before_reading_key() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config.providers.get_mut("qwen").unwrap().api_flavor = ApiFlavor::Generic;
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            true,
            Arc::new(|_| panic!("invalid options must not read a key")),
        )
        .unwrap();
        let error = factory.create("qwen-max").err().unwrap();
        assert!(error.message.contains("不支持思考策略"));
    }

    #[test]
    fn missing_capability_is_rejected_before_reading_key() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config
            .models
            .get_mut("qwen-max")
            .unwrap()
            .thinking_capabilities = ThinkingCapabilities::default();
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            true,
            Arc::new(|_| panic!("missing capability must not read a key")),
        )
        .unwrap();
        let error = factory.create("qwen-max").err().unwrap();
        assert!(error.message.contains("未声明支持"));
    }

    #[test]
    fn unmappable_history_capability_is_rejected_before_reading_key() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config.providers.get_mut("qwen").unwrap().api_flavor = ApiFlavor::Deepseek;
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            true,
            Arc::new(|_| panic!("invalid history capability must not read a key")),
        )
        .unwrap();
        let error = factory.create("qwen-max").err().unwrap();
        assert!(error.message.contains("历史思考控制能力"));
    }

    #[test]
    fn model_selection_reads_only_selected_credentials() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config.providers.insert(
            "deepseek".into(),
            ProviderConfig {
                kind: ProviderKind::OpenaiChat,
                api_flavor: ApiFlavor::Deepseek,
                base_url: "https://example.test".into(),
                api_key_env: "DEEPSEEK_API_KEY".into(),
            },
        );
        config.models.insert(
            "deepseek".into(),
            ModelConfig {
                provider: "deepseek".into(),
                model: "fixture-model".into(),
                display_name: "Fixture".into(),
                supports_tools: true,
                thinking: ThinkingOptions {
                    mode: ThinkingMode::Disabled,
                },
                thinking_capabilities: ThinkingCapabilities {
                    can_disable: true,
                    supports_history_control: false,
                },
            },
        );
        let reads = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = reads.clone();
        let factory = Arc::new(
            ConfiguredModelFactory::with_key_reader(
                config,
                test_client(),
                true,
                Arc::new(move |name| {
                    captured.lock().unwrap().push(name.into());
                    Ok("fixture-key".into())
                }),
            )
            .unwrap(),
        );
        assert!(reads.lock().unwrap().is_empty());
        let mut manager = ModelManager::new(factory, "qwen-max").unwrap();
        assert_eq!(*reads.lock().unwrap(), vec!["QWEN_API_KEY".to_string()]);
        manager.select("deepseek").unwrap();
        assert_eq!(
            *reads.lock().unwrap(),
            vec!["QWEN_API_KEY".to_string(), "DEEPSEEK_API_KEY".to_string(),]
        );
    }

    #[test]
    fn same_provider_can_assemble_distinct_model_thinking_policies() {
        let supported = ThinkingCapabilities {
            can_disable: true,
            supports_history_control: true,
        };
        let default = chat_options(
            ApiFlavor::Dashscope,
            ThinkingOptions::default(),
            ThinkingCapabilities::default(),
        )
        .unwrap();
        let disabled = chat_options(
            ApiFlavor::Dashscope,
            ThinkingOptions {
                mode: ThinkingMode::Disabled,
            },
            supported,
        )
        .unwrap();
        assert_eq!(default.dialect, disabled.dialect);
        assert_eq!(default.thinking.mode, ThinkingMode::ProviderDefault);
        assert_eq!(disabled.thinking.mode, ThinkingMode::Disabled);
        let hosted_capabilities = ThinkingCapabilities {
            can_disable: true,
            supports_history_control: false,
        };
        let hosted =
            chat_options(ApiFlavor::Dashscope, disabled.thinking, hosted_capabilities).unwrap();
        assert_eq!(hosted.dialect, disabled.dialect);
        assert!(!hosted.thinking_capabilities.supports_history_control);
        assert!(disabled.thinking_capabilities.supports_history_control);
        assert_eq!(
            chat_options(ApiFlavor::Deepseek, disabled.thinking, hosted_capabilities)
                .unwrap()
                .dialect,
            OpenAiChatDialect::DeepSeek,
        );
    }

    fn config_with_undeclared_model() -> AppConfig {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        let mut model = config.models["qwen-max"].clone();
        model.thinking_capabilities = ThinkingCapabilities::default();
        config.models.insert("undeclared".into(), model);
        config
    }

    #[test]
    fn same_provider_models_do_not_share_thinking_capabilities() {
        let reads = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = reads.clone();
        let factory = ConfiguredModelFactory::with_key_reader(
            config_with_undeclared_model(),
            test_client(),
            true,
            Arc::new(move |name| {
                captured.lock().unwrap().push(name.into());
                Ok("fixture-key".into())
            }),
        )
        .unwrap();

        assert!(factory.create("qwen-max").is_ok());
        let error = factory.create("undeclared").err().unwrap();
        assert!(error.message.contains("未声明支持"));
        assert!(factory.create("qwen-max").is_ok());
        assert_eq!(
            *reads.lock().unwrap(),
            vec!["QWEN_API_KEY".to_string(), "QWEN_API_KEY".to_string()],
        );
    }

    #[test]
    fn failed_thinking_selection_keeps_current_model_and_provider() {
        let reads = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = reads.clone();
        let factory = Arc::new(
            ConfiguredModelFactory::with_key_reader(
                config_with_undeclared_model(),
                test_client(),
                true,
                Arc::new(move |name| {
                    captured.lock().unwrap().push(name.into());
                    Ok("fixture-key".into())
                }),
            )
            .unwrap(),
        );
        let mut manager = ModelManager::new(factory, "qwen-max").unwrap();
        let original = manager.provider();

        assert!(manager.select("undeclared").is_err());
        assert_eq!(manager.current().id, "qwen-max");
        assert!(Arc::ptr_eq(&original, &manager.provider()));
        assert_eq!(*reads.lock().unwrap(), vec!["QWEN_API_KEY".to_string()]);
    }

    #[test]
    fn actual_tools_disable_incompatible_models_even_when_search_is_off() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config.models.get_mut("qwen-max").unwrap().supports_tools = false;
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            true,
            Arc::new(|_| panic!("incompatible model must not read a key")),
        )
        .unwrap();
        let catalog = factory.catalog();
        let info = catalog.iter().find(|info| info.id == "qwen-max").unwrap();
        assert!(
            info.unavailable_reason
                .as_deref()
                .unwrap()
                .contains("已注册工具")
        );
        let error = factory.create("qwen-max").err().unwrap();
        assert!(error.message.contains("已注册工具"));
    }

    #[test]
    fn empty_tool_requirement_allows_a_model_without_tools() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        config.models.get_mut("qwen-max").unwrap().supports_tools = false;
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            false,
            Arc::new(|_| Ok("fixture-key-not-a-real-secret".into())),
        )
        .unwrap();
        assert!(
            factory
                .catalog()
                .iter()
                .all(|info| info.unavailable_reason.is_none())
        );
        assert!(factory.create("qwen-max").is_ok());
    }

    #[test]
    fn failed_selection_for_tool_capability_keeps_current_model() {
        let mut config = AppConfig::default();
        config.search.enabled = false;
        let mut unsupported = config.models.get("qwen-max").unwrap().clone();
        unsupported.supports_tools = false;
        config.models.insert("without-tools".into(), unsupported);
        let factory = ConfiguredModelFactory::with_key_reader(
            config,
            test_client(),
            true,
            Arc::new(|_| Ok("fixture-key-not-a-real-secret".into())),
        )
        .unwrap();
        let mut manager = ModelManager::new(Arc::new(factory), "qwen-max").unwrap();
        let original = manager.provider();
        assert!(manager.select("without-tools").is_err());
        assert_eq!(manager.current().id, "qwen-max");
        assert!(Arc::ptr_eq(&original, &manager.provider()));
    }
}
