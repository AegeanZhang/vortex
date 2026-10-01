use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    pin::Pin,
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    DuplicateTool(String),
    UnknownTool(String),
    InvalidArguments(String),
    Execution(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTool(name) => write!(formatter, "duplicate tool: {name}"),
            Self::UnknownTool(name) => write!(formatter, "unknown tool: {name}"),
            Self::InvalidArguments(message) => {
                write!(formatter, "invalid tool arguments: {message}")
            }
            Self::Execution(message) => write!(formatter, "tool execution failed: {message}"),
        }
    }
}

impl Error for ToolError {}

pub type ToolFuture<'a> = 
    Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + 'a>>;

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, arguments: serde_json::Value) -> ToolFuture<'_>;
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    // Rust 的Smart Constructor 模式（聪明构造器）：在构造时做验证，
    //     确保一旦对象被创建，其内部状态就是有效的。
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Result<Self, ToolError> {
        let mut names = BTreeSet::new();

        for tool in &tools {
            let name = tool.spec().name;

            // 用 BTreeSet 的 insert 返回值来做"一次性的重复检测和添加
            if !names.insert(name.clone()) {
                return Err(ToolError::DuplicateTool(name));
            }
        }

        Ok(Self { tools })
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let tool = self
            .tools.iter()
            .find(|tool| tool.spec().name == call.name);

        let outcome = match tool {
            Some(tool) => tool.execute(call.arguments.clone()).await,
            None => Err(ToolError::UnknownTool(call.name.clone())),
        };

        match outcome {
            Ok(output) => ToolResult {
                call_id: call.call_id.clone(),
                output,
                is_error: false,
            },
            Err(error) => ToolResult {
                call_id: call.call_id.clone(),
                output: error.to_string(),
                is_error: true,
            },
        }
    }
}
