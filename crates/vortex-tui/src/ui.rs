use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph},
};

// 将终端分为三个部分，分别是顶部、中部和底部
fn sections(area: Rect) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    [chunks[0], chunks[1], chunks[2]]
}

pub(crate) fn render(frame: &mut Frame<'_>) {
    let [header_area, main_area, footer_area] = sections(frame.area());

    // 绘制顶部标题栏
    let header = Paragraph::new("Rust TUI Demo").block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, header_area);

    let main = Paragraph::new("Press 'q' to quit.\n\nTry resizing the terminal window.")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(main, main_area);

    let footer = Paragraph::new("Status: Running").block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, footer_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn splits_standard_terminal_into_three_sections() {
        let areas = sections(Rect::new(0, 0, 80, 24));

        assert_eq!(areas[0], Rect::new(0, 0, 80, 3));
        assert_eq!(areas[1], Rect::new(0, 3, 80, 18));
        assert_eq!(areas[2], Rect::new(0, 21, 80, 3));
    }

    #[test]
    fn renders_widgets_at_expected_positions() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(render).unwrap();

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(0, 2)].symbol(), "└");

        assert_eq!(buffer[(0, 3)].symbol(), "┌");
        assert_eq!(buffer[(0, 20)].symbol(), "└");

        assert_eq!(buffer[(0, 21)].symbol(), "┌");
        assert_eq!(buffer[(0, 23)].symbol(), "└");
    }
}
