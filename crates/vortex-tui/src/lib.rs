mod action;
mod app;
mod error;
mod event_loop;
mod terminal;
mod ui;
mod widgets;

pub mod options;

use crossterm::event::{Event, KeyCode, KeyModifiers};

use crate::{action::UiAction, options::TuiOptions, terminal::enter};

use vortex_core::SessionConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiOutcome {
    UserExit,
}

pub async fn run(
    connection: SessionConnection,
    _tui_potions: TuiOptions,
) -> Result<TuiOutcome, error::TuiError> {
    let (mut terminal, mut guard) = enter()?;

    let outcome = event_loop::run_event_loop(&mut terminal, connection).await?;

    guard.cleanup()?;

    Ok(outcome)
}

pub(crate) fn map_event(event: Event) -> Option<UiAction> {
    match event {
        Event::Key(key)
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(UiAction::Quit)
        }
        Event::Key(key) if key.code == KeyCode::Enter => Some(UiAction::SubmitPrompt),
        Event::Key(key) => Some(UiAction::EditPrompt(key.into())),
        _ => None,
    }
}
