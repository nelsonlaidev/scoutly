use anyhow::{Result, bail};
use std::io::{IsTerminal, stdin, stdout};
use tokio::sync::mpsc::UnboundedSender;

use crate::cli::OutputFormat;
use crate::config::RuntimeOptions;
use crate::models::{CrawlReport, CrawlSummary};
use crate::update::UpdateNotice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Tui,
    Text,
    Json,
}

impl LaunchMode {
    pub const fn output_format(self) -> Option<OutputFormat> {
        match self {
            Self::Tui => None,
            Self::Text => Some(OutputFormat::Text),
            Self::Json => Some(OutputFormat::Json),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStage {
    LoadingConfig,
    Crawling,
    CheckingLinks,
    AnalyzingSeo,
    GeneratingReport,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub stage: RunStage,
    pub message: String,
    pub pages_crawled: usize,
    pub links_discovered: usize,
    pub links_checked: usize,
    pub total_links: usize,
    pub summary: CrawlSummary,
}

impl ProgressSnapshot {
    pub fn new(stage: RunStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
            pages_crawled: 0,
            links_discovered: 0,
            links_checked: 0,
            total_links: 0,
            summary: CrawlSummary {
                total_pages: 0,
                total_links: 0,
                broken_links: 0,
                errors: 0,
                warnings: 0,
                infos: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum RunEvent {
    Progress(ProgressSnapshot),
    ReportReady(CrawlReport),
    UpdateAvailable(UpdateNotice),
    Error(String),
}

pub type RunEventSender = UnboundedSender<RunEvent>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSupport {
    pub stdin_is_terminal: bool,
    pub stdout_is_terminal: bool,
}

impl TerminalSupport {
    pub fn current() -> Self {
        Self {
            stdin_is_terminal: stdin().is_terminal(),
            stdout_is_terminal: stdout().is_terminal(),
        }
    }

    pub const fn is_interactive(self) -> bool {
        self.stdin_is_terminal && self.stdout_is_terminal
    }
}

pub fn resolve_launch_mode(
    runtime: &RuntimeOptions,
    terminal: TerminalSupport,
) -> Result<LaunchMode> {
    if runtime.cli && runtime.tui {
        bail!("--cli and --tui cannot be used together");
    }

    if runtime.tui {
        if terminal.is_interactive() {
            return Ok(LaunchMode::Tui);
        }

        bail!("The TUI requires an interactive terminal");
    }

    match runtime.output {
        Some(OutputFormat::Json) => Ok(LaunchMode::Json),
        Some(OutputFormat::Text) => Ok(LaunchMode::Text),
        None if runtime.cli => Ok(LaunchMode::Text),
        None if terminal.is_interactive() => Ok(LaunchMode::Tui),
        None => Ok(LaunchMode::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuntimeOptions;

    fn runtime() -> RuntimeOptions {
        RuntimeOptions {
            url: None,
            depth: 5,
            max_pages: 200,
            output: None,
            save: None,
            cli: false,
            external: false,
            verbose: false,
            ignore_redirects: false,
            keep_fragments: false,
            rate_limit: None,
            concurrency: 5,
            respect_robots_txt: true,
            tui: false,
            config: None,
        }
    }

    const INTERACTIVE: TerminalSupport = TerminalSupport {
        stdin_is_terminal: true,
        stdout_is_terminal: true,
    };

    const NON_INTERACTIVE: TerminalSupport = TerminalSupport {
        stdin_is_terminal: false,
        stdout_is_terminal: false,
    };

    const STDIN_ONLY: TerminalSupport = TerminalSupport {
        stdin_is_terminal: true,
        stdout_is_terminal: false,
    };

    const STDOUT_ONLY: TerminalSupport = TerminalSupport {
        stdin_is_terminal: false,
        stdout_is_terminal: true,
    };

    #[test]
    fn defaults_to_tui_for_interactive_terminals() {
        assert_eq!(
            resolve_launch_mode(&runtime(), INTERACTIVE).unwrap(),
            LaunchMode::Tui
        );
    }

    #[test]
    fn defaults_to_text_for_non_interactive_terminals() {
        assert_eq!(
            resolve_launch_mode(&runtime(), NON_INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn cli_flag_forces_text() {
        let mut options = runtime();
        options.cli = true;

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn output_json_forces_json() {
        let mut options = runtime();
        options.output = Some(OutputFormat::Json);

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Json
        );
    }

    #[test]
    fn explicit_tui_requires_an_interactive_terminal() {
        let mut options = runtime();
        options.tui = true;

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Tui
        );
        assert!(resolve_launch_mode(&options, NON_INTERACTIVE).is_err());
    }

    #[test]
    fn output_text_forces_text() {
        let mut options = runtime();
        options.output = Some(OutputFormat::Text);

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn cli_and_tui_conflict_returns_error() {
        let mut options = runtime();
        options.cli = true;
        options.tui = true;

        assert!(resolve_launch_mode(&options, INTERACTIVE).is_err());
        assert!(resolve_launch_mode(&options, NON_INTERACTIVE).is_err());
    }

    #[test]
    fn cli_with_output_json_prefers_output_format() {
        let mut options = runtime();
        options.cli = true;
        options.output = Some(OutputFormat::Json);

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Json
        );
    }

    #[test]
    fn cli_with_output_text_prefers_output_format() {
        let mut options = runtime();
        options.cli = true;
        options.output = Some(OutputFormat::Text);

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn tui_with_output_json_on_interactive_prefers_tui() {
        let mut options = runtime();
        options.tui = true;
        options.output = Some(OutputFormat::Json);

        // --tui is checked before output, so TUI wins on interactive terminals
        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::Tui
        );
    }

    #[test]
    fn tui_with_output_json_on_non_interactive_falls_back_to_output() {
        let mut options = runtime();
        options.tui = true;
        options.output = Some(OutputFormat::Json);

        // --tui fails on non-interactive, but --tui is checked first so error
        assert!(resolve_launch_mode(&options, NON_INTERACTIVE).is_err());
    }

    #[test]
    fn non_interactive_with_output_text() {
        let mut options = runtime();
        options.output = Some(OutputFormat::Text);

        assert_eq!(
            resolve_launch_mode(&options, NON_INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn non_interactive_with_output_json() {
        let mut options = runtime();
        options.output = Some(OutputFormat::Json);

        assert_eq!(
            resolve_launch_mode(&options, NON_INTERACTIVE).unwrap(),
            LaunchMode::Json
        );
    }

    #[test]
    fn cli_on_non_interactive_terminal() {
        let mut options = runtime();
        options.cli = true;

        assert_eq!(
            resolve_launch_mode(&options, NON_INTERACTIVE).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn stdin_only_is_not_interactive() {
        assert_eq!(
            resolve_launch_mode(&runtime(), STDIN_ONLY).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn stdout_only_is_not_interactive() {
        assert_eq!(
            resolve_launch_mode(&runtime(), STDOUT_ONLY).unwrap(),
            LaunchMode::Text
        );
    }

    #[test]
    fn explicit_tui_fails_with_partial_terminal() {
        let mut options = runtime();
        options.tui = true;

        assert!(resolve_launch_mode(&options, STDIN_ONLY).is_err());
        assert!(resolve_launch_mode(&options, STDOUT_ONLY).is_err());
    }

    #[test]
    fn launch_mode_output_format_mapping() {
        assert_eq!(LaunchMode::Tui.output_format(), None);
        assert_eq!(LaunchMode::Text.output_format(), Some(OutputFormat::Text));
        assert_eq!(LaunchMode::Json.output_format(), Some(OutputFormat::Json));
    }
}
