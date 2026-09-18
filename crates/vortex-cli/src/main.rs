mod exec;
mod logging;

use clap::{Parser, Subcommand};
use exec::execute;
use std::{env, error::Error, sync::Arc};

use vortex_core::start_session;
use vortex_provider::OpenAiChatProvider;
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

    let run_mode = match &cli.command {
        Some(Commands::Exec { .. }) => logging::RunMode::Exec,
        None => logging::RunMode::Tui,
    };

    let logging_guard = logging::init(run_mode, u8::from(cli.verbose), false)?;

    tracing::info!(
        mode = run_mode.as_str(),
        version = env!("CARGO_PKG_VERSION"),
        log_path = %logging_guard.path().display(),
        "vortex started",
    );

    match cli.command {
        Some(Commands::Exec { prompt }) => {
            execute(prompt).await?;
        }
        None => {
            //let api_key = env::var("DEEPSEEK_API_KEY")?;
            let api_key = env::var("QWEN_API_KEY")?;

            /*let provider = Arc::new(OpenAiChatProvider::new(
                "https://api.deepseek.com",
                "deepseek-v4-pro",
                api_key,
            ));*/
            let provider = Arc::new(OpenAiChatProvider::new(
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "qwen3.8-max",
                api_key,
            ));

            let connection = start_session(provider);
            let tui_options = TuiOptions { color: None };

            run(connection, tui_options).await?;
        }
    }

    Ok(())
}
