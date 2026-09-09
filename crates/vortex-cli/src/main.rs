mod exec;

use clap::{Parser, Subcommand};
use exec::execute;
use std::error::Error;

use vortex_tui::options::TuiOptions;
use vortex_tui::run;

use vortex_core::start_session;

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
            let connection = start_session();
            let tui_options = TuiOptions { color: None };
            run(connection, tui_options).await?;
        }
    }

    Ok(())
}
