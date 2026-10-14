use crate::{
    action::{Effect, ScrollCommand, UiAction},
    command::{CommandParse, SlashCommand, parse_command},
    widgets::{CommandMenu, MenuKey, PromptEditor, PromptInput, Transcript},
};

use vortex_core::{AgentCommand, CoreEvent, SessionSnapshot, SessionStatus};

#[derive(Debug)]
pub(crate) struct AppState {
    status: SessionStatus,
    notice: Option<String>,
    prompt_editor: PromptEditor,
    transcript: Transcript,
    command_menu: CommandMenu,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: SessionStatus::Idle,
            notice: None,
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::default(),
            command_menu: CommandMenu::default(),
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
            command_menu: CommandMenu::default(),
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

    pub(crate) fn transcript_mut(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    pub(crate) fn command_menu(&self) -> &CommandMenu {
        &self.command_menu
    }
}

// 用户按下tab键时，补全命令
fn complete_selected_command(state: &mut AppState) -> Vec<Effect> {
    if let Some(spec) = state.command_menu.selected_command() {
        let text = format!("/{}", spec.name);
        state.prompt_editor.replace_text(&text);
        state.command_menu.sync(&text);
        state.command_menu.dismiss();
        state.notice = None;
    } else {
        state.notice = Some("没有可补全的命令".to_string());
    }
    vec![Effect::Redraw]
}

fn edit_prompt(state: &mut AppState, input: PromptInput) -> Vec<Effect> {
    if state.command_menu.is_open() {
        match input.menu_key() {
            Some(MenuKey::Up) => {
                state.command_menu.previous();
                return vec![Effect::Redraw];
            }
            Some(MenuKey::Down) => {
                state.command_menu.next();
                return vec![Effect::Redraw];
            }
            Some(MenuKey::Tab) => return complete_selected_command(state),
            Some(MenuKey::Escape) => {
                state.command_menu.dismiss();
                return vec![Effect::Redraw];
            }
            None => {}
        }
    }

    let before = state.prompt_editor.text();
    state.prompt_editor.handle_input(input);
    let after = state.prompt_editor.text();
    if before != after {
        state.notice = None;
        state.command_menu.sync(&after);
    }
    vec![Effect::Redraw]
}

fn submit_prompt(state: &mut AppState) -> Vec<Effect> {
    // 先读后判，不以 take_text 为解析或状态检查的前提
    let text = state.prompt_editor.text();
    match parse_command(&text) {
        CommandParse::Ready(SlashCommand::Exit) => {
            // 同步路由到现有语义 Action; Quit 最终分支，不会回到提交形成递归.
            return update(state, UiAction::Quit);
        }
        CommandParse::Incomplete => {
            if !state.command_menu.is_open() {
                state.command_menu.sync(&text);
            }
            return complete_selected_command(state);
        }
        CommandParse::Invalid(message) => {
            state.notice = Some(message.to_string());
            return vec![Effect::Redraw];
        }
        CommandParse::NotCommand => {}
    }

    if text.trim().is_empty() {
        return vec![Effect::Redraw];
    }
    if state.status == SessionStatus::Running {
        state.notice = Some("a turn is already running".to_string());
        return vec![Effect::Redraw];
    }

    let content = state.prompt_editor.take_text();
    state.command_menu.sync("");
    state.notice = None;
    vec![
        Effect::SendCommand(AgentCommand::SubmitPrompt { content }),
        Effect::Redraw,
    ]
}

pub(crate) fn update(state: &mut AppState, action: UiAction) -> Vec<Effect> {
    match action {
        UiAction::Quit => {
            vec![Effect::SendCommand(AgentCommand::Shutdown), Effect::Exit]
        }
        UiAction::EditPrompt(input) => edit_prompt(state, input),
        UiAction::SubmitPrompt => submit_prompt(state),
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
        UiAction::CoreEvent(CoreEvent::ToolCallStarted { call }) => {
            state.notice = Some(format!("Running tool: {}", call.name));
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ToolCallFinished { name, result }) => {
            state.notice = Some(if result.is_error {
                format!("Tool failed: {name}; waiting for model")
            } else {
                format!("Tool completed: {name}; waiting for model")
            });
            vec![Effect::Redraw]
        }
        UiAction::ScrollTranscript(command) => {
            match command {
                ScrollCommand::PageUp => state.transcript.page_up(),
                ScrollCommand::PageDown => state.transcript.page_down(),
                ScrollCommand::ToTop => state.transcript.scroll_to_top(),
                ScrollCommand::ToBottom => state.transcript.scroll_to_bottom(),
                ScrollCommand::LinesUp(lines) => state.transcript.scroll_up(lines),
                ScrollCommand::LinesDown(lines) => state.transcript.scroll_down(lines),
            }

            vec![Effect::Redraw]
        }
        UiAction::RedrawRequested => vec![Effect::Redraw],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn state_with_draft(text: &str, status: SessionStatus) -> AppState {
        let mut state = AppState {
            status,
            ..Default::default()
        };
        state.prompt_editor.replace_text(text);
        state.command_menu.sync(text);
        state
    }

    fn plain_key(code: KeyCode) -> UiAction {
        UiAction::EditPrompt(KeyEvent::new(code, KeyModifiers::NONE).into())
    }

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
        let mut state = AppState {
            status: SessionStatus::Running,
            ..Default::default()
        };

        let effects = update(&mut state, UiAction::CoreEvent(CoreEvent::TurnCompleted));

        assert_eq!(state.status, SessionStatus::Idle);
        assert_eq!(effects, vec![Effect::Redraw]);
    }

    #[test]
    fn command_rejection_keeps_current_status() {
        let mut state = AppState {
            status: SessionStatus::Running,
            ..Default::default()
        };

        let effects = update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::CommandRejected {
                message: "a turn is already running".to_string(),
            }),
        );

        assert_eq!(state.status, SessionStatus::Running);
        assert_eq!(effects, vec![Effect::Redraw]);
    }

    #[test]
    fn incomplete_enter_completes_before_exit() {
        for text in ["/", "/e", "/ex", "/exi"] {
            let mut state = state_with_draft(text, SessionStatus::Idle);
            assert_eq!(
                update(&mut state, UiAction::SubmitPrompt),
                vec![Effect::Redraw]
            );
            assert_eq!(state.prompt_editor.text(), "/exit");
            assert!(!state.command_menu.is_open());
            assert_eq!(
                update(&mut state, UiAction::SubmitPrompt),
                vec![Effect::SendCommand(AgentCommand::Shutdown), Effect::Exit]
            );
        }
    }

    #[test]
    fn tab_completes_without_exiting() {
        let mut state = state_with_draft("/ex", SessionStatus::Idle);
        assert_eq!(
            update(&mut state, plain_key(KeyCode::Tab)),
            vec![Effect::Redraw]
        );
        assert_eq!(state.prompt_editor.text(), "/exit");
        assert!(!state.command_menu.is_open());
        assert!(state.notice.is_none());
    }

    #[test]
    fn exit_requests_shutdown_before_exit_in_every_session_state() {
        for status in [
            SessionStatus::Idle,
            SessionStatus::Running,
            SessionStatus::Failed,
        ] {
            for text in ["/exit", "/exit ", "/exit\t"] {
                let mut state = state_with_draft(text, status);
                assert_eq!(
                    update(&mut state, UiAction::SubmitPrompt),
                    vec![Effect::SendCommand(AgentCommand::Shutdown), Effect::Exit]
                );
                assert_eq!(state.status, status);
            }
        }
    }

    #[test]
    fn invalid_commands_keep_draft_and_never_send_a_prompt() {
        for text in ["/exit now", "/unknown", "/exit\n", "/exit\r\n", "/ex "] {
            let mut state = state_with_draft(text, SessionStatus::Idle);
            // TextArea 插入时会将 CRLF 规范化；比较提交前后的实际草稿。
            let before = state.prompt_editor.text();
            assert_eq!(
                update(&mut state, UiAction::SubmitPrompt),
                vec![Effect::Redraw]
            );
            assert_eq!(state.prompt_editor.text(), before);
            assert_eq!(state.status, SessionStatus::Idle);
            assert!(state.notice.is_some());
        }
    }

    #[test]
    fn running_rejects_normal_prompt_without_losing_it() {
        let mut state = state_with_draft("继续搜索", SessionStatus::Running);
        assert_eq!(
            update(&mut state, UiAction::SubmitPrompt),
            vec![Effect::Redraw]
        );
        assert_eq!(state.prompt_editor.text(), "继续搜索");
        assert_eq!(state.status, SessionStatus::Running);
        assert!(state.notice.is_some());
    }

    #[test]
    fn ordinary_prompt_preserves_original_text_and_clears_editor_after_submission() {
        for text in [" /exit", "说明 /exit", "  hello  "] {
            let mut state = state_with_draft(text, SessionStatus::Idle);
            assert_eq!(
                update(&mut state, UiAction::SubmitPrompt),
                vec![
                    Effect::SendCommand(AgentCommand::SubmitPrompt {
                        content: text.to_string()
                    }),
                    Effect::Redraw,
                ]
            );
            assert_eq!(state.prompt_editor.text(), "");
            assert!(!state.command_menu.is_open());
        }
    }

    #[test]
    fn empty_input_never_sends_a_command() {
        for status in [
            SessionStatus::Idle,
            SessionStatus::Running,
            SessionStatus::Failed,
        ] {
            for text in ["", "  ", "\t\n"] {
                let mut state = state_with_draft(text, status);
                assert_eq!(
                    update(&mut state, UiAction::SubmitPrompt),
                    vec![Effect::Redraw]
                );
                assert_eq!(state.prompt_editor.text(), text);
                assert_eq!(state.status, status);
            }
        }
    }

    #[test]
    fn escape_stays_closed_across_core_events_and_cursor_movement() {
        let mut state = state_with_draft("/ex", SessionStatus::Running);
        assert_eq!(
            update(&mut state, plain_key(KeyCode::Esc)),
            vec![Effect::Redraw]
        );
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantTextDelta {
                delta: "answer".to_string(),
            }),
        );
        update(&mut state, UiAction::CoreEvent(CoreEvent::TurnCompleted));
        update(&mut state, UiAction::RedrawRequested);
        update(&mut state, plain_key(KeyCode::Left));
        update(&mut state, plain_key(KeyCode::Right));
        assert_eq!(state.prompt_editor.text(), "/ex");
        assert!(!state.command_menu.is_open());

        update(&mut state, plain_key(KeyCode::Char('i')));
        assert_eq!(state.prompt_editor.text(), "/exi");
        assert!(state.command_menu.is_open());
    }

    #[test]
    fn menu_navigation_does_not_change_draft_or_status() {
        let mut state = state_with_draft("/ex", SessionStatus::Running);
        for code in [KeyCode::Up, KeyCode::Down] {
            assert_eq!(update(&mut state, plain_key(code)), vec![Effect::Redraw]);
            assert_eq!(state.prompt_editor.text(), "/ex");
            assert_eq!(state.command_menu.selected_command().unwrap().name, "exit");
            assert_eq!(state.status, SessionStatus::Running);
        }
    }

    #[test]
    fn removing_leading_slash_closes_menu() {
        let mut state = state_with_draft("/ex", SessionStatus::Idle);
        update(&mut state, plain_key(KeyCode::Home));
        update(&mut state, plain_key(KeyCode::Delete));
        assert_eq!(state.prompt_editor.text(), "ex");
        assert!(!state.command_menu.is_open());
    }

    #[test]
    fn tab_with_no_matches_keeps_draft_and_reports_no_completion() {
        let mut state = state_with_draft("/zzz", SessionStatus::Idle);
        assert_eq!(
            update(&mut state, plain_key(KeyCode::Tab)),
            vec![Effect::Redraw]
        );
        assert_eq!(state.prompt_editor.text(), "/zzz");
        assert!(state.command_menu.is_open());
        assert_eq!(state.notice.as_deref(), Some("没有可补全的命令"));
    }

    #[test]
    fn incomplete_enter_after_escape_only_completes() {
        let mut state = state_with_draft("/ex", SessionStatus::Idle);
        update(&mut state, plain_key(KeyCode::Esc));
        assert!(!state.command_menu.is_open());
        assert_eq!(
            update(&mut state, UiAction::SubmitPrompt),
            vec![Effect::Redraw]
        );
        assert_eq!(state.prompt_editor.text(), "/exit");
        assert!(!state.command_menu.is_open());
    }
}
