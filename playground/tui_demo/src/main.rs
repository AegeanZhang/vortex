use std::io;

use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;

fn main() -> io::Result<()> {
    let mut guard = TerminalGuard::new()?;

    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    // 当前阶段没有文本输入，隐藏硬件光标可避免它停留在最后一次绘制位置。
    terminal.hide_cursor()?;

    run_app(&mut terminal)?;

    guard.cleanup()?;

    Ok(())
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;

        // 从 raw mode 生效后立即接管清理；后续初始化失败时 Drop 也能恢复终端。
        let guard = Self { active: true };

        let mut stdout = std::io::stdout();

        execute!(stdout, EnterAlternateScreen)?;

        Ok(guard)
    }

    fn cleanup(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }

        disable_raw_mode()?;

        let mut stdout = std::io::stdout();
        execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show)?;

        self.active = false;

        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();

            let mut stdout = std::io::stdout();

            let _ = execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show);
        }
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> io::Result<()> {
    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(area);

            // 绘制顶部标题栏
            let header =
                Paragraph::new("Rust TUI Demo").block(Block::default().borders(Borders::ALL));

            frame.render_widget(header, chunks[0]);

            let main = Paragraph::new("Press 'q' to quit.\n\nTry resizing the terminal window.")
                .block(Block::default().borders(Borders::ALL));

            frame.render_widget(main, chunks[1]);

            let footer =
                Paragraph::new("Status: Running").block(Block::default().borders(Borders::ALL));

            frame.render_widget(footer, chunks[2]);
        })?;

        // 同步等待事件
        let event = event::read()?;

        match event {
            Event::Key(key) if key.code == KeyCode::Char('q') => {
                break;
            }

            Event::Resize(_, _) => {}

            _ => {}
        }
    }

    Ok(())
}
