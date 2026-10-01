use crate::{ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationItem {
    Message(SessionMessage),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
