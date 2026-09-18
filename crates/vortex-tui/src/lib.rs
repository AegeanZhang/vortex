mod action;
mod app;
mod command;
mod error;
mod event_loop;
mod markdown;
mod terminal;
mod ui;
mod widgets;

pub mod options;

pub use error::TuiError;

use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind};

use crate::{
    action::{ScrollCommand, UiAction},
    options::TuiOptions,
    terminal::enter,
};

use vortex_core::SessionConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiOutcome {
    UserExit,
}

pub async fn run(
    connection: SessionConnection,
    _tui_options: TuiOptions,
) -> Result<TuiOutcome, TuiError> {
    let (mut terminal, mut guard) = enter()?;

    let outcome = event_loop::run_event_loop(&mut terminal, connection).await?;

    guard.cleanup()?;

    Ok(outcome)
}

const WHEEL_SCROLL_LINES: usize = 3;

pub(crate) fn map_event(event: Event) -> Option<UiAction> {
    match event {
        Event::Key(key) if key.code == KeyCode::PageUp => {
            Some(UiAction::ScrollTranscript(ScrollCommand::PageUp))
        }
        Event::Key(key) if key.code == KeyCode::PageDown => {
            Some(UiAction::ScrollTranscript(ScrollCommand::PageDown))
        }
        Event::Key(key)
            if key.code == KeyCode::Home && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(UiAction::ScrollTranscript(ScrollCommand::ToTop))
        }
        Event::Key(key)
            if key.code == KeyCode::End && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(UiAction::ScrollTranscript(ScrollCommand::ToBottom))
        }
        /*Event::Key(key)
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(UiAction::Quit)
        }*/
        Event::Key(key) if key.code == KeyCode::Enter => Some(UiAction::SubmitPrompt),
        Event::Key(key) => Some(UiAction::EditPrompt(key.into())),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(UiAction::ScrollTranscript(
                ScrollCommand::LinesUp(WHEEL_SCROLL_LINES),
            )),
            MouseEventKind::ScrollDown => Some(UiAction::ScrollTranscript(
                ScrollCommand::LinesDown(WHEEL_SCROLL_LINES),
            )),
            _ => None,
        },
        _ => None,
    }
}
