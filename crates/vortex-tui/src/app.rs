use crate::action::UiAction;
use crate::widgets::{PromptEditor, Transcript};

#[derive(Debug)]
pub(crate) struct AppState {
    exit_requested: bool,
    status_line: String,
    prompt_editor: PromptEditor,
    transcript: Transcript,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            exit_requested: false,
            status_line: "Status: Running".to_string(),
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::default(),
        }
    }
}

impl AppState {
    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(crate) fn status_line(&self) -> &str {
        &self.status_line
    }

    pub(crate) fn prompt_editor(&self) -> &PromptEditor {
        &self.prompt_editor
    }

    pub(crate) fn transcript(&self) -> &Transcript {
        &self.transcript
    }
}

pub(crate) fn update(state: &mut AppState, action: UiAction) {
    match action {
        UiAction::Quit => {
            state.exit_requested = true;
        }
        UiAction::EditPrompt(input) => {
            state.prompt_editor.handle_input(input);
        }
        UiAction::SubmitPrompt => {
            let content = state.prompt_editor.take_text();

            if !content.trim().is_empty() {
                state.transcript.push_user(content);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_action_requests_exit() {
        let mut state = AppState::default();

        update(&mut state, UiAction::Quit);

        assert!(state.exit_requested());
    }
}
