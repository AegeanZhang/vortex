use crate::widgets::PromptInput;
use vortex_core::{AgentCommand, CoreEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiAction {
    Quit,
    EditPrompt(PromptInput),
    SubmitPrompt,
    CoreEvent(CoreEvent),
    ScrollTranscript(ScrollCommand),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    SendCommand(AgentCommand),
    Redraw,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollCommand {
    PageUp,
    PageDown,
    ToTop,
    ToBottom,
}
