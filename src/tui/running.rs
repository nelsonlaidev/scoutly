use std::collections::VecDeque;
use std::time::Instant;

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{
    Style,
    palette::tailwind::{GREEN, NEUTRAL},
};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Padding, Paragraph};
use scoutly::{Phase, Progress, ResourceProgress};
use tui_scrollbar::ScrollBarInteraction;

use super::{MOUSE_WHEEL_DELTA, handle_scrollbar_mouse, render_pane, render_scrollbar, title_case};
use crate::cli_progress::{format_elapsed, format_resource};

const MAX_ACTIVITY_ENTRIES: usize = 200;
const SUMMARY_ROWS: u16 = 4;
const CURRENT_PANE_ROWS: u16 = 4;
const PHASES: [Phase; 6] = [
    Phase::Robots,
    Phase::Crawl,
    Phase::Sitemaps,
    Phase::Links,
    Phase::Images,
    Phase::Report,
];

#[derive(Debug)]
pub(super) struct RunningState {
    pub(super) target: String,
    started_at: Instant,
    now: Instant,
    progress: Progress,
    activity: VecDeque<String>,
    activity_offset: usize,
    activity_following: bool,
    activity_scrollbar_interaction: ScrollBarInteraction,
    canceling: bool,
}

impl RunningState {
    pub(super) fn new(target: String) -> Self {
        let now = Instant::now();

        Self {
            target,
            started_at: now,
            now,
            progress: Progress {
                phase: Phase::Robots,
                current_url: String::new(),
                pages: scoutly::PageProgress::default(),
                sitemaps: scoutly::SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            },
            activity: VecDeque::new(),
            activity_offset: 0,
            activity_following: true,
            activity_scrollbar_interaction: ScrollBarInteraction::new(),
            canceling: false,
        }
    }

    pub(super) fn receive_progress(&mut self, progress: Progress) {
        if !progress.current_url.is_empty() {
            let entry = format!("{}: {}", progress.phase.as_str(), progress.current_url);
            if self.activity.back() != Some(&entry) {
                self.activity.push_back(entry);
                if self.activity.len() > MAX_ACTIVITY_ENTRIES {
                    self.activity.pop_front();
                    if !self.activity_following {
                        self.activity_offset = self.activity_offset.saturating_sub(1);
                    }
                }
            }
        }

        self.progress = progress;
    }

    pub(super) fn tick(&mut self) {
        self.now = Instant::now();
    }

    pub(super) fn begin_cancel(&mut self) -> bool {
        if self.canceling {
            return false;
        }

        self.canceling = true;

        true
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) {
        let (_, recent_area) = activity_areas(area);
        let visible = activity_visible_rows(recent_area);

        if handle_scrollbar_mouse(
            mouse,
            recent_area,
            self.activity.len(),
            visible,
            &mut self.activity_offset,
            &mut self.activity_scrollbar_interaction,
        ) {
            let maximum = self.activity.len().saturating_sub(visible);
            self.activity_following = self.activity_offset == maximum;
            return;
        }

        if !recent_area.contains(Position::new(mouse.column, mouse.row)) {
            return;
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.activity_offset = self.activity_offset.saturating_sub(MOUSE_WHEEL_DELTA);
                self.activity_following = false;
            }
            MouseEventKind::ScrollDown => {
                self.activity_offset = self.activity_offset.saturating_add(MOUSE_WHEEL_DELTA);
            }
            _ => return,
        }

        let maximum = self.activity.len().saturating_sub(visible);
        self.activity_offset = self.activity_offset.min(maximum);
        self.activity_following = self.activity_offset == maximum;
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let phase_area = Rect::new(area.x, area.y, area.width, 1);
        let metric_area = Rect::new(area.x, area.y.saturating_add(2), area.width, 1);
        let (current_area, recent_area) = activity_areas(area);

        let current_phase = PHASES
            .iter()
            .position(|phase| *phase == self.progress.phase)
            .unwrap_or(0);

        let mut spans = Vec::with_capacity(PHASES.len() * 2);

        for (index, phase) in PHASES.into_iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw(" "));
            }
            let (marker, style) = if index < current_phase {
                ("[x]", Style::new().fg(GREEN.c400).bold())
            } else if index == current_phase {
                ("[>]", Style::new().fg(NEUTRAL.c50).bold())
            } else {
                ("[ ]", Style::new().fg(NEUTRAL.c500))
            };
            spans.push(Span::styled(
                format!("{marker} {}", title_case(phase.as_str())),
                style,
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(Block::new().padding(Padding::horizontal(1))),
            phase_area,
        );

        let status = if self.canceling { " · canceling" } else { "" };

        let metrics = format!(
            "{} · {}/{} pages · {} sitemaps · {} links · {} images{status}",
            format_elapsed(self.now.saturating_duration_since(self.started_at)),
            self.progress.pages.crawled,
            self.progress.pages.discovered,
            self.progress.sitemaps.fetched,
            format_resource(&self.progress.links),
            format_resource(&self.progress.images),
        );
        frame.render_widget(
            Paragraph::new(metrics)
                .style(NEUTRAL.c200)
                .block(Block::new().padding(Padding::horizontal(1))),
            metric_area,
        );

        let current_url = if self.progress.current_url.is_empty() {
            "Preparing audit..."
        } else {
            &self.progress.current_url
        };

        render_pane(
            frame,
            current_area,
            "Current activity",
            false,
            |frame, inner| {
                frame.render_widget(Paragraph::new(current_url).style(NEUTRAL.c200), inner);
            },
        );

        render_pane(
            frame,
            recent_area,
            "Recent activity",
            false,
            |frame, inner| {
                let visible = activity_visible_rows(recent_area);
                debug_assert_eq!(visible, usize::from(inner.height).max(1));
                let maximum = self.activity.len().saturating_sub(visible);
                if self.activity_following {
                    self.activity_offset = maximum;
                } else {
                    self.activity_offset = self.activity_offset.min(maximum);
                    self.activity_following = self.activity_offset == maximum;
                }
                let content = if self.activity.is_empty() {
                    Text::from(Line::from(Span::styled(
                        "Waiting for the first request...",
                        NEUTRAL.c500,
                    )))
                } else {
                    Text::from(
                        self.activity
                            .iter()
                            .skip(self.activity_offset)
                            .take(visible)
                            .map(|entry| Line::from(entry.as_str()))
                            .collect::<Vec<_>>(),
                    )
                };
                frame.render_widget(Paragraph::new(content).style(NEUTRAL.c200), inner);
                render_scrollbar(
                    frame,
                    recent_area,
                    self.activity.len(),
                    visible,
                    self.activity_offset,
                    false,
                );
            },
        );
    }
}

fn activity_areas(area: Rect) -> (Rect, Rect) {
    let current = Rect::new(
        area.x,
        area.y.saturating_add(SUMMARY_ROWS),
        area.width,
        CURRENT_PANE_ROWS.min(area.height.saturating_sub(SUMMARY_ROWS)),
    );
    let recent = Rect::new(
        area.x,
        current.bottom(),
        area.width,
        area.bottom().saturating_sub(current.bottom()),
    );

    (current, recent)
}

fn activity_visible_rows(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(2)).max(1)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use ratatui::{Terminal, backend::TestBackend};
    use scoutly::{PageProgress, Phase, Progress, ResourceProgress, SitemapProgress};

    use super::{MAX_ACTIVITY_ENTRIES, RunningState};

    #[test]
    fn activity_is_deduplicated_and_bounded() {
        let mut state = RunningState::new("https://example.com".into());

        for index in 0..MAX_ACTIVITY_ENTRIES + 5 {
            state.receive_progress(Progress {
                phase: Phase::Crawl,
                current_url: format!("https://example.com/{index}"),
                pages: PageProgress::default(),
                sitemaps: SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            });
        }

        let last = state.progress.clone();
        state.receive_progress(last);

        assert_eq!(state.activity.len(), MAX_ACTIVITY_ENTRIES);
    }

    #[test]
    fn cancel_is_idempotent() {
        let mut state = RunningState::new("https://example.com".into());
        assert!(state.begin_cancel());
        assert!(!state.begin_cancel());
    }

    #[test]
    fn mouse_scrolling_pauses_and_resumes_activity_following() {
        let mut state = RunningState::new("https://example.com".into());

        for index in 0..30 {
            state.receive_progress(Progress {
                phase: Phase::Crawl,
                current_url: format!("https://example.com/{index}"),
                pages: PageProgress::default(),
                sitemaps: SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            });
        }

        state.activity_offset = 20;
        let area = Rect::new(1, 3, 68, 16);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 2,
                row: 15,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        assert!(!state.activity_following);
        assert_eq!(state.activity_offset, 17);

        for _ in 0..20 {
            state.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: 2,
                    row: 15,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
        }

        assert!(state.activity_following);
    }

    #[test]
    fn activity_scrollbar_drag_updates_the_visible_offset() {
        let mut state = RunningState::new("https://example.com".into());

        for index in 0..30 {
            state.receive_progress(Progress {
                phase: Phase::Crawl,
                current_url: format!("https://example.com/{index}"),
                pages: PageProgress::default(),
                sitemaps: SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            });
        }

        state.activity_offset = 0;
        state.activity_following = false;

        let area = Rect::new(1, 3, 68, 16);
        let (_, recent_area) = super::activity_areas(area);
        let scrollbar_column = recent_area.right() - 1;

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: scrollbar_column,
                row: recent_area.y + 1,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                column: scrollbar_column,
                row: recent_area.y + 3,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );

        assert!(state.activity_offset > 0);
        assert!(!state.activity_following);
    }

    #[test]
    fn resizing_to_reveal_the_bottom_resumes_activity_following() {
        let mut state = RunningState::new("https://example.com".into());

        for index in 0..10 {
            state.receive_progress(Progress {
                phase: Phase::Crawl,
                current_url: format!("https://example.com/{index}"),
                pages: PageProgress::default(),
                sitemaps: SitemapProgress::default(),
                links: ResourceProgress::default(),
                images: ResourceProgress::default(),
            });
        }

        state.activity_offset = 2;
        state.activity_following = false;

        let backend = TestBackend::new(70, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| state.render(frame, Rect::new(1, 3, 68, 26)))
            .unwrap();

        assert!(state.activity_following);
        assert_eq!(state.activity_offset, 0);
    }
}
