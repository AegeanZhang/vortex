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
    let header = Paragraph::new("Vortex").block(Block::default().borders(Borders::ALL));
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

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
