use std::{error::Error, fmt, io};
use vortex_core::SessionError;

#[derive(Debug)]
pub enum TuiError {
    Io(io::Error),
    Session(SessionError),
    TerminalEventStreamClosed,
    CoreEventStreamClosed,
}

impl fmt::Display for TuiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "terminal I/O error: {error}"),
            Self::Session(error) => write!(formatter, "session error: {error}"),
            Self::TerminalEventStreamClosed => {
                write!(formatter, "terminal event stream closed")
            }
            Self::CoreEventStreamClosed => {
                write!(formatter, "core event stream closed")
            }
        }
    }
}

impl Error for TuiError {}

impl From<io::Error> for TuiError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SessionError> for TuiError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}
