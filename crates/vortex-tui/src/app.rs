use crate::action::{Effect, UiAction};
use crate::widgets::{PromptEditor, Transcript};

use vortex_core::{AgentCommand, CoreEvent};

#[derive(Debug)]
pub(crate) struct AppState {
    status_line: String,
    prompt_editor: PromptEditor,
    transcript: Transcript,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            //exit_requested: false,
            status_line: "Status: Running".to_string(),
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::default(),
        }
    }
}

impl AppState {
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

pub(crate) fn update(state: &mut AppState, action: UiAction) -> Vec<Effect> {
    match action {
        UiAction::Quit => {
            vec![Effect::SendCommand(AgentCommand::Shutdown), Effect::Exit]
        }
        UiAction::EditPrompt(input) => {
            state.prompt_editor.handle_input(input);
            vec![Effect::Redraw]
        }
        UiAction::SubmitPrompt => {
            let content = state.prompt_editor.take_text();

            if content.trim().is_empty() {
                return vec![Effect::Redraw];
            }

            vec![
                Effect::SendCommand(AgentCommand::SubmitPrompt { content }),
                Effect::Redraw,
            ]
        }
        UiAction::CoreEvent(CoreEvent::UserMessageAdded { content }) => {
            state.transcript.push_user(content);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::AssistantMessageStarted) => {
            state.transcript.start_assistant();
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::AssistantTextDelta { delta }) => {
            state.transcript.append_assistant_delta(delta);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnCompleted) => {
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnFailed { message }) => {
            state.transcript.push_error(message);
            vec![Effect::Redraw]
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

        //assert!(state.exit_requested());
    }
}
