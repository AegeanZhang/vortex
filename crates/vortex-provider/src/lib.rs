mod openai_chat;
mod sse_transport;

#[cfg(debug_assertions)]
use vortex_diagnostics::wire_trace;

pub use openai_chat::{
    OpenAiChatDialect, OpenAiChatOptions, OpenAiChatProvider, ReasoningHistoryPolicy,
    RequestOptionsError, ResolvedThinkingProfile, resolve_thinking_profile,
};
