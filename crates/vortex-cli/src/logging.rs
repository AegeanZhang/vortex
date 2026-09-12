use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

pub(crate) type LoggingError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RunMode {
    Tui,
    Exec,
}

impl RunMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Exec => "exec",
        }
    }
}

pub(crate) struct LoggingGuard {
    _worker_guard: WorkerGuard,
    log_path: PathBuf,
}

impl LoggingGuard {
    pub(crate) fn path(&self) -> &Path {
        &self.log_path
    }
}

pub(crate) fn init(
    _mode: RunMode,
    verbosity: u8,
    quiet: bool,
) -> Result<LoggingGuard, LoggingError> {
    let vortex_home = env::var_os("VORTEX_HOME")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "VORTEX_HOME is not set"))?;

    let vortex_home = PathBuf::from(vortex_home);

    if !vortex_home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "VORTEX_HOME must be an absolute path",
        )
        .into());
    }

    let log_dir = vortex_home.join("logs");
    fs::create_dir_all(&log_dir)?;

    let log_path = log_dir.join("vortex.log");

    let mut options = OpenOptions::new();
    options.create(true).append(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let file = options.open(&log_path)?;
    let (writer, worker_guard) = tracing_appender::non_blocking(file);

    let directive = if quiet {
        "error"
    } else {
        match verbosity {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
    };

    let filter = if verbosity > 0 || quiet {
        EnvFilter::new(directive)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directive))
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .json()
        .try_init()?;

    Ok(LoggingGuard {
        _worker_guard: worker_guard,
        log_path,
    })
}
