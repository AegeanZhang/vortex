use crate::widgets::PromptInput;
use vortex_core::{AgentCommand, CoreEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiAction {
    Quit,
    EditPrompt(PromptInput),
    SubmitPrompt,
    CoreEvent(CoreEvent),
}

#[derive(Debug)]
pub(crate) enum Effect {
    SendCommand(AgentCommand),
    Redraw,
    Exit,
}
