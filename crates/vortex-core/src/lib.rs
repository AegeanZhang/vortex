mod agent_loop;
mod conversation;
mod model;
mod provider;
mod session;
mod tool;
mod tool_budget;

pub mod config;

pub use conversation::{ConversationItem, MessageRole, SessionMessage};
pub use model::{ModelFactory, ModelInfo, ModelManager, ModelSelectionError};
pub use provider::{
    FinishReason, ModelEvent, ModelProvider, ModelRequest, ModelStream, ProviderError,
    ReasoningOutputPolicy, ThinkingCapabilities, ThinkingCapabilitiesOverride, ThinkingMode,
    ThinkingOptions,
};
pub use session::{
    AgentCommand, AgentHandle, CoreEvent, CoreEventStream, SessionConnection, SessionError,
    SessionSnapshot, SessionStatus, start_session, start_session_with_tool_limits,
};
pub use tool::{Tool, ToolCall, ToolError, ToolFuture, ToolRegistry, ToolResult, ToolSpec};
pub use tool_budget::{
    DEFAULT_TOOL_CALL_LIMIT_PER_TURN, ToolBudgetExceeded, ToolCallLimits, ToolCallLimitsError,
};
