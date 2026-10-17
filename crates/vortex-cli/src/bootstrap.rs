use std::{error::Error, sync::Arc, time::Duration};

use vortex_core::{
    Tool, ToolRegistry,
    config::{AppConfig, SearchBackend},
};
use vortex_tools::{MetasoSearchOptions, MetasoWebSearch, MockWebSearch};

use crate::model_factory::{CredentialError, read_api_key};

fn build_tools(config: &AppConfig) -> Result<ToolRegistry, Box<dyn Error + Send + Sync>> {
    build_tools_with_key_reader(config, read_api_key)
}

fn build_tools_with_key_reader(
    config: &AppConfig,
    read_key: impl Fn(&str) -> Result<String, CredentialError>,
) -> Result<ToolRegistry, Box<dyn Error + Send + Sync>> {
    if !config.search.enabled {
        return Ok(ToolRegistry::empty());
    }
    let tool: Arc<dyn Tool> = match config.search.backend {
        SearchBackend::Mock => Arc::new(MockWebSearch::new()),
        SearchBackend::Metaso => Arc::new(MetasoWebSearch::new_with_options(
            read_key(&config.search.api_key_env)?,
            MetasoSearchOptions {
                max_results: config.search.max_results,
                connect_timeout: Duration::from_secs(config.search.connect_timeout_secs),
                request_timeout: Duration::from_secs(config.search.request_timeout_secs),
            },
        )?),
    };
    Ok(ToolRegistry::new(vec![tool])?)
}

pub(crate) async fn start_tui_session(
    paths: &crate::paths::AppPaths,
    overrides: Vec<String>,
    model_override: Option<String>,
) -> Result<vortex_core::SessionConnection, Box<dyn Error + Send + Sync>> {
    use crate::model_factory::ConfiguredModelFactory;
    use std::sync::Arc;
    use vortex_core::config::{ConfigPaths, load_config};
    use vortex_core::{ModelManager, start_session};

    let config_paths = ConfigPaths {
        user: paths.user_config.clone(),
        project: paths.project_config.clone(),
    };
    // 普通文件读取只发生在启动前，并从异步 executor 移到阻塞池。
    let loaded = tokio::task::spawn_blocking(move || {
        load_config(&config_paths, &overrides, model_override.as_deref())
    })
    .await??;
    report_config_sources(paths, &loaded.sources);
    let config = loaded.config;

    let tools = build_tools(&config)?;
    let initial = config.default_model.clone();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| std::io::Error::other("模型 HTTP 客户端初始化失败"))?;
    let factory = Arc::new(ConfiguredModelFactory::new(config, client)?);
    let models = ModelManager::new(factory, &initial)?;
    let selected = models.current();
    eprintln!(
        "模型: {} | Provider: {} | 目标: {}",
        selected.id, selected.provider_id, selected.endpoint_host
    );
    tracing::info!(
        model_id = %selected.id, provider_id = %selected.provider_id,
        endpoint_host = %selected.endpoint_host, "TUI session configured",
    );
    Ok(start_session(models, tools))
}

fn report_config_sources(
    paths: &crate::paths::AppPaths,
    sources: &vortex_core::config::ConfigSources,
) {
    use vortex_core::config::ConfigFileStatus;
    // Debug 格式转义路径中的终端控制字符；仅作显示，不转回 I/O 路径。
    eprintln!("工作目录: {:?}", paths.cwd.as_os_str());
    eprintln!("Vortex HOME: {:?}", paths.home.as_os_str());
    eprintln!(
        "用户配置: {:?} ({})",
        sources.user.path.as_os_str(),
        sources.user.status.as_str()
    );
    eprintln!(
        "项目配置: {:?} ({})",
        sources.project.path.as_os_str(),
        sources.project.status.as_str()
    );
    if sources.project.status == ConfigFileStatus::Missing {
        eprintln!("未加载项目配置；只检查启动目录，不向父目录查找");
    }
    tracing::info!(
        cwd = %paths.cwd.display(), vortex_home = %paths.home.display(),
        user_config = %sources.user.path.display(), user_config_status = sources.user.status.as_str(),
        project_config = %sources.project.path.display(), project_config_status = sources.project.status.as_str(),
        "configuration sources resolved",
    );
}
