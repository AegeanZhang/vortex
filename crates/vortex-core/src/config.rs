use serde::{Deserialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fmt, path::PathBuf};

use crate::{ThinkingCapabilitiesOverride, ThinkingOptions, ToolCallLimits};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    OpenaiChat,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ApiFlavor {
    #[default]
    Generic,
    Dashscope,
    Deepseek,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    #[serde(default)]
    pub api_flavor: ApiFlavor,
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub display_name: String,
    pub supports_tools: bool,
    //pub request_options: RequestOptions,
    #[serde(default)]
    pub thinking: ThinkingOptions,
    #[serde(default)]
    pub thinking_preset: Option<String>,
    #[serde(default)]
    pub thinking_capabilities: ThinkingCapabilitiesOverride,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SearchBackend {
    Metaso,
    Mock,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub enabled: bool,
    pub backend: SearchBackend,
    pub api_key_env: String,
    pub max_results: usize,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub default_model: String,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub models: BTreeMap<String, ModelConfig>,
    pub search: SearchConfig,
    pub tools: ToolsConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserConfigPatch {
    pub default_model: Option<String>,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub models: BTreeMap<String, ModelConfig>,
    pub search: Option<SearchPatch>,
    pub tools: Option<ToolsPatch>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchPatch {
    pub enabled: Option<bool>,
    pub backend: Option<SearchBackend>,
    pub api_key_env: Option<String>,
    pub max_results: Option<usize>,
    pub connect_timeout_secs: Option<u64>,
    pub request_timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfigPatch {
    pub default_model: Option<String>,
    pub search: Option<ProjectSearchPatch>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectSearchPatch {
    pub enabled: Option<bool>,
    pub max_results: Option<usize>,
}

pub struct ConfigPaths {
    pub user: PathBuf,
    pub project: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFileStatus {
    Loaded,
    Missing,
}

impl ConfigFileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    pub status: ConfigFileStatus,
}

#[derive(Debug, Clone)]
pub struct ConfigSources {
    pub user: ConfigSource,
    pub project: ConfigSource,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub sources: ConfigSources,
}

#[derive(Debug)]
pub enum ConfigError {
    Invalid(&'static str),
    Read {
        path: std::path::PathBuf,
        kind: std::io::ErrorKind,
    },
    Parse {
        path: std::path::PathBuf,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "配置错误: {message}"),
            Self::Read { path, kind } => write!(f, "无法读取配置 {:?}: {kind:?}", path.as_os_str()),
            Self::Parse { path } => write!(f, "配置 {:?} 的 TOML 或字段类型无效", path.as_os_str()),
        }
    }
}
impl std::error::Error for ConfigError {}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_model: "qwen-max".into(),
            providers: BTreeMap::from([(
                "qwen".into(),
                ProviderConfig {
                    kind: ProviderKind::OpenaiChat,
                    api_flavor: ApiFlavor::Dashscope,
                    base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                    api_key_env: "QWEN_API_KEY".into(),
                },
            )]),
            models: BTreeMap::from([(
                "qwen-max".into(),
                ModelConfig {
                    provider: "qwen".into(),
                    model: "qwen3.8-max".into(),
                    display_name: "Qwen Max".into(),
                    supports_tools: true,
                    //request_options: RequestOptions::QwenStageA,
                    thinking: ThinkingOptions {
                        mode: crate::ThinkingMode::Disabled,
                    },
                    thinking_preset: Some("dashscope-qwen-display-only".into()),
                    thinking_capabilities: ThinkingCapabilitiesOverride::default(),
                },
            )]),
            search: SearchConfig {
                enabled: true,
                backend: SearchBackend::Metaso,
                api_key_env: "METASO_API_KEY".into(),
                max_results: 5,
                connect_timeout_secs: 5,
                request_timeout_secs: 20,
            },
            tools: ToolsConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn apply_user(&mut self, patch: UserConfigPatch) {
        if let Some(value) = patch.default_model {
            self.default_model = value;
        }
        self.providers.extend(patch.providers);
        self.models.extend(patch.models);
        if let Some(patch) = patch.search {
            if let Some(v) = patch.enabled {
                self.search.enabled = v;
            }
            if let Some(v) = patch.backend {
                self.search.backend = v;
            }
            if let Some(v) = patch.api_key_env {
                self.search.api_key_env = v;
            }
            if let Some(v) = patch.max_results {
                self.search.max_results = v;
            }
            if let Some(v) = patch.connect_timeout_secs {
                self.search.connect_timeout_secs = v;
            }
            if let Some(v) = patch.request_timeout_secs {
                self.search.request_timeout_secs = v;
            }
        }

        if let Some(patch) = patch.tools {
            self.tools.call_limits.extend(patch.call_limits);
        }
    }

    pub fn apply_project(&mut self, patch: ProjectConfigPatch) -> Result<(), ConfigError> {
        if let Some(id) = &patch.default_model
            && !self.models.contains_key(id)
        {
            return Err(ConfigError::Invalid("项目默认模型必须引用已有 profile"));
        }
        if let Some(search) = &patch.search {
            if search.enabled == Some(true) && !self.search.enabled {
                return Err(ConfigError::Invalid("项目配置不能重新开启用户已关闭的搜索"));
            }
            if let Some(limit) = search.max_results
                && !(1..=self.search.max_results).contains(&limit)
            {
                return Err(ConfigError::Invalid("项目搜索条数只能降低且不能为零"));
            }
        }
        if let Some(id) = patch.default_model {
            self.default_model = id;
        }
        if let Some(search) = patch.search {
            if let Some(v) = search.enabled {
                self.search.enabled = v;
            }
            if let Some(v) = search.max_results {
                self.search.max_results = v;
            }
        }
        Ok(())
    }

    pub fn apply_override(&mut self, override_value: &str) -> Result<(), ConfigError> {
        let (key, raw) = override_value
            .split_once('=')
            .ok_or(ConfigError::Invalid("-c 必须使用 KEY=VALUE"))?;
        match key {
            "default_model" => self.default_model = parse_cli_value(raw)?,
            "search.enabled" => self.search.enabled = parse_cli_value(raw)?,
            "search.backend" => self.search.backend = parse_cli_value(raw)?,
            "search.max_results" => self.search.max_results = parse_cli_value(raw)?,
            "search.connect_timeout_secs" => {
                self.search.connect_timeout_secs = parse_cli_value(raw)?
            }
            "search.request_timeout_secs" => {
                self.search.request_timeout_secs = parse_cli_value(raw)?
            }
            _ => return Err(ConfigError::Invalid("不支持这个 -c 配置项")),
        }
        Ok(())
    }

    pub fn validate_definitions(&self) -> Result<(), ConfigError> {
        if self.providers.len() > 16 || self.models.is_empty() || self.models.len() > 64 {
            return Err(ConfigError::Invalid(
                "Provider/模型数量超过限制或模型列表为空",
            ));
        }
        for (id, provider) in &self.providers {
            if !valid_profile_id(id) || !valid_env_name(&provider.api_key_env) {
                return Err(ConfigError::Invalid("Provider ID 或 api_key_env 不合法"));
            }
            if !valid_base_url(&provider.base_url) {
                return Err(ConfigError::Invalid(
                    "Provider base_url 必须是合法 HTTPS API base",
                ));
            }
        }
        for (id, model) in &self.models {
            if !valid_profile_id(id) || !self.providers.contains_key(&model.provider) {
                return Err(ConfigError::Invalid(
                    "模型 ID 不合法或引用了不存在的 Provider",
                ));
            }
            if model
                .thinking_preset
                .as_deref()
                .is_some_and(|id| !valid_profile_id(id))
            {
                return Err(ConfigError::Invalid("思考预设 ID 不合法"));
            }
            if model.model.is_empty()
                || model.model.len() > 128
                || model
                    .model
                    .chars()
                    .any(|ch| ch.is_whitespace() || ch.is_control())
            {
                return Err(ConfigError::Invalid("服务端模型名称不合法"));
            }
            if model.display_name.trim().is_empty()
                || model.display_name.chars().count() > 80
                || model.display_name.chars().any(char::is_control)
            {
                return Err(ConfigError::Invalid("模型显示名称不合法"));
            }
        }
        let search = &self.search;
        if !valid_env_name(&search.api_key_env)
            || !(1..=5).contains(&search.max_results)
            || !(1..=30).contains(&search.connect_timeout_secs)
            || !(1..=120).contains(&search.request_timeout_secs)
            || search.connect_timeout_secs > search.request_timeout_secs
        {
            return Err(ConfigError::Invalid("搜索环境变量、条数或超时不合法"));
        }

        ToolCallLimits::new(self.tools.call_limits.clone())
            .map_err(|_| ConfigError::Invalid("tools.call_limits 的名称或次数不合法"))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_definitions()?;
        let default = self
            .models
            .get(&self.default_model)
            .ok_or(ConfigError::Invalid("default_model 没有对应模型 profile"))?;
        if self.search.enabled && !default.supports_tools {
            return Err(ConfigError::Invalid("开启搜索时默认模型必须支持工具调用"));
        }
        Ok(())
    }
}

fn parse_cli_value<T: DeserializeOwned>(raw: &str) -> Result<T, ConfigError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wrapper<T> {
        value: T,
    }
    toml::from_str::<Wrapper<T>>(&format!("value = {raw}"))
        .map(|wrapper| wrapper.value)
        .map_err(|_| ConfigError::Invalid("-c 的值必须是对应类型的 TOML 字面量"))
}

pub fn valid_profile_id(id: &str) -> bool {
    let allowed_start = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !id.is_empty()
        && id.len() <= 64
        && allowed_start(id.as_bytes()[0])
        && id.bytes().all(|b| allowed_start(b) || b == b'-')
}

fn valid_base_url(value: &str) -> bool {
    if value.len() > 2_048
        || value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && !url
            .path()
            .trim_end_matches('/')
            .ends_with("/chat/completions")
}

fn valid_env_name(value: &str) -> bool {
    let start = |b: u8| b.is_ascii_uppercase() || b == b'_';
    !value.is_empty()
        && value.len() <= 128
        && start(value.as_bytes()[0])
        && value.bytes().all(|b| start(b) || b.is_ascii_digit())
}

pub fn load_config(
    paths: &ConfigPaths,
    overrides: &[String],
    model_override: Option<&str>,
) -> Result<LoadedConfig, ConfigError> {
    let mut config = AppConfig::default();
    let user = read_optional::<UserConfigPatch>(&paths.user)?;
    let user_status = if user.is_some() {
        ConfigFileStatus::Loaded
    } else {
        ConfigFileStatus::Missing
    };
    if let Some(patch) = user {
        config.apply_user(patch);
    }
    // 定义非法仍拒绝，但最终选择可能被项目、-c 或 -m 改变。
    config.validate_definitions()?;
    check_project_path(&paths.project)?;
    let project = read_optional::<ProjectConfigPatch>(&paths.project)?;
    let project_status = if project.is_some() {
        ConfigFileStatus::Loaded
    } else {
        ConfigFileStatus::Missing
    };
    if let Some(patch) = project {
        config.apply_project(patch)?;
    }
    for override_value in overrides {
        config.apply_override(override_value)?;
    }
    if let Some(id) = model_override {
        config.default_model = id.to_string();
    }
    config.validate()?;
    Ok(LoadedConfig {
        config,
        sources: ConfigSources {
            user: ConfigSource {
                path: paths.user.clone(),
                status: user_status,
            },
            project: ConfigSource {
                path: paths.project.clone(),
                status: project_status,
            },
        },
    })
}

fn read_optional<T: DeserializeOwned>(path: &std::path::Path) -> Result<Option<T>, ConfigError> {
    use std::io::Read;
    // 先排除目录/设备/管道，避免普通 open 在命名管道上等待生产者。
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ConfigError::Read {
                path: path.into(),
                kind: error.kind(),
            });
        }
    };
    if !metadata.is_file() {
        return Err(ConfigError::Invalid("配置路径必须是普通文件"));
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ConfigError::Read {
                path: path.into(),
                kind: error.kind(),
            });
        }
    };
    let metadata = file.metadata().map_err(|error| ConfigError::Read {
        path: path.into(),
        kind: error.kind(),
    })?;
    if !metadata.is_file() {
        return Err(ConfigError::Invalid("配置路径必须是普通文件"));
    }
    let mut bytes = Vec::new();
    file.take(64 * 1_024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ConfigError::Read {
            path: path.into(),
            kind: error.kind(),
        })?;
    if bytes.len() > 64 * 1_024 {
        return Err(ConfigError::Invalid("配置文件超过 64 KiB"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| ConfigError::Parse { path: path.into() })?;
    toml::from_str(text)
        .map(Some)
        .map_err(|_| ConfigError::Parse { path: path.into() })
}

fn check_project_path(path: &std::path::Path) -> Result<(), ConfigError> {
    let directory = path
        .parent()
        .ok_or(ConfigError::Invalid("项目配置路径缺少目录"))?;
    let workspace = directory
        .parent()
        .ok_or(ConfigError::Invalid("项目配置路径缺少工作区"))?;
    for candidate in [directory, path] {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ConfigError::Invalid("项目配置及其目录不能是符号链接"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(ConfigError::Read {
                    path: candidate.into(),
                    kind: error.kind(),
                });
            }
        }
    }
    let canonical = |candidate: &std::path::Path| {
        candidate.canonicalize().map_err(|error| ConfigError::Read {
            path: candidate.into(),
            kind: error.kind(),
        })
    };
    if !canonical(path)?.starts_with(canonical(workspace)?) {
        return Err(ConfigError::Invalid("项目配置路径越过工作区边界"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThinkingMode;

    #[test]
    fn builtin_qwen_explicitly_disables_thinking() {
        let config = AppConfig::default();
        let model = &config.models["qwen-max"];

        assert_eq!(config.providers["qwen"].api_flavor, ApiFlavor::Dashscope);
        assert_eq!(model.thinking.mode, ThinkingMode::Disabled);
        assert_eq!(
            model.thinking_preset.as_deref(),
            Some("dashscope-qwen-display-only"),
        );
        assert_eq!(
            model.thinking_capabilities,
            ThinkingCapabilitiesOverride::default(),
        );
    }

    #[test]
    fn parses_official_and_hosted_models_with_distinct_flavors() {
        let patch: UserConfigPatch = toml::from_str(
            r#"
[providers.example]
kind = "openai-chat"
api_flavor = "deepseek"
base_url = "https://example.test"
api_key_env = "FIXTURE_KEY"

[providers.hosted]
kind = "openai-chat"
api_flavor = "dashscope"
base_url = "https://hosted.example.test/compatible-mode/v1"
api_key_env = "HOSTED_FIXTURE_KEY"

[models.fixture]
provider = "example"
model = "deepseek-v4-pro"
display_name = "Fixture"
supports_tools = true

[models.fixture.thinking]
mode = "disabled"

[models.fixture.thinking_capabilities]
can_disable = true
supports_history_control = false

[models.hosted]
provider = "hosted"
model = "deepseek-v4-pro"
display_name = "Hosted fixture"
supports_tools = true

[models.hosted.thinking]
mode = "disabled"

[models.hosted.thinking_capabilities]
can_disable = true
supports_history_control = false
"#,
        )
        .unwrap();
        assert_eq!(patch.providers["example"].api_flavor, ApiFlavor::Deepseek);
        assert_eq!(patch.providers["hosted"].api_flavor, ApiFlavor::Dashscope);
        assert_eq!(patch.models["fixture"].model, patch.models["hosted"].model);
        assert_eq!(
            patch.models["fixture"].thinking.mode,
            ThinkingMode::Disabled
        );
        assert_eq!(patch.models["hosted"].thinking.mode, ThinkingMode::Disabled);
        for id in ["fixture", "hosted"] {
            let model = &patch.models[id];

            assert_eq!(model.thinking_preset, None);
            assert_eq!(model.thinking_capabilities.can_enable, None);
            assert_eq!(model.thinking_capabilities.can_disable, Some(true),);
            assert_eq!(
                model.thinking_capabilities.supports_history_control,
                Some(false),
            );
        }
    }

    #[test]
    fn omitted_options_use_defaults_and_replacement_does_not_inherit() {
        let patch: UserConfigPatch = toml::from_str(
            r#"
[providers.qwen]
kind = "openai-chat"
base_url = "https://example.test"
api_key_env = "FIXTURE_KEY"

[models.qwen-max]
provider = "qwen"
model = "fixture-model"
display_name = "Fixture"
supports_tools = true
"#,
        )
        .unwrap();
        let mut config = AppConfig::default();
        config.apply_user(patch);
        assert_eq!(config.providers["qwen"].api_flavor, ApiFlavor::Generic);
        assert_eq!(
            config.models["qwen-max"].thinking.mode,
            ThinkingMode::ProviderDefault
        );

        let model = &config.models["qwen-max"];
        assert_eq!(model.thinking_preset, None);
        assert_eq!(
            model.thinking_capabilities,
            ThinkingCapabilitiesOverride::default()
        );
        assert_eq!(model.thinking_capabilities.can_enable, None);
        assert_eq!(model.thinking_capabilities.can_disable, None);
        assert_eq!(model.thinking_capabilities.supports_history_control, None,);
    }

    #[test]
    fn rejects_old_fields_and_unknown_options() {
        let model = r#"
[models.fixture]
provider = "qwen"
model = "fixture-model"
display_name = "Fixture"
supports_tools = true
"#;
        for value in ["plain", "qwen-stage-a"] {
            let text = format!("{model}\nrequest_options = \"{value}\"\n");
            assert!(toml::from_str::<UserConfigPatch>(&text).is_err());
        }
        for extra in [
            "[models.fixture.thinking]\nmode = \"unknown\"",
            "[models.fixture.thinking]\nmode = \"disabled\"\nunknown = true",
            "[models.fixture.thinking_capabilities]\ncan_disable = \"true\"",
            "[models.fixture.thinking_capabilities]\ncan_disable = true\nunknown = true",
        ] {
            assert!(toml::from_str::<UserConfigPatch>(&format!("{model}\n{extra}")).is_err());
        }
        let provider = r#"
[providers.fixture]
kind = "openai-chat"
base_url = "https://example.test"
api_key_env = "FIXTURE_KEY"
"#;
        for flavor in ["unknown", "qwen"] {
            assert!(
                toml::from_str::<UserConfigPatch>(&format!(
                    "{provider}\napi_flavor = \"{flavor}\""
                ),)
                .is_err()
            );
        }
    }

    #[test]
    fn parses_enabled_mode_without_declaring_capabilities() {
        let patch: UserConfigPatch = toml::from_str(
            r#"
[models.fixture]
provider = "qwen"
model = "fixture-model"
display_name = "Fixture"
supports_tools = true

[models.fixture.thinking]
mode = "enabled"
"#,
        )
        .unwrap();

        let model = &patch.models["fixture"];

        assert_eq!(model.thinking.mode, ThinkingMode::Enabled);
        assert_eq!(model.thinking_preset, None);
        assert_eq!(
            model.thinking_capabilities,
            ThinkingCapabilitiesOverride::default(),
        );
    }

    #[test]
    fn project_and_cli_cannot_override_thinking_definitions() {
        for text in [
            "[providers.qwen]\napi_flavor = \"dashscope\"",
            "[models.qwen-max.thinking]\nmode = \"disabled\"",
            "[models.qwen-max.thinking_capabilities]\ncan_disable = true",
        ] {
            assert!(toml::from_str::<ProjectConfigPatch>(text).is_err());
        }
        let mut config = AppConfig::default();
        for value in [
            "providers.qwen.api_flavor=\"dashscope\"",
            "models.qwen-max.thinking.mode=\"disabled\"",
            "models.qwen-max.thinking_capabilities.can_disable=true",
        ] {
            assert!(config.apply_override(value).is_err());
        }
    }

    #[test]
    fn validates_preset_id_shape_but_defers_preset_existence_to_factory() {
        for id in [
            String::new(),
            "x".repeat(65),
            "preset\nname".into(),
            "preset\0name".into(),
        ] {
            let mut config = AppConfig::default();
            config.models.get_mut("qwen-max").unwrap().thinking_preset = Some(id);
            assert!(matches!(
                config.validate_definitions(),
                Err(ConfigError::Invalid("思考预设 ID 不合法"))
            ));
        }
        let mut config = AppConfig::default();
        config.models.get_mut("qwen-max").unwrap().thinking_preset = Some("unknown-preset".into());
        config.validate_definitions().unwrap();
    }

    #[test]
    fn parses_explicit_preset_and_boolean_capability_overrides() {
        let model: ModelConfig = toml::from_str(
            r#"
provider = "qwen"
model = "fixture-model"
display_name = "Fixture"
supports_tools = true
thinking_preset = "dashscope-qwen-display-only"
[thinking]
mode = "enabled"
[thinking_capabilities]
can_enable = false
can_disable = true
supports_history_control = false
"#,
        )
        .unwrap();
        assert_eq!(
            model.thinking_preset.as_deref(),
            Some("dashscope-qwen-display-only")
        );
        assert_eq!(model.thinking.mode, ThinkingMode::Enabled);
        assert_eq!(
            model.thinking_capabilities,
            ThinkingCapabilitiesOverride {
                can_enable: Some(false),
                can_disable: Some(true),
                supports_history_control: Some(false),
            }
        );
    }

    #[test]
    fn rejects_invalid_capability_types_and_user_supplied_history_policy() {
        let base = "provider = \"qwen\"\nmodel = \"fixture-model\"\ndisplay_name = \"Fixture\"\nsupports_tools = true\n";
        for field in ["can_enable", "can_disable", "supports_history_control"] {
            for value in ["\"true\"", "1", "[]"] {
                assert!(
                    toml::from_str::<ModelConfig>(&format!(
                        "{base}[thinking_capabilities]\n{field} = {value}"
                    ))
                    .is_err()
                );
            }
        }
        for extra in [
            "history_policy = \"display-only-with-history-disabled\"",
            "[thinking_capabilities]\nhistory_policy = \"display-only-with-history-disabled\"",
            "[thinking_capabilities]\nenable_thinking = true",
        ] {
            assert!(toml::from_str::<ModelConfig>(&format!("{base}{extra}")).is_err());
        }
    }

    #[test]
    fn project_and_cli_cannot_supply_presets_capabilities_or_history_contracts() {
        for text in [
            "[models.qwen-max]\nthinking_preset = \"dashscope-qwen-display-only\"",
            "[models.qwen-max.thinking_capabilities]\ncan_enable = true",
            "[models.qwen-max]\nhistory_policy = \"display-only-with-history-disabled\"",
            "thinking_preset = \"dashscope-qwen-display-only\"",
        ] {
            assert!(toml::from_str::<ProjectConfigPatch>(text).is_err());
        }
        let mut config = AppConfig::default();
        for value in [
            "models.qwen-max.thinking_preset=\"dashscope-qwen-display-only\"",
            "models.qwen-max.thinking_capabilities.can_enable=true",
            "models.qwen-max.history_policy=\"display-only-with-history-disabled\"",
        ] {
            assert!(config.apply_override(value).is_err());
        }
        assert_eq!(
            config.models["qwen-max"].thinking.mode,
            ThinkingMode::Disabled
        );
    }

    #[test]
    fn tool_limits_merge_by_name_and_reject_zero() {
        let mut config = AppConfig::default();
        let patch: UserConfigPatch =
            toml::from_str("[tools.call_limits]\nlist_directory = 80\n").unwrap();
        config.apply_user(patch);
        config.validate_definitions().unwrap();
        assert_eq!(config.tools.call_limits["web_search"], 4);
        assert_eq!(config.tools.call_limits["list_directory"], 80);
        config.tools.call_limits.insert("web_search".into(), 0);
        assert!(config.validate_definitions().is_err());
    }

    #[test]
    fn tool_limits_are_not_project_or_cli_fields() {
        assert!(
            toml::from_str::<ProjectConfigPatch>("[tools.call_limits]\nweb_search = 100\n",)
                .is_err()
        );
        assert!(
            AppConfig::default()
                .apply_override("tools.call_limits.web_search=100")
                .is_err()
        );
        for value in ["-1", "4294967296", "\"4\""] {
            let text = format!("[tools.call_limits]\nweb_search = {value}\n");
            assert!(toml::from_str::<UserConfigPatch>(&text).is_err());
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub call_limits: BTreeMap<String, u32>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            call_limits: ToolCallLimits::default().overrides().clone(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolsPatch {
    pub call_limits: BTreeMap<String, u32>,
}
