use std::{env, sync::Arc};

use vortex_core::{
    ModelFactory, ModelInfo, ModelProvider, ModelSelectionError,
    config::{AppConfig, ConfigError, ProviderKind, RequestOptions},
};
use vortex_provider::{OpenAiChatProvider, QwenThinkingOptions};

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
}

impl ConfiguredModelFactory {
    pub(crate) fn new(config: AppConfig, client: reqwest::Client) -> Result<Self, ConfigError> {
        Self::with_key_reader(config, client, Arc::new(read_api_key))
    }

    fn with_key_reader(
        config: AppConfig,
        client: reqwest::Client,
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
                    unavailable_reason: (config.search.enabled && !model.supports_tools)
                        .then(|| "当前搜索工具已启用，该模型未声明工具调用支持".into()),
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok(Self {
            config,
            client,
            read_key,
            catalog,
        })
    }
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
        if self.config.search.enabled && !model.supports_tools {
            return Err(fail("当前搜索工具要求模型支持工具调用".into()));
        }
        let profile = self
            .config
            .providers
            .get(&model.provider)
            .ok_or_else(|| fail("模型引用的 Provider 不存在".into()))?;
        let key = (self.read_key)(&profile.api_key_env).map_err(|error| fail(error.0))?;
        let provider = match profile.kind {
            ProviderKind::OpenaiChat => {
                let provider = OpenAiChatProvider::from_client(
                    self.client.clone(),
                    &profile.base_url,
                    &model.model,
                    key,
                );
                match model.request_options {
                    RequestOptions::Plain => provider,
                    RequestOptions::QwenStageA => {
                        provider.with_qwen_thinking(QwenThinkingOptions {
                            enabled: false,
                            preserve_history: false,
                        })
                    }
                }
            }
        };
        Ok(Arc::new(provider))
    }
}
