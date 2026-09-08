use crate::widgets::PromptInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiAction {
    Quit,
    EditPrompt(PromptInput),
    SubmitPrompt,
}
