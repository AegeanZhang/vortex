use crate::app::AppState;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph},
};

// 将终端分为四个部分，分别是顶部、中部、状态栏和prompt输入区域
fn sections(area: Rect) -> [Rect; 4] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // transcript
            Constraint::Length(1), // status
            Constraint::Length(3), // prompt
        ])
        .split(area);

    [chunks[0], chunks[1], chunks[2], chunks[3]]
}

fn command_menu_area(main_area: Rect, prompt_area: Rect, desired_height: u16) -> Option<Rect> {
    let height = desired_height.min(main_area.height);
    let width = prompt_area.width.min(main_area.width).min(48);
    if width < 4 || height < 3 {
        return None;
    }
    Some(Rect::new(
        main_area.x,
        main_area.bottom().saturating_sub(height),
        width,
        height,
    ))
}

fn model_picker_area(area: Rect) -> Option<Rect> {
    let width = area.width.saturating_sub(2).min(80);
    let height = area.height.saturating_sub(2).min(18);
    if width < 4 || height < 4 {
        return None;
    }
    Some(Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    ))
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &mut AppState) {
    let [header_area, main_area, status_area, prompt_area] = sections(frame.area());

    // 绘制顶部标题栏
    let header = Paragraph::new("Vortex | 拖选文本 · Ctrl+C 复制 · Shift+方向键选择 Prompt")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, header_area);

    state.transcript_mut().render(frame, main_area);

    let status = Paragraph::new(state.status_line());
    frame.render_widget(status, status_area);

    if state.command_menu().is_open()
        && let Some(area) = command_menu_area(
            main_area,
            prompt_area,
            state.command_menu().desired_height(),
        )
    {
        state.command_menu().render(frame, area);
    }

    // 输入框最后绘制，保证编辑器拥有最终的光标呈现。
    state.prompt_editor().render(frame, prompt_area);

    if state.model_picker().is_open()
        && let Some(area) = model_picker_area(frame.area())
    {
        state.model_picker().render(frame, area);
    }

    // 在高亮之前截取实际渲染单元格，复制可见纯文本而不是 Markdown/装饰。
    let regions = if state.model_picker().is_open() || state.command_menu().is_open() {
        Vec::new()
    } else {
        let inner = main_area.inner(ratatui::layout::Margin::new(1, 1));
        let transcript = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        let mut regions = vec![transcript];
        // placeholder 是提示，不是待复制的用户草稿。
        if !state.prompt_editor().text().is_empty() {
            regions.push(prompt_area.inner(ratatui::layout::Margin::new(1, 1)));
        }
        regions
    };
    state.selection.capture(frame.buffer_mut(), &regions);
    state.selection.highlight(frame.buffer_mut());
    // 居中模型窗口可覆盖状态栏，不能在其上重画状态文字。
    if !state.model_picker().is_open() {
        frame.render_widget(Paragraph::new(state.status_line()), status_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn select(state: &mut AppState, start: (u16, u16), end: (u16, u16)) {
        use crate::{action::UiAction, app::update};
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        for (kind, (column, row)) in [
            (MouseEventKind::Down(MouseButton::Left), start),
            (MouseEventKind::Drag(MouseButton::Left), end),
            (MouseEventKind::Up(MouseButton::Left), end),
        ] {
            update(
                state,
                UiAction::SelectText(MouseEvent {
                    kind,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                }),
            );
        }
    }

    #[test]
    fn selection_status_refresh_does_not_overwrite_model_picker_border() {
        let mut state = AppState::default();
        for ch in "/model".chars() {
            edit_key(&mut state, crossterm::event::KeyCode::Char(ch));
        }
        crate::app::update(&mut state, crate::action::UiAction::SubmitPrompt);
        assert!(state.model_picker().is_open());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let picker = model_picker_area(Rect::new(0, 0, 80, 24)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(picker.x, picker.bottom() - 1)].symbol(),
            "└"
        );
    }

    #[test]
    fn empty_prompt_placeholder_cannot_be_copied_as_user_text() {
        let mut state = AppState::default();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        select(&mut state, (1, 22), (15, 22));
        assert!(state.selection.text().is_none());
    }

    #[test]
    fn mouse_selects_rendered_markdown_and_prompt_without_chrome_or_submission() {
        use crate::{
            action::{Effect, UiAction},
            app::update,
        };
        use vortex_core::CoreEvent;
        let mut state = AppState::default();
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantMessageStarted),
        );
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantTextDelta {
                delta: "**Hello** 中文".into(),
            }),
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        select(&mut state, (1, 5), (5, 5));
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        assert!(
            terminal.backend().buffer()[(1, 5)]
                .modifier
                .contains(ratatui::style::Modifier::REVERSED)
        );
        assert_eq!(
            update(&mut state, UiAction::CopySelection),
            vec![Effect::CopyText("Hello".into()), Effect::Redraw]
        );
        // 只复制渲染纯文本，星号和外框不在内容区域。
        assert_eq!(state.selection.text().as_deref(), Some("Hello"));
        for ch in "prompt".chars() {
            edit_key(&mut state, crossterm::event::KeyCode::Char(ch));
        }
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        select(&mut state, (1, 22), (6, 22));
        assert_eq!(
            update(&mut state, UiAction::CopySelection),
            vec![Effect::CopyText("prompt".into()), Effect::Redraw]
        );
        assert_eq!(state.prompt_editor().text(), "prompt");
    }

    #[test]
    fn selection_is_invalidated_on_stream_update_scroll_resize_and_menu_overlay() {
        use crate::{
            action::{ScrollCommand, UiAction},
            app::update,
        };
        use vortex_core::CoreEvent;
        let mut state = AppState::default();
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantMessageStarted),
        );
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantTextDelta {
                delta: "first".into(),
            }),
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        select(&mut state, (1, 5), (5, 5));
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::AssistantTextDelta {
                delta: " second".into(),
            }),
        );
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        assert!(state.selection.text().is_none());
        for action in [
            UiAction::ScrollTranscript(ScrollCommand::PageUp),
            UiAction::RedrawRequested,
        ] {
            select(&mut state, (1, 5), (5, 5));
            update(&mut state, action);
            assert!(state.selection.text().is_none());
        }
        select(&mut state, (1, 5), (5, 5));
        edit_key(&mut state, crossterm::event::KeyCode::Char('/'));
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        select(&mut state, (1, 5), (5, 5));
        assert!(state.selection.text().is_none());
    }

    #[test]
    fn splits_standard_terminal_into_four_sections() {
        let areas = sections(Rect::new(0, 0, 80, 24));

        assert_eq!(areas[0], Rect::new(0, 0, 80, 3));
        assert_eq!(areas[1], Rect::new(0, 3, 80, 17));
        assert_eq!(areas[2], Rect::new(0, 20, 80, 1));
        assert_eq!(areas[3], Rect::new(0, 21, 80, 3));
    }

    #[test]
    fn renders_widgets_at_expected_positions() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::default();

        terminal.draw(|frame| render(frame, &mut state)).unwrap();

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(0, 2)].symbol(), "└");

        assert_eq!(buffer[(0, 3)].symbol(), "┌");
        assert_eq!(buffer[(0, 19)].symbol(), "└");

        assert_eq!(buffer[(0, 21)].symbol(), "┌");
        assert_eq!(buffer[(0, 23)].symbol(), "└");
    }

    fn edit_key(state: &mut AppState, code: crossterm::event::KeyCode) {
        use crate::{action::UiAction, app::update};
        use crossterm::event::{KeyEvent, KeyModifiers};

        update(
            state,
            UiAction::EditPrompt(KeyEvent::new(code, KeyModifiers::NONE).into()),
        );
    }

    #[test]
    fn core_events_render_plain_thinking_and_markdown_answer_as_separate_sections() {
        use crate::{action::UiAction, app::update};
        use vortex_core::CoreEvent;

        let mut state = AppState::default();
        for event in [
            CoreEvent::AssistantMessageStarted,
            CoreEvent::ModelStepStarted { step: 1 },
            CoreEvent::AssistantReasoningDelta {
                delta: "**literal thought**".into(),
            },
            CoreEvent::AssistantTextDelta {
                delta: "**answer**".into(),
            },
            CoreEvent::CommandRejected {
                message: "busy".into(),
            },
        ] {
            update(&mut state, UiAction::CoreEvent(event));
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let thinking = rows
            .iter()
            .position(|line| line.contains("Thinking:"))
            .unwrap();
        let reasoning = rows
            .iter()
            .position(|line| line.contains("**literal thought**"))
            .unwrap();
        let answer = rows
            .iter()
            .position(|line| line.contains("answer"))
            .unwrap();
        assert!(thinking < reasoning && reasoning < answer);
        assert!(!rows.iter().any(|line| line.contains("**answer**")));
        // TestBackend 的双宽字符后带有占位单元格，比较文字时去掉这些空格。
        assert!(rows[20].replace(' ', "").contains("回答中"));
        assert!(rows[20].contains("busy"));
        assert_eq!(
            buffer[(1, reasoning as u16)].fg,
            ratatui::style::Color::DarkGray
        );
    }

    #[test]
    fn menu_rect_is_bounded_and_tiny_areas_are_skipped() {
        let main = Rect::new(0, 3, 80, 17);
        let prompt = Rect::new(0, 21, 80, 3);
        assert_eq!(
            command_menu_area(main, prompt, 3),
            Some(Rect::new(0, 17, 48, 3))
        );
        assert_eq!(
            command_menu_area(main, prompt, 100),
            Some(Rect::new(0, 3, 48, 17))
        );
        assert!(command_menu_area(Rect::new(0, 3, 3, 17), prompt, 3).is_none());
        assert!(command_menu_area(Rect::new(0, 3, 80, 2), prompt, 3).is_none());
        assert_eq!(
            command_menu_area(Rect::new(5, 8, 12, 5), Rect::new(5, 14, 12, 3), 3),
            Some(Rect::new(5, 10, 12, 3))
        );
    }

    #[test]
    fn menu_closes_without_moving_prompt_or_erasing_underlying_history() {
        use crate::{
            action::{ScrollCommand, UiAction},
            app::update,
        };
        use crossterm::event::KeyCode;
        use vortex_core::CoreEvent;

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut state = AppState::default();
        let content = (0..40)
            .map(|index| format!("history line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        update(
            &mut state,
            UiAction::CoreEvent(CoreEvent::UserMessageAdded { content }),
        );
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        update(
            &mut state,
            UiAction::ScrollTranscript(ScrollCommand::PageUp),
        );
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let before = terminal.backend().buffer().clone();

        edit_key(&mut state, KeyCode::Char('/'));
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let buffer = terminal.backend().buffer();
        let [_, main_area, _, prompt_area] = sections(buffer.area);
        let menu_area = command_menu_area(
            main_area,
            prompt_area,
            state.command_menu().desired_height(),
        )
        .unwrap();
        assert_eq!(menu_area.height, 4);
        for (offset, command) in [(1, "/exit"), (2, "/model")] {
            let row: String = (menu_area.x..menu_area.right())
                .map(|x| buffer[(x, menu_area.y + offset)].symbol())
                .collect();
            assert!(row.contains(command), "expected {command} in {row:?}");
        }
        assert_eq!(buffer[(0, 21)].symbol(), "┌");

        edit_key(&mut state, KeyCode::Esc);
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let buffer = terminal.backend().buffer();
        for y in 3..20 {
            for x in 0..80 {
                assert_eq!(buffer[(x, y)], before[(x, y)], "cell ({x}, {y})");
            }
        }
        assert_eq!(buffer[(0, 21)].symbol(), "┌");
    }

    #[test]
    fn unknown_prefix_renders_no_matching_commands() {
        use crossterm::event::KeyCode;

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut state = AppState::default();
        for ch in "/zzz".chars() {
            edit_key(&mut state, KeyCode::Char(ch));
        }
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        assert_eq!(terminal.backend().buffer()[(1, 18)].symbol(), "没");
    }

    #[test]
    fn open_menu_renders_safely_in_small_terminals() {
        use crossterm::event::KeyCode;

        for (width, height) in [(0, 0), (1, 1), (3, 6), (20, 5), (80, 8), (12, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut state = AppState::default();
            edit_key(&mut state, KeyCode::Char('/'));
            terminal.draw(|frame| render(frame, &mut state)).unwrap();
        }
    }
}
