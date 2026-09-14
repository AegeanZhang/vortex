use crate::{
    TuiOutcome,
    action::Effect,
    app::{AppState, update},
    error::TuiError,
    map_event,
    terminal::TuiTerminal,
    ui::render,
};

use crossterm::event::EventStream;
use futures_util::StreamExt;
use vortex_core::{AgentHandle, SessionConnection};

#[derive(Debug, Default)]
struct EffectResult {
    redraw: bool,
    exit: bool,
}

async fn execute_effects(
    agent: &AgentHandle,
    effects: Vec<Effect>,
) -> Result<EffectResult, TuiError> {
    let mut result = EffectResult::default();

    for effect in effects {
        match effect {
            Effect::SendCommand(command) => {
                agent.send(command).await?;
            }
            Effect::Redraw => {
                result.redraw = true;
            }
            Effect::Exit => {
                result.exit = true;
            }
        }
    }

    Ok(result)
}

pub(crate) async fn run_event_loop(
    terminal: &mut TuiTerminal,
    connection: SessionConnection,
) -> Result<TuiOutcome, TuiError> {
    let SessionConnection {
        snapshot,
        agent,
        mut events,
    } = connection;

    let mut state = AppState::from(snapshot);
    let mut terminal_events = EventStream::new();
    let mut redraw = true;

    loop {
        if redraw {
            terminal.draw(|frame| render(frame, &mut state))?;
            redraw = false;
        }

        let action = tokio::select! {
            terminal_event = terminal_events.next() => {
                match terminal_event {
                    Some(Ok(event)) => map_event(event),
                    Some(Err(error)) => return Err(error.into()),
                    None => return Err(TuiError::TerminalEventStreamClosed),
                }
            }
            core_event = events.recv() => {
                match core_event {
                    Some(event) => Some(crate::action::UiAction::CoreEvent(event)),
                    None => return Err(TuiError::CoreEventStreamClosed),
                }
            }
        };

        let Some(action) = action else {
            continue;
        };

        let effects = update(&mut state, action);
        let effect_result = execute_effects(&agent, effects).await?;

        if effect_result.exit {
            return Ok(TuiOutcome::UserExit);
        }

        redraw |= effect_result.redraw;
    }
}
