mod agent_loop;
mod conversation;
mod model;
mod provider;
mod session;
mod tool;

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
    SessionSnapshot, SessionStatus, start_session,
};
pub use tool::{Tool, ToolCall, ToolError, ToolFuture, ToolRegistry, ToolResult, ToolSpec};
