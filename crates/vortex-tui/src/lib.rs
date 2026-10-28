mod action;
mod app;
mod clipboard;
mod command;
mod error;
mod event_loop;
mod markdown;
mod selection;
mod terminal;
mod ui;
mod widgets;

pub mod options;

pub use error::TuiError;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};

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
        Event::Key(key) if key.kind == KeyEventKind::Release => None,
        Event::Key(key)
            if key.kind == KeyEventKind::Repeat
                && matches!(key.code, KeyCode::Enter | KeyCode::Tab | KeyCode::Esc) =>
        {
            None
        }
        Event::Key(key)
            if matches!(key.code, KeyCode::Char('c' | 'C'))
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            Some(UiAction::CopySelection)
        }
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
        Event::Key(key) if key.code == KeyCode::Enter && key.modifiers.is_empty() => {
            Some(UiAction::SubmitPrompt)
        }
        Event::Key(key) => Some(UiAction::EditPrompt(key.into())),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left) => Some(UiAction::SelectText(mouse)),
            MouseEventKind::ScrollUp => Some(UiAction::ScrollTranscript(ScrollCommand::LinesUp(
                WHEEL_SCROLL_LINES,
            ))),
            MouseEventKind::ScrollDown => Some(UiAction::ScrollTranscript(
                ScrollCommand::LinesDown(WHEEL_SCROLL_LINES),
            )),
            _ => None,
        },
        Event::Resize(_, _) => Some(UiAction::RedrawRequested),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};

    fn key_event(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
        let mut key = KeyEvent::new(code, modifiers);
        key.kind = kind;
        Event::Key(key)
    }

    #[test]
    fn copy_shortcuts_and_left_button_selection_are_mapped_without_affecting_wheel() {
        assert_eq!(
            map_event(Event::Key(KeyEvent::new(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ))),
            Some(UiAction::CopySelection)
        );
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ] {
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(KeyCode::Char('c'), modifiers))),
                Some(UiAction::CopySelection)
            );
        }
        assert_eq!(
            map_event(key_event(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Release
            )),
            None
        );
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            let mouse = MouseEvent {
                kind,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE,
            };
            assert_eq!(
                map_event(Event::Mouse(mouse)),
                Some(UiAction::SelectText(mouse))
            );
        }
        assert_eq!(
            map_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE
            })),
            None
        );
    }

    #[test]
    fn submit_requires_plain_enter_press() {
        assert_eq!(
            map_event(key_event(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Press
            )),
            Some(UiAction::SubmitPrompt)
        );
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
        ] {
            let key = KeyEvent::new(KeyCode::Enter, modifiers);
            assert_eq!(
                map_event(Event::Key(key)),
                Some(UiAction::EditPrompt(key.into()))
            );
        }
    }

    #[test]
    fn key_releases_never_produce_actions() {
        for code in [
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Esc,
            KeyCode::Char('/'),
            KeyCode::Up,
            KeyCode::PageUp,
        ] {
            assert_eq!(
                map_event(key_event(code, KeyModifiers::NONE, KeyEventKind::Release)),
                None
            );
        }
    }

    #[test]
    fn repeat_ignores_confirmation_keys_but_keeps_editing_and_navigation() {
        for code in [KeyCode::Enter, KeyCode::Tab, KeyCode::Esc] {
            assert_eq!(
                map_event(key_event(code, KeyModifiers::NONE, KeyEventKind::Repeat)),
                None
            );
        }
        for code in [KeyCode::Char('e'), KeyCode::Up, KeyCode::Down] {
            let mut key = KeyEvent::new(code, KeyModifiers::NONE);
            key.kind = KeyEventKind::Repeat;
            assert_eq!(
                map_event(Event::Key(key)),
                Some(UiAction::EditPrompt(key.into()))
            );
        }
    }

    #[test]
    fn menu_keys_are_routed_to_stateful_editor_handling() {
        for code in [KeyCode::Up, KeyCode::Down, KeyCode::Tab, KeyCode::Esc] {
            let key = KeyEvent::new(code, KeyModifiers::NONE);
            assert_eq!(
                map_event(Event::Key(key)),
                Some(UiAction::EditPrompt(key.into()))
            );
        }
    }

    #[test]
    fn transcript_scroll_keys_and_mouse_wheel_keep_their_mapping() {
        for (code, modifiers, command) in [
            (KeyCode::PageUp, KeyModifiers::NONE, ScrollCommand::PageUp),
            (
                KeyCode::PageDown,
                KeyModifiers::NONE,
                ScrollCommand::PageDown,
            ),
            (KeyCode::Home, KeyModifiers::CONTROL, ScrollCommand::ToTop),
            (KeyCode::End, KeyModifiers::CONTROL, ScrollCommand::ToBottom),
        ] {
            assert_eq!(
                map_event(Event::Key(KeyEvent::new(code, modifiers))),
                Some(UiAction::ScrollTranscript(command))
            );
        }
        for (kind, command) in [
            (MouseEventKind::ScrollUp, ScrollCommand::LinesUp(3)),
            (MouseEventKind::ScrollDown, ScrollCommand::LinesDown(3)),
        ] {
            let mouse = MouseEvent {
                kind,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE,
            };
            assert_eq!(
                map_event(Event::Mouse(mouse)),
                Some(UiAction::ScrollTranscript(command))
            );
        }
    }

    #[test]
    fn resize_requests_redraw() {
        assert_eq!(
            map_event(Event::Resize(80, 24)),
            Some(UiAction::RedrawRequested)
        );
    }
}
