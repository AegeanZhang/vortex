mod exec;

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

    match cli.command {
        Some(Commands::Exec { prompt }) => {
            execute(prompt).await?;
        }
        None => {
            let api_key = env::var("DEEPSEEK_API_KEY")?;

            let provider = Arc::new(OpenAiChatProvider::new(
                "https://api.deepseek.com",
                "deepseek-v4-pro",
                api_key,
            ));

            let connection = start_session(provider);
            let tui_options = TuiOptions { color: None };

            run(connection, tui_options).await?;
        }
    }

    Ok(())
}
