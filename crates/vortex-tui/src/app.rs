use crate::{
    action::{Effect, ScrollCommand, UiAction},
    command::{CommandParse, ParsedCommand, parse_command},
    widgets::{CommandMenu, MenuKey, ModelPicker, PromptEditor, PromptInput, Transcript},
};

use vortex_core::{AgentCommand, CoreEvent, ModelInfo, SessionSnapshot, SessionStatus};

#[derive(Debug)]
struct PendingModel {
    model_id: String,
    command_draft: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponsePhase {
    Waiting,
    Reasoning,
    Answering,
    Tool,
}

#[derive(Debug)]
pub(crate) struct AppState {
    status: SessionStatus,
    notice: Option<String>,
    prompt_editor: PromptEditor,
    transcript: Transcript,
    command_menu: CommandMenu,
    selected_model: Option<ModelInfo>,
    available_models: Vec<ModelInfo>,
    model_picker: ModelPicker,
    pending_model: Option<PendingModel>,
    response_phase: Option<ResponsePhase>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: SessionStatus::Idle,
            notice: None,
            prompt_editor: PromptEditor::default(),
            transcript: Transcript::default(),
            command_menu: CommandMenu::default(),
            selected_model: None,
            available_models: Vec::new(),
            model_picker: ModelPicker::default(),
            pending_model: None,
            response_phase: None,
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
            selected_model: Some(snapshot.model),
            available_models: snapshot.models,
            model_picker: ModelPicker::default(),
            pending_model: None,
            response_phase: (snapshot.status == SessionStatus::Running)
                .then_some(ResponsePhase::Waiting),
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
        /*
        match &self.notice {
            Some(notice) => format!("{status} | {notice}"),
            None => status.to_string(),
        }
        */

        let mut line = status.to_string();

        if let Some(phase) = self.response_phase {
            let label = match phase {
                ResponsePhase::Waiting => "等待模型",
                ResponsePhase::Reasoning => "思考中",
                ResponsePhase::Answering => "回答中",
                ResponsePhase::Tool => "执行工具",
            };
            line.push_str(&format!(" | {label}"));
        }

        if let Some(model) = &self.selected_model {
            line.push_str(&format!(" | Model: {}", model.id));
        }

        if let Some(notice) = &self.notice {
            line.push_str(&format!(" | {notice}"));
        }

        line
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

    pub(crate) fn model_picker(&self) -> &ModelPicker {
        &self.model_picker
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
    if state.model_picker.is_open() {
        match input.menu_key() {
            Some(MenuKey::Escape) => state.model_picker.close(),
            Some(MenuKey::Up) if state.pending_model.is_none() => state.model_picker.previous(),
            Some(MenuKey::Down) if state.pending_model.is_none() => state.model_picker.next(),
            _ => {}
        }
        return vec![Effect::Redraw];
    }

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

fn open_model_picker(state: &mut AppState) -> Vec<Effect> {
    if state.pending_model.is_some() {
        state.notice = Some("正在等待模型切换确认".into());
        return vec![Effect::Redraw];
    }
    state.command_menu.dismiss();
    state
        .model_picker
        .open(&state.available_models, state.selected_model.as_ref());
    state.notice = None;
    vec![Effect::Redraw]
}

fn request_model_selection(state: &mut AppState, model_id: String) -> Vec<Effect> {
    if state.pending_model.is_some() {
        state.notice = Some("正在等待模型切换确认".into());
        return vec![Effect::Redraw];
    }
    if state.status == SessionStatus::Running {
        state.notice = Some("当前 Turn 正在运行，完成后再切换模型".into());
        return vec![Effect::Redraw];
    }
    if let Some(reason) = state
        .available_models
        .iter()
        .find(|model| model.id == model_id)
        .and_then(|model| model.unavailable_reason.as_ref())
    {
        state.notice = Some(reason.clone());
        return vec![Effect::Redraw];
    }
    state.pending_model = Some(PendingModel {
        model_id: model_id.clone(),
        command_draft: state.prompt_editor.text(),
    });
    state.notice = Some("正在等待模型切换确认".into());
    vec![
        Effect::SendCommand(AgentCommand::SelectModel { model_id }),
        Effect::Redraw,
    ]
}

fn submit_prompt(state: &mut AppState) -> Vec<Effect> {
    if state.model_picker.is_open() {
        if let Some(model_id) = state
            .model_picker
            .selected_model()
            .map(|model| model.id.clone())
        {
            return request_model_selection(state, model_id);
        }
        state.notice = Some("没有可选择的模型".into());
        return vec![Effect::Redraw];
    }

    // 先读后判，不以 take_text 为解析或状态检查的前提
    let text = state.prompt_editor.text();
    match parse_command(&text) {
        CommandParse::Ready(ParsedCommand::Exit) => {
            // 同步路由到现有语义 Action; Quit 最终分支，不会回到提交形成递归.
            return update(state, UiAction::Quit);
        }
        CommandParse::Ready(ParsedCommand::Model { model_id: None }) => {
            return open_model_picker(state);
        }
        CommandParse::Ready(ParsedCommand::Model { model_id: Some(id) }) => {
            return request_model_selection(state, id);
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

    if state.pending_model.is_some() {
        state.notice = Some("模型切换尚未确认，请稍后提交".into());
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
            state.response_phase = Some(ResponsePhase::Waiting);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::AssistantTextDelta { delta }) => {
            state.transcript.append_assistant_delta(delta);
            state.response_phase = Some(ResponsePhase::Answering);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnCompleted) => {
            state.status = SessionStatus::Idle;
            state.notice = None;
            state.response_phase = None;
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::TurnFailed { message }) => {
            state.status = SessionStatus::Failed;
            state.notice = None;
            state.transcript.fail_assistant(message);
            state.response_phase = None;
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::CommandRejected { message }) => {
            state.notice = Some(message);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ToolCallStarted { call }) => {
            state.notice = Some(format!("Running tool: {}", call.name));
            state.response_phase = Some(ResponsePhase::Tool);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ToolCallFinished { name, result }) => {
            state.notice = Some(if result.is_error {
                format!("Tool failed: {name}; waiting for model")
            } else {
                format!("Tool completed: {name}; waiting for model")
            });
            state.response_phase = Some(ResponsePhase::Waiting);
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ModelSelected { model }) => {
            let matches_pending = state
                .pending_model
                .as_ref()
                .is_some_and(|pending| pending.model_id == model.id);
            state.selected_model = Some(model.clone());
            if matches_pending {
                if let Some(pending) = state.pending_model.take() {
                    // 用户可能已经 Esc 并编辑；确认不能抹掉之后输入的新草稿。
                    if state.prompt_editor.text() == pending.command_draft {
                        state.prompt_editor.replace_text("");
                        state.command_menu.sync("");
                    }
                }
                state.model_picker.close();
                state.notice = Some(format!("已切换模型：{}（仅本会话）", model.id));
            }
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ModelSelectionRejected { model_id, message }) => {
            if state
                .pending_model
                .as_ref()
                .is_some_and(|pending| pending.model_id == model_id)
            {
                state.pending_model = None;
                state.notice = Some(message);
            }
            // 保留当前模型、原草稿和仍打开的窗口，允许重新选择。
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::ModelStepStarted { step }) => {
            state.transcript.start_model_step();
            state.response_phase = Some(ResponsePhase::Waiting);
            state.notice = Some(format!("模型步骤 {step}"));
            vec![Effect::Redraw]
        }
        UiAction::CoreEvent(CoreEvent::AssistantReasoningDelta { delta }) => {
            state.transcript.append_reasoning_delta(delta);
            if !matches!(state.response_phase, Some(ResponsePhase::Answering)) {
                state.response_phase = Some(ResponsePhase::Reasoning);
            }
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

    fn receive(state: &mut AppState, event: CoreEvent) {
        assert_eq!(
            update(state, UiAction::CoreEvent(event)),
            vec![Effect::Redraw]
        );
    }

    #[test]
    fn response_phases_follow_model_and_tool_steps_without_late_reasoning_regression() {
        let mut state = AppState::default();
        receive(&mut state, CoreEvent::AssistantMessageStarted);
        assert_eq!(state.status, SessionStatus::Running);
        assert_eq!(state.response_phase, Some(ResponsePhase::Waiting));
        receive(&mut state, CoreEvent::ModelStepStarted { step: 1 });
        receive(
            &mut state,
            CoreEvent::AssistantReasoningDelta { delta: "R1".into() },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Reasoning));
        assert!(state.status_line().contains("思考中"));
        receive(
            &mut state,
            CoreEvent::AssistantTextDelta { delta: "P".into() },
        );
        receive(
            &mut state,
            CoreEvent::AssistantReasoningDelta {
                delta: "late R1".into(),
            },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Answering));
        assert!(state.status_line().contains("回答中"));
        let call = vortex_core::ToolCall {
            call_id: "fixture-call".into(),
            name: "fixture_tool".into(),
            arguments: Default::default(),
        };
        receive(
            &mut state,
            CoreEvent::ToolCallStarted { call: call.clone() },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Tool));
        assert!(state.status_line().contains("执行工具"));
        receive(
            &mut state,
            CoreEvent::ToolCallFinished {
                name: call.name,
                result: vortex_core::ToolResult {
                    call_id: call.call_id,
                    output: "fixture result".into(),
                    is_error: false,
                },
            },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Waiting));
        receive(&mut state, CoreEvent::ModelStepStarted { step: 2 });
        assert_eq!(state.response_phase, Some(ResponsePhase::Waiting));
        assert_eq!(state.notice.as_deref(), Some("模型步骤 2"));
        receive(
            &mut state,
            CoreEvent::AssistantReasoningDelta { delta: "R2".into() },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Reasoning));
        receive(
            &mut state,
            CoreEvent::AssistantTextDelta { delta: "A".into() },
        );
        receive(&mut state, CoreEvent::TurnCompleted);
        assert_eq!(state.status, SessionStatus::Idle);
        assert_eq!(state.response_phase, None);
        assert_eq!(state.notice, None);
    }

    #[test]
    fn rejection_preserves_active_thinking_draft_and_pending_model() {
        let mut state = state_with_draft("next prompt", SessionStatus::Running);
        state.pending_model = Some(PendingModel {
            model_id: "fixture-model".into(),
            command_draft: "/model fixture-model".into(),
        });
        receive(&mut state, CoreEvent::AssistantMessageStarted);
        receive(
            &mut state,
            CoreEvent::AssistantReasoningDelta { delta: "R".into() },
        );
        receive(
            &mut state,
            CoreEvent::CommandRejected {
                message: "busy".into(),
            },
        );
        assert_eq!(state.status, SessionStatus::Running);
        assert_eq!(state.response_phase, Some(ResponsePhase::Reasoning));
        assert_eq!(state.prompt_editor.text(), "next prompt");
        assert_eq!(
            state.pending_model.as_ref().unwrap().model_id,
            "fixture-model"
        );
        assert_eq!(state.notice.as_deref(), Some("busy"));
        receive(
            &mut state,
            CoreEvent::TurnFailed {
                message: "fixture failure".into(),
            },
        );
        assert_eq!(state.status, SessionStatus::Failed);
        assert_eq!(state.response_phase, None);
        assert_eq!(state.notice, None);
    }

    #[test]
    fn status_line_combines_response_phase_dynamic_model_and_notice() {
        let mut state = AppState::default();
        receive(
            &mut state,
            CoreEvent::ModelSelected {
                model: ModelInfo {
                    id: "fixture-thinking".into(),
                    display_name: "Fixture".into(),
                    provider_id: "fixture".into(),
                    endpoint_host: "example.test".into(),
                    supports_tools: true,
                    unavailable_reason: None,
                },
            },
        );
        receive(&mut state, CoreEvent::AssistantMessageStarted);
        receive(&mut state, CoreEvent::ModelStepStarted { step: 1 });
        receive(
            &mut state,
            CoreEvent::AssistantReasoningDelta { delta: "R".into() },
        );
        assert_eq!(
            state.status_line(),
            "Status: Running | 思考中 | Model: fixture-thinking | 模型步骤 1"
        );
        receive(&mut state, CoreEvent::TurnCompleted);
        assert_eq!(
            state.status_line(),
            "Status: Idle | Model: fixture-thinking"
        );
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
    fn a_new_model_step_resets_answering_to_waiting() {
        let mut state = AppState::default();
        receive(&mut state, CoreEvent::AssistantMessageStarted);
        receive(
            &mut state,
            CoreEvent::AssistantTextDelta {
                delta: "prefix".into(),
            },
        );
        assert_eq!(state.response_phase, Some(ResponsePhase::Answering));
        receive(&mut state, CoreEvent::ModelStepStarted { step: 2 });
        assert_eq!(state.response_phase, Some(ResponsePhase::Waiting));
        assert_eq!(state.status, SessionStatus::Running);
        assert!(state.status_line().contains("等待模型"));
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
