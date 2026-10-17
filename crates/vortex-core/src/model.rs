use std::{collections::BTreeSet, fmt, sync::Arc};

use crate::ModelProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider_id: String,
    pub endpoint_host: String,
    pub supports_tools: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectionError {
    pub model_id: String,
    pub message: String,
}

impl fmt::Display for ModelSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "模型 {}: {}", self.model_id, self.message)
    }
}
impl std::error::Error for ModelSelectionError {}

pub trait ModelFactory: Send + Sync {
    fn catalog(&self) -> Vec<ModelInfo>;
    // 同步构建，仅做本地装配和凭据读取，禁止执行网络请求。
    fn create(&self, model_id: &str) -> Result<Arc<dyn ModelProvider>, ModelSelectionError>;
}

pub struct ModelManager {
    factory: Arc<dyn ModelFactory>,
    catalog: Vec<ModelInfo>,
    current: ModelInfo,
    provider: Arc<dyn ModelProvider>,
}

impl ModelManager {
    pub fn new(factory: Arc<dyn ModelFactory>, initial: &str) -> Result<Self, ModelSelectionError> {
        let catalog = factory.catalog();
        let mut ids = BTreeSet::new();
        if catalog.is_empty() || catalog.iter().any(|info| !ids.insert(info.id.clone())) {
            return Err(ModelSelectionError {
                model_id: initial.into(),
                message: "模型列表为空或 ID 重复".into(),
            });
        }
        let current = Self::find_selectable(&catalog, initial)?;
        let provider = factory.create(initial)?;
        Ok(Self {
            factory,
            catalog,
            current,
            provider,
        })
    }

    fn find_selectable(catalog: &[ModelInfo], id: &str) -> Result<ModelInfo, ModelSelectionError> {
        let info =
            catalog
                .iter()
                .find(|info| info.id == id)
                .ok_or_else(|| ModelSelectionError {
                    model_id: id.into(),
                    message: "不存在这个模型 profile".into(),
                })?;
        if let Some(message) = &info.unavailable_reason {
            return Err(ModelSelectionError {
                model_id: id.into(),
                message: message.clone(),
            });
        }
        Ok(info.clone())
    }

    pub fn current(&self) -> &ModelInfo {
        &self.current
    }
    pub fn catalog(&self) -> &[ModelInfo] {
        &self.catalog
    }
    pub fn provider(&self) -> Arc<dyn ModelProvider> {
        Arc::clone(&self.provider)
    }

    pub fn select(&mut self, id: &str) -> Result<ModelInfo, ModelSelectionError> {
        let next = Self::find_selectable(&self.catalog, id)?;
        if next.id == self.current.id {
            // 幂等成功，仍需要 Session 发确认事件。
            return Ok(self.current.clone());
        }
        let provider = self.factory.create(id)?;
        // 构建成功后才一起替换；失败不会损坏旧绑定。
        self.current = next;
        self.provider = provider;
        Ok(self.current.clone())
    }
}
