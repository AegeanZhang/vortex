use crate::widgets::PromptInput;
use vortex_core::{AgentCommand, CoreEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiAction {
    Quit,
    SelectText(crossterm::event::MouseEvent),
    CopySelection,
    ClipboardResult(Result<(), String>),
    EditPrompt(PromptInput),
    SubmitPrompt,
    CoreEvent(CoreEvent),
    ScrollTranscript(ScrollCommand),
    RedrawRequested,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    SendCommand(AgentCommand),
    CopyText(String),
    Redraw,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollCommand {
    PageUp,
    PageDown,
    ToTop,
    ToBottom,
    LinesUp(usize),
    LinesDown(usize),
}
