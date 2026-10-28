mod bootstrap;
mod exec;
mod logging;
mod model_factory;
mod paths;

use clap::{Parser, Subcommand};
use exec::execute;
use std::error::Error;

#[cfg(debug_assertions)]
use vortex_diagnostics::wire_trace;
use vortex_tui::options::TuiOptions;
use vortex_tui::run;

#[derive(Parser, Debug)]
#[command(name = "vortex", version, about = "A simple CLI Agent", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Override a TOML configuration value
    #[arg(short, long, global = true, value_name = "KEY=VALUE")]
    config: Vec<String>,

    #[arg(short = 'm', long, global = true, value_name = "MODEL_ID")]
    model: Option<String>,

    /// Subcommand
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "exec")]
    Exec { prompt: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cli = Cli::parse();

    let paths = paths::AppPaths::resolve()?;

    let run_mode = match &cli.command {
        Some(Commands::Exec { .. }) => logging::RunMode::Exec,
        None => logging::RunMode::Tui,
    };

    let logging_guard = logging::init(&paths.log_dir, run_mode, u8::from(cli.verbose), false)?;

    tracing::info!(
        mode = run_mode.as_str(),
        version = env!("CARGO_PKG_VERSION"),
        log_path = %logging_guard.path().display(),
        "vortex started",
    );

    match cli.command {
        Some(Commands::Exec { prompt }) => {
            if cli.model.is_some() || !cli.config.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "模型/搜索配置当前仅接入 TUI；exec 暂不支持 -m/-c",
                )
                .into());
            }
            execute(prompt).await?;
        }
        None => {
            let connection = bootstrap::start_tui_session(&paths, cli.config, cli.model).await?;
            run(connection, TuiOptions { color: None }).await?;
        }
    }

    Ok(())
}
