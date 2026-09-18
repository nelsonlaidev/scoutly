use std::path::Path;
use std::process::{Command, Output};

#[path = "../common/mod.rs"]
mod common;

mod audit;
mod cli;
mod config;
mod signals;

fn binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_scoutly"))
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .current_dir(common::repository_root())
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", binary().display()))
}

fn assert_command_succeeded(name: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{name} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
