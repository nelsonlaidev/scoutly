use anyhow::Result;
use clap::Parser;
use colored::*;
use scoutly::cli::Cli;
use scoutly::run;
use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let args = Cli::parse();

    if let Err(e) = run(args).await {
        eprintln!("{} {}", "Error:".bright_red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}

fn init_logging() {
    let filter = EnvFilter::try_from_env("SCOUTLY_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("off"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();
}
