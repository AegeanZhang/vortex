use crate::action::{Effect, UiAction};
use crate::widgets::{PromptEditor, Transcript};

use vortex_core::{AgentCommand, CoreEvent, SessionSnapshot, SessionStatus};

#[derive(Debug)]
pub(crate) struct AppState {
    status: SessionStatus,
    notice: Option<String>,
    prompt_editor: PromptEditor,
    transcript: Transcript,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: SessionStatus::Idle,
            notice: None,
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::default(),
        }
    }
}

impl From<SessionSnapshot> for AppState {
    fn from(snapshot: SessionSnapshot) -> Self {
        Self {
            status: snapshot.status,
            notice: None,
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::from_messages(snapshot.messages),
        }
    }
}

impl AppState {
    pub(crate) fn status_line(&self) -> String {
        let status = match self.status {
            SessionStatus::Idle => "Status: Idle",
            SessionStatus::Running => "Status: Running",
            SessionStatus::Failed => "Status: Failed",
        };

        match &self.notice {
            Some(notice) => format!("{status} | {notice}"),
            None => status.to_string(),
        }
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
            state.notice = None;
            state.prompt_editor.handle_input(input);
            vec![Effect::Redraw]
        }
        UiAction::SubmitPrompt => {
            if state.status == SessionStatus::Running {
                state.notice = Some("a turn is already running".to_string());
                return vec![Effect::Redraw];
            }

            let content = state.prompt_editor.take_text();

            if content.trim().is_empty() {
                return vec![Effect::Redraw];
            }

            state.notice = None;

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
            state.status = SessionStatus::Running;
            state.notice = None;
            state.transcript.start_assistant();
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::AssistantTextDelta { delta }) => {
            state.transcript.append_assistant_delta(delta);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnCompleted) => {
            state.status = SessionStatus::Idle;
            state.notice = None;
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnFailed { message }) => {
            state.status = SessionStatus::Failed;
            state.notice = None;
            state.transcript.fail_assistant(message);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::CommandRejected { message }) => {
            state.notice = Some(message);
            vec![Effect::Redraw]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_requests_shutdown_then_exit() {
        let mut state = AppState::default();

        let effects = update(&mut state, UiAction::Quit);

        assert_eq!(
            effects,
            vec![Effect::SendCommand(AgentCommand::Shutdown), Effect::Exit,]
        );
    }

    #[test]
    fn turn_completed_changes_status_to_idle() {
        let mut state = AppState::default();
        state.status = SessionStatus::Running;

        let effects = update(&mut state, UiAction::CoreEvent(CoreEvent::TurnCompleted));

        assert_eq!(state.status, SessionStatus::Idle);
        assert_eq!(effects, vec![Effect::Redraw]);
    }

    #[test]
    fn command_rejection_keeps_current_status() {
        let mut state = AppState::default();
        state.status = SessionStatus::Running;

        let effects = update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::CommandRejected {
                message: "a turn is already running".to_string(),
            }),
        );

        assert_eq!(state.status, SessionStatus::Running);
        assert_eq!(effects, vec![Effect::Redraw]);
    }
}
