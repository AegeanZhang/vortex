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

pub(crate) fn render(frame: &mut Frame<'_>, state: &AppState) {
    let [header_area, main_area, status_area, prompt_area] = sections(frame.area());

    // 绘制顶部标题栏
    let header = Paragraph::new("Rust TUI Demo").block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, header_area);

    let main = Paragraph::new("Press 'Ctrl-C' to quit.\n\nTry resizing the terminal window.")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(main, main_area);

    let status = Paragraph::new(state.status_line());
    frame.render_widget(status, status_area);

    state.prompt_editor().render(frame, prompt_area);
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

        let state = AppState::default();

        terminal.draw(|frame| render(frame, &state)).unwrap();

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(0, 2)].symbol(), "└");

        assert_eq!(buffer[(0, 3)].symbol(), "┌");
        assert_eq!(buffer[(0, 19)].symbol(), "└");

        assert_eq!(buffer[(0, 21)].symbol(), "┌");
        assert_eq!(buffer[(0, 23)].symbol(), "└");
    }
}
