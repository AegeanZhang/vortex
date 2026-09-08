mod action;
mod app;
mod terminal;
mod ui;

pub mod options;

use crossterm::event::{self, Event, KeyCode};
use std::io;
use std::io::Error;

use crate::action::UiAction;
use crate::app::{AppState, update};
use crate::options::TuiOptions;
use crate::terminal::{TuiTerminal, enter};

use crate::ui::render;

pub fn run(_tui_potions: TuiOptions) -> Result<(), Error> {
    let (mut terminal, mut guard) = enter()?;

    run_app(&mut terminal)?;

    guard.cleanup()?;

    Ok(())
}

fn map_event(event: Event) -> Option<UiAction> {
    match event {
        Event::Key(key) if key.code == KeyCode::Char('q') => Some(UiAction::Quit),
        _ => None,
    }
}

fn run_app(terminal: &mut TuiTerminal) -> io::Result<()> {
    let mut state = AppState::default();

    while !state.exit_requested() {
        terminal.draw(|frame| render(frame, &state))?;

        if let Some(action) = map_event(event::read()?) {
            update(&mut state, action);
        }
    }

    Ok(())
}
