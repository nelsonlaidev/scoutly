use anyhow::Result;
use clap::Parser;
use colored::*;
use scoutly::cli::Cli;
use scoutly::run;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    if let Err(e) = run(args).await {
        eprintln!("{} {}", "Error:".bright_red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}
