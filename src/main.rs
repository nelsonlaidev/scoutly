mod cli;
mod cli_progress;
mod output;
mod tui;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = u8::try_from(error.exit_code()).expect("clap exit codes fit in u8");
            let _ = error.print();
            return ExitCode::from(code);
        }
    };

    match cli::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "scoutly: error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
