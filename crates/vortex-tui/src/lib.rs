mod terminal;
mod ui;

pub mod tui_options;

use crossterm::event::{self, Event, KeyCode};
use std::io;
use std::io::Error;

use crate::terminal::{TuiTerminal, enter};
use crate::tui_options::TuiOptions;

use crate::ui::render;

pub fn run(_tui_potions: TuiOptions) -> Result<(), Error> {
    let (mut terminal, mut guard) = enter()?;

    run_app(&mut terminal)?;

    guard.cleanup()?;

    Ok(())
}

fn run_app(terminal: &mut TuiTerminal) -> io::Result<()> {
    loop {
        terminal.draw(render)?;

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
