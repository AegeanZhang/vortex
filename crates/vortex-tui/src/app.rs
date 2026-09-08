use crate::action::UiAction;
use crate::widgets::PromptEditor;

#[derive(Debug)]
pub(crate) struct AppState {
    exit_requested: bool,
    status_line: String,
    prompt_editor: PromptEditor,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            exit_requested: false,
            status_line: "Status: Running".to_string(),
            prompt_editor: PromptEditor::default(),
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
}

pub(crate) fn update(state: &mut AppState, action: UiAction) {
    match action {
        UiAction::Quit => {
            state.exit_requested = true;
        }
        UiAction::EditPrompt(input) => {
            state.prompt_editor.handle_input(input);
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
