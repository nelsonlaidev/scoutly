use std::io::{self, Write};
use std::time::{Duration, Instant};

use scoutly::{Phase, Progress, ProgressMode, ResourceProgress};

const REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const FRAME_LINES: usize = 3;

pub(crate) struct ProgressDisplay<'a, W> {
    writer: &'a mut W,
    enabled: bool,
    terminal: bool,
    width: usize,
    started_at: Instant,
    last_rendered: Option<Instant>,
    last_phase: Phase,
    latest: Option<Progress>,
    rendered: bool,
    spinner: usize,
    closed: bool,
}

impl<'a, W> ProgressDisplay<'a, W>
where
    W: Write,
{
    pub(crate) fn new(mode: ProgressMode, writer: &'a mut W, terminal: bool, width: usize) -> Self {
        let enabled = match mode {
            ProgressMode::Auto => terminal,
            ProgressMode::Always => true,
            ProgressMode::Never => false,
        };

        Self {
            writer,
            enabled,
            terminal,
            width: width.max(1),
            started_at: Instant::now(),
            last_rendered: None,
            last_phase: Phase::Robots,
            latest: None,
            rendered: false,
            spinner: 0,
            closed: false,
        }
    }

    #[must_use]
    pub(crate) const fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn update(&mut self, snapshot: Progress) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let now = Instant::now();
        let phase = snapshot.phase;
        self.latest = Some(snapshot);

        if self.should_render(phase, now) {
            self.render(now)?;
        }

        Ok(())
    }

    pub(crate) fn close(&mut self) -> io::Result<()> {
        if !self.enabled || self.closed {
            return Ok(());
        }

        self.closed = true;

        let now = Instant::now();
        let phase = self.latest.as_ref().map(|snapshot| snapshot.phase);

        if phase.is_some_and(|phase| self.should_render(phase, now)) {
            self.render(now)?;
        }

        if self.terminal && self.rendered {
            write!(self.writer, "\x1b[{FRAME_LINES}A")?;
            for line in 0..FRAME_LINES {
                self.writer.write_all(b"\r\x1b[2K")?;
                if line + 1 < FRAME_LINES {
                    self.writer.write_all(b"\n")?;
                }
            }
        }

        Ok(())
    }

    fn should_render(&self, phase: Phase, now: Instant) -> bool {
        !self.rendered
            || phase != self.last_phase
            || self
                .last_rendered
                .is_none_or(|rendered| now.saturating_duration_since(rendered) >= REFRESH_INTERVAL)
    }

    fn render(&mut self, now: Instant) -> io::Result<()> {
        let Some(snapshot) = &self.latest else {
            return Ok(());
        };

        if self.terminal {
            let frame = format_progress_frame(
                snapshot,
                now.saturating_duration_since(self.started_at),
                self.spinner,
                self.width,
            );

            if self.rendered {
                write!(self.writer, "\x1b[{FRAME_LINES}A")?;
            }

            for line in frame.lines() {
                writeln!(self.writer, "\r\x1b[2K{line}")?;
            }

            self.spinner += 1;
        } else {
            writeln!(
                self.writer,
                "[{}] pages {}/{}, sitemaps {}, links {}, images {}{}",
                snapshot.phase.as_str(),
                snapshot.pages.crawled,
                snapshot.pages.discovered,
                snapshot.sitemaps.fetched,
                format_resource(&snapshot.links),
                format_resource(&snapshot.images),
                format_current_url(&snapshot.current_url),
            )?;
        }

        self.rendered = true;
        self.last_rendered = Some(now);
        self.last_phase = snapshot.phase;

        Ok(())
    }
}

fn format_progress_frame(
    progress: &Progress,
    elapsed: Duration,
    spinner_index: usize,
    width: usize,
) -> String {
    const SPINNERS: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    let label = match progress.phase {
        Phase::Robots => "Checking robots.txt",
        Phase::Crawl => "Crawling pages",
        Phase::Sitemaps => "Reading sitemaps",
        Phase::Links => "Checking links",
        Phase::Images => "Checking images",
        Phase::Report => "Building report",
    };

    let first = format!(
        "{} {label} · {}",
        SPINNERS[spinner_index % SPINNERS.len()],
        format_elapsed(elapsed),
    );
    let second = format!(
        "Pages {}/{} · Sitemaps {} · Links {} · Images {}",
        progress.pages.crawled,
        progress.pages.discovered,
        progress.sitemaps.fetched,
        format_resource(&progress.links),
        format_resource(&progress.images),
    );
    let current_url = if progress.current_url.is_empty() {
        "Waiting for the next request...".to_owned()
    } else {
        single_line(&progress.current_url)
    };

    let lines: [String; FRAME_LINES] = [
        truncate_chars(&first, width),
        truncate_chars(&second, width),
        truncate_chars(&current_url, width),
    ];

    lines.join("\n")
}

pub(crate) fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();

    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub(crate) fn format_resource(progress: &ResourceProgress) -> String {
    progress.total.map_or_else(
        || format!("{}/--", progress.checked),
        |total| format!("{}/{total}", progress.checked),
    )
}

fn format_current_url(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        format!(" · {}", single_line(value))
    }
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    let maximum = maximum.max(1);

    if value.chars().count() <= maximum {
        return value.to_owned();
    }

    if maximum == 1 {
        return "…".to_owned();
    }

    value.chars().take(maximum - 1).chain(['…']).collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use scoutly::{PageProgress, Phase, Progress, ProgressMode, ResourceProgress, SitemapProgress};

    use super::{FRAME_LINES, ProgressDisplay, format_progress_frame, truncate_chars};

    fn progress(phase: Phase) -> Progress {
        Progress {
            phase,
            current_url: "https://example.com/a/very/long/path".into(),
            pages: PageProgress {
                discovered: 2,
                crawled: 1,
            },
            sitemaps: SitemapProgress::default(),
            links: ResourceProgress {
                checked: 1,
                total: Some(3),
            },
            images: ResourceProgress::default(),
        }
    }

    #[test]
    fn modes_route_non_terminal_progress() {
        let mut never_output = Vec::new();
        let mut never = ProgressDisplay::new(ProgressMode::Never, &mut never_output, false, 80);

        assert!(!never.enabled());
        never.update(progress(Phase::Crawl)).unwrap();
        never.close().unwrap();
        assert!(never_output.is_empty());

        let mut auto_output = Vec::new();
        let mut auto = ProgressDisplay::new(ProgressMode::Auto, &mut auto_output, false, 80);

        assert!(!auto.enabled());
        auto.update(progress(Phase::Crawl)).unwrap();
        assert!(auto_output.is_empty());

        let mut always_output = Vec::new();
        let mut always = ProgressDisplay::new(ProgressMode::Always, &mut always_output, false, 80);

        always.update(progress(Phase::Crawl)).unwrap();
        always.close().unwrap();
        let output = String::from_utf8(always_output).unwrap();
        assert!(output.starts_with("[crawl] pages 1/2"));
    }

    #[test]
    fn terminal_frame_has_the_fixed_bounded_height() {
        let frame = format_progress_frame(&progress(Phase::Links), Duration::from_secs(65), 0, 30);
        let lines = frame.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), FRAME_LINES);
        assert!(lines[0].contains("1:05"));
        assert!(lines.iter().all(|line| line.chars().count() <= 30));
    }

    #[test]
    fn progress_urls_cannot_inject_lines_or_terminal_controls() {
        let mut snapshot = progress(Phase::Crawl);
        snapshot.current_url = "https://example.com/first\nsecond\r\x1b[2J".into();

        let frame = format_progress_frame(&snapshot, Duration::ZERO, 0, 80);

        assert_eq!(frame.lines().count(), FRAME_LINES);
        assert!(!frame.contains('\r'));
        assert!(!frame.contains('\x1b'));

        let mut output = Vec::new();
        let mut display = ProgressDisplay::new(ProgressMode::Always, &mut output, false, 80);

        display.update(snapshot).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert_eq!(output.lines().count(), 1);
        assert!(!output.contains('\r'));
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn truncation_handles_narrow_widths() {
        for (value, maximum, expected) in [
            ("abc", 0, "…"),
            ("a", 1, "a"),
            ("abc", 1, "…"),
            ("abcdef", 4, "abc…"),
        ] {
            assert_eq!(truncate_chars(value, maximum), expected);
        }
    }
}
