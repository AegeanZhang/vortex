mod exec;

use std::process::ExitCode;

use exec::execute;

use clap::{Parser, Subcommand};

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

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Exec { prompt }) => execute(prompt),
        None => {
            eprintln!("错误：交互式 TUI 尚未实现，请使用 vortex exec <TASK>");
            ExitCode::FAILURE
        }
    }
}
