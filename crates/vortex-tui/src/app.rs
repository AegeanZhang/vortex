use crate::action::UiAction;

#[derive(Debug)]
pub(crate) struct AppState {
    exit_requested: bool,
    status_line: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            exit_requested: false,
            status_line: "Status: Running".to_string(),
        }
    }
}

impl AppState {
    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(crate) fn status_line(&self) -> &str {
        &self.status_line
    }
}

pub(crate) fn update(state: &mut AppState, action: UiAction) {
    match action {
        UiAction::Quit => {
            state.exit_requested = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_action_requests_exit() {
        let mut state = AppState::default();

        update(&mut state, UiAction::Quit);

        assert!(state.exit_requested());
    }
}
