mod terminal;

pub mod tui_options;

use std::io;
use std::io::Error;

use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

use crate::terminal::{TuiTerminal, enter};
use crate::tui_options::TuiOptions;

pub fn run(_tui_potions: TuiOptions) -> Result<(), Error> {
    let (mut terminal, mut guard) = enter()?;

    run_app(&mut terminal)?;

    guard.cleanup()?;

    Ok(())
}

fn run_app(terminal: &mut TuiTerminal) -> io::Result<()> {
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
