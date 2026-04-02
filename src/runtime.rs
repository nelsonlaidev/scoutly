use anyhow::{Result, bail};
use std::io::{IsTerminal, stdin, stdout};
use tokio::sync::mpsc::UnboundedSender;

use crate::cli::OutputFormat;
use crate::config::RuntimeOptions;
use crate::models::{CrawlReport, CrawlSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Tui,
    ClassicText,
    ClassicJson,
}

impl LaunchMode {
    pub const fn output_format(self) -> Option<OutputFormat> {
        match self {
            Self::Tui => None,
            Self::ClassicText => Some(OutputFormat::Text),
            Self::ClassicJson => Some(OutputFormat::Json),
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
        Some(OutputFormat::Json) => Ok(LaunchMode::ClassicJson),
        Some(OutputFormat::Text) => Ok(LaunchMode::ClassicText),
        None if runtime.cli => Ok(LaunchMode::ClassicText),
        None if terminal.is_interactive() => Ok(LaunchMode::Tui),
        None => Ok(LaunchMode::ClassicText),
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

    #[test]
    fn defaults_to_tui_for_interactive_terminals() {
        assert_eq!(
            resolve_launch_mode(&runtime(), INTERACTIVE).unwrap(),
            LaunchMode::Tui
        );
    }

    #[test]
    fn defaults_to_classic_text_for_non_interactive_terminals() {
        assert_eq!(
            resolve_launch_mode(&runtime(), NON_INTERACTIVE).unwrap(),
            LaunchMode::ClassicText
        );
    }

    #[test]
    fn cli_flag_forces_classic_text() {
        let mut options = runtime();
        options.cli = true;

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::ClassicText
        );
    }

    #[test]
    fn output_json_forces_classic_json() {
        let mut options = runtime();
        options.output = Some(OutputFormat::Json);

        assert_eq!(
            resolve_launch_mode(&options, INTERACTIVE).unwrap(),
            LaunchMode::ClassicJson
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
}
