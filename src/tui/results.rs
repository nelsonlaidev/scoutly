use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Position, Rect};
use ratatui::style::{Style, palette::tailwind::NEUTRAL};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Padding, Paragraph, Row, Table, TableState, Wrap,
};
use scoutly::{
    FailureReason, Image, ImageOccurrence, Issue, Link, LinkOccurrence, Page, Report, ResultKind,
    TargetType,
};
use tui_scrollbar::ScrollBarInteraction;

use super::{MOUSE_WHEEL_DELTA, text_byte_index};
use super::{handle_scrollbar_mouse, pane_inner, render_pane, render_scrollbar};

const RESULT_HEADER_ROWS: u16 = 4;
const FILTER_FIELD_WIDTH: u16 = 22;
const TAB_PADDING: u16 = 1;
// The content spans the full terminal width, so this matches the documented
// 100-column terminal breakpoint.
const SPLIT_PANE_WIDTH: u16 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultTab {
    Overview,
    Issues,
    Pages,
    Links,
    Images,
}

impl ResultTab {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Issues,
        Self::Pages,
        Self::Links,
        Self::Images,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Issues => "Issues",
            Self::Pages => "Pages",
            Self::Links => "Links",
            Self::Images => "Images",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultFocus {
    List,
    Detail,
    Search,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultScrollbar {
    List,
    Detail,
    Overview,
}

#[derive(Clone, Copy, Debug)]
enum ResultSource {
    Issue(usize),
    Page(usize),
    Link(usize),
    Image(usize),
}

#[derive(Debug)]
struct ResultItem {
    source: ResultSource,
    label: String,
    description: String,
}

impl ResultItem {
    fn issue(index: usize, issue: &Issue) -> Self {
        Self {
            source: ResultSource::Issue(index),
            label: issue.message.clone(),
            description: format!(
                "{}  {}",
                issue.severity.as_str().to_uppercase(),
                issue.target.url
            ),
        }
    }

    fn page(index: usize, page: &Page) -> Self {
        Self {
            source: ResultSource::Page(index),
            label: page.title.clone().unwrap_or_else(|| page.url.clone()),
            description: format!(
                "{}  {}",
                page.status_code
                    .map_or_else(|| "FAILED".to_owned(), |value| value.to_string()),
                page.url
            ),
        }
    }

    fn link(index: usize, link: &Link) -> Self {
        Self {
            source: ResultSource::Link(index),
            label: link.url.clone(),
            description: describe_link(link),
        }
    }

    fn image(index: usize, image: &Image) -> Self {
        Self {
            source: ResultSource::Image(index),
            label: image.url.clone(),
            description: describe_image(image),
        }
    }

    fn kind(&self) -> &'static str {
        match self.source {
            ResultSource::Issue(_) => "Issue",
            ResultSource::Page(_) => "Page",
            ResultSource::Link(_) => "Link",
            ResultSource::Image(_) => "Image",
        }
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Debug)]
struct ResultFilters {
    issues: usize,
    pages: usize,
    links: usize,
    images: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResultsAction {
    None,
    Quit,
    NewAudit,
}

#[derive(Debug)]
pub(super) struct ResultsState {
    tab: ResultTab,
    focus: ResultFocus,
    query: String,
    query_cursor: usize,
    filters: ResultFilters,
    selected: usize,
    list_offset: usize,
    detail_open: bool,
    detail_offset: usize,
    overview_offset: usize,
    list_scrollbar_interaction: ScrollBarInteraction,
    detail_scrollbar_interaction: ScrollBarInteraction,
    overview_scrollbar_interaction: ScrollBarInteraction,
    active_scrollbar: Option<ResultScrollbar>,
    items: Vec<ResultItem>,
}

impl ResultsState {
    pub(super) fn new(report: &Report) -> Self {
        let mut state = Self {
            tab: ResultTab::Overview,
            focus: ResultFocus::Detail,
            query: String::new(),
            query_cursor: 0,
            filters: ResultFilters {
                issues: 0,
                pages: 0,
                links: 0,
                images: 0,
            },
            selected: 0,
            list_offset: 0,
            detail_open: false,
            detail_offset: 0,
            overview_offset: 0,
            list_scrollbar_interaction: ScrollBarInteraction::new(),
            detail_scrollbar_interaction: ScrollBarInteraction::new(),
            overview_scrollbar_interaction: ScrollBarInteraction::new(),
            active_scrollbar: None,
            items: Vec::new(),
        };

        state.refresh(report);

        state
    }

    pub(super) fn handle_key(
        &mut self,
        key: KeyEvent,
        report: &Report,
        area: Rect,
    ) -> ResultsAction {
        if self.focus == ResultFocus::Search {
            match key.code {
                KeyCode::Esc | KeyCode::Tab | KeyCode::Enter => {
                    self.focus = ResultFocus::List;
                }
                KeyCode::Left => self.query_cursor = self.query_cursor.saturating_sub(1),
                KeyCode::Right => {
                    self.query_cursor = (self.query_cursor + 1).min(self.query.chars().count());
                }
                KeyCode::Home => self.query_cursor = 0,
                KeyCode::End => self.query_cursor = self.query.chars().count(),
                KeyCode::Backspace => {
                    if self.query_cursor > 0 {
                        self.query_cursor -= 1;
                        if self.remove_query_character() {
                            self.reset_selection();
                            self.refresh(report);
                        }
                    }
                }
                KeyCode::Delete => {
                    if self.remove_query_character() {
                        self.reset_selection();
                        self.refresh(report);
                    }
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    let byte = text_byte_index(&self.query, self.query_cursor);
                    self.query.insert(byte, character);
                    self.query_cursor += 1;
                    self.reset_selection();
                    self.refresh(report);
                }
                _ => {}
            }

            return ResultsAction::None;
        }

        match key.code {
            KeyCode::Char(character @ '1'..='5') => {
                let index = character.to_digit(10).unwrap_or(1) as usize - 1;
                self.change_tab(ResultTab::ALL[index], report);
            }
            KeyCode::Left => {
                let index = self.tab_index();
                self.change_tab(
                    ResultTab::ALL[(index + ResultTab::ALL.len() - 1) % ResultTab::ALL.len()],
                    report,
                );
            }
            KeyCode::Right => {
                let index = self.tab_index();
                self.change_tab(ResultTab::ALL[(index + 1) % ResultTab::ALL.len()], report);
            }
            KeyCode::Char('q') => return ResultsAction::Quit,
            KeyCode::Char('n') => return ResultsAction::NewAudit,
            KeyCode::Char('/') if self.tab != ResultTab::Overview => {
                self.focus = ResultFocus::Search;
                self.query_cursor = self.query.chars().count();
            }
            KeyCode::Char('f') if self.tab != ResultTab::Overview => {
                self.cycle_filter();
                self.refresh(report);
            }
            KeyCode::Tab if self.tab != ResultTab::Overview => {
                self.focus = if self.focus == ResultFocus::List {
                    ResultFocus::Detail
                } else {
                    ResultFocus::List
                };
            }
            KeyCode::Esc => {
                if area.width < SPLIT_PANE_WIDTH && self.detail_open {
                    self.detail_open = false;
                    self.focus = ResultFocus::List;
                } else if self.focus == ResultFocus::Detail {
                    self.focus = ResultFocus::List;
                } else if !self.query.is_empty() {
                    self.query.clear();
                    self.query_cursor = 0;
                    self.reset_selection();
                    self.refresh(report);
                }
            }
            KeyCode::Enter
                if area.width < SPLIT_PANE_WIDTH
                    && self.tab != ResultTab::Overview
                    && !self.items.is_empty() =>
            {
                self.detail_open = !self.detail_open;
                self.focus = if self.detail_open {
                    ResultFocus::Detail
                } else {
                    ResultFocus::List
                };
            }
            KeyCode::Up => self.move_vertical(-1, area),
            KeyCode::Down => self.move_vertical(1, area),
            KeyCode::PageUp => self.move_vertical(-i32::from(area.height.max(1)), area),
            KeyCode::PageDown => self.move_vertical(i32::from(area.height.max(1)), area),
            KeyCode::Home => self.move_to_edge(false),
            KeyCode::End => self.move_to_edge(true),
            _ => {}
        }

        ResultsAction::None
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, report: &Report, area: Rect) {
        if self.handle_scrollbars_mouse(mouse, report, area) {
            return;
        }

        if !area.contains(Position::new(mouse.column, mouse.row)) {
            return;
        }

        let x = mouse.column - area.x;
        let y = mouse.row - area.y;

        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                    -(MOUSE_WHEEL_DELTA as i32)
                } else {
                    MOUSE_WHEEL_DELTA as i32
                };
                self.scroll_mouse(delta, x, y, area);
                return;
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return,
        }

        if y == 0 {
            if let Some(tab) = tab_at(x)
                && tab != self.tab
            {
                self.change_tab(tab, report);
            }
            return;
        }

        if self.tab == ResultTab::Overview {
            if (1..4).contains(&y) {
                let point = Position::new(x, y);
                if let Some(index) = overview_metric_areas(Rect::new(0, 1, area.width, 3))
                    .iter()
                    .position(|metric| metric.contains(point))
                {
                    self.change_tab(
                        [
                            ResultTab::Pages,
                            ResultTab::Issues,
                            ResultTab::Links,
                            ResultTab::Images,
                        ][index],
                        report,
                    );
                }
            }
            return;
        }

        if y < RESULT_HEADER_ROWS {
            let search_width = area.width.saturating_sub(FILTER_FIELD_WIDTH + 1);
            if x < search_width {
                self.focus = ResultFocus::Search;
                self.query_cursor = self.query.chars().count();
            } else if x > search_width {
                self.focus = ResultFocus::List;
                self.cycle_filter();
                self.refresh(report);
            }
            return;
        }

        let pane_area = Rect::new(
            area.x,
            area.y + RESULT_HEADER_ROWS,
            area.width,
            area.height.saturating_sub(RESULT_HEADER_ROWS),
        );
        let (list_area, detail_area) = pane_areas(pane_area);
        let point = Position::new(mouse.column, mouse.row);

        if list_area.contains(point) && !(area.width < SPLIT_PANE_WIDTH && self.detail_open) {
            self.focus = ResultFocus::List;
            let data_top = list_area.y.saturating_add(2);
            if mouse.row >= data_top {
                let index = self.list_offset + usize::from(mouse.row - data_top);
                if index < self.items.len() {
                    self.selected = index;
                    self.detail_offset = 0;
                    if area.width < SPLIT_PANE_WIDTH {
                        self.detail_open = true;
                        self.focus = ResultFocus::Detail;
                    }
                }
            }
        } else if detail_area.is_some_and(|detail| detail.contains(point))
            || (area.width < SPLIT_PANE_WIDTH && self.detail_open)
        {
            self.focus = ResultFocus::Detail;
        }
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, report: &Report) {
        self.render_tabs(frame, Rect::new(area.x, area.y, area.width, 1));

        if self.tab == ResultTab::Overview {
            self.render_overview(
                frame,
                Rect::new(
                    area.x,
                    area.y + 1,
                    area.width,
                    area.height.saturating_sub(1),
                ),
                report,
            );
            return;
        }

        let controls = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            3.min(area.height.saturating_sub(1)),
        );
        self.render_controls(frame, controls);

        let panes = Rect::new(
            area.x,
            area.y + RESULT_HEADER_ROWS,
            area.width,
            area.height.saturating_sub(RESULT_HEADER_ROWS),
        );
        self.render_panes(frame, panes, report);
    }

    fn render_tabs(&self, frame: &mut Frame<'_>, area: Rect) {
        let spans = ResultTab::ALL
            .into_iter()
            .enumerate()
            .flat_map(|(index, tab)| {
                let separator = (index > 0).then(|| Span::raw("  "));
                separator.into_iter().chain(std::iter::once(Span::styled(
                    format!("{} {}", index + 1, tab.label()),
                    if tab == self.tab {
                        Style::new().fg(NEUTRAL.c50).bold()
                    } else {
                        Style::new().fg(NEUTRAL.c500)
                    },
                )))
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(Line::from(spans))
                .block(Block::new().padding(Padding::horizontal(TAB_PADDING))),
            area,
        );
    }

    fn render_controls(&self, frame: &mut Frame<'_>, area: Rect) {
        let search_width = area.width.saturating_sub(FILTER_FIELD_WIDTH + 1);
        let search = Rect::new(area.x, area.y, search_width, area.height);
        let filter = Rect::new(
            search.right().saturating_add(1),
            area.y,
            area.right()
                .saturating_sub(search.right().saturating_add(1)),
            area.height,
        );
        let search_style = if self.focus == ResultFocus::Search {
            Style::new().fg(NEUTRAL.c50).bold()
        } else {
            Style::new().fg(NEUTRAL.c700)
        };
        let label_style = if self.focus == ResultFocus::Search {
            Style::new().fg(NEUTRAL.c50).bold()
        } else {
            Style::new().fg(NEUTRAL.c500)
        };

        let search_block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(search_style)
            .padding(Padding::horizontal(1));
        let search_inner = search_block.inner(search);
        frame.render_widget(search_block, search);

        let query = if self.query.is_empty() && self.focus != ResultFocus::Search {
            Line::from(vec![
                Span::styled("Search: ", label_style),
                Span::styled("Press / to search", NEUTRAL.c500),
            ])
        } else {
            Line::from(vec![
                Span::styled("Search: ", label_style),
                Span::styled(self.query.clone(), NEUTRAL.c200),
            ])
        };
        frame.render_widget(Paragraph::new(query), search_inner);

        if self.focus == ResultFocus::Search {
            let cursor_byte = text_byte_index(&self.query, self.query_cursor);
            let cursor = 8_usize.saturating_add(Line::from(&self.query[..cursor_byte]).width());
            if search_inner.width > 0 {
                frame.set_cursor_position(Position::new(
                    search_inner.x
                        + u16::try_from(cursor)
                            .unwrap_or(u16::MAX)
                            .min(search_inner.width - 1),
                    search_inner.y,
                ));
            }
        }

        let filter_block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(NEUTRAL.c700)
            .padding(Padding::horizontal(1));
        let filter_inner = filter_block.inner(filter);
        frame.render_widget(filter_block, filter);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Filter: ", NEUTRAL.c500),
                Span::styled(self.current_filter(), Style::new().fg(NEUTRAL.c50).bold()),
            ])),
            filter_inner,
        );
    }

    fn render_panes(&mut self, frame: &mut Frame<'_>, area: Rect, report: &Report) {
        let (list_area, detail_area) = pane_areas(area);

        if area.width < SPLIT_PANE_WIDTH && self.detail_open {
            self.render_detail(frame, list_area, report);
            return;
        }

        self.render_list(frame, list_area);

        if let Some(detail) = detail_area {
            self.render_detail(frame, detail, report);
        }
    }

    fn render_list(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let title = format!("Results ({})", self.items.len());

        render_pane(
            frame,
            area,
            &title,
            self.focus == ResultFocus::List,
            |frame, inner| {
                if self.items.is_empty() {
                    frame.render_widget(
                        Paragraph::new("No matching results.").style(NEUTRAL.c500),
                        inner,
                    );
                    return;
                }

                let visible_rows = usize::from(inner.height.saturating_sub(1)).max(1);
                self.ensure_selection_visible(visible_rows);
                let rows = self.items.iter().map(|item| {
                    Row::new(vec![
                        Cell::from(item.label()),
                        Cell::from(item.description()),
                    ])
                });
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(67), Constraint::Percentage(33)],
                )
                .header(Row::new([self.tab.label(), "Status"]).style(NEUTRAL.c500))
                .row_highlight_style(Style::new().fg(NEUTRAL.c50).bold())
                .column_spacing(1);
                let mut state = TableState::new()
                    .with_offset(self.list_offset)
                    .with_selected(Some(self.selected));

                frame.render_stateful_widget(table, inner, &mut state);
                self.list_offset = state.offset();

                render_scrollbar(
                    frame,
                    area,
                    self.items.len(),
                    visible_rows,
                    self.list_offset,
                    self.focus == ResultFocus::List,
                );
            },
        );
    }

    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect, report: &Report) {
        let title = self.items.get(self.selected).map_or_else(
            || "Details".to_owned(),
            |item| format!("Details: {}", item.kind()),
        );
        let lines = self.detail_lines(report);

        render_pane(
            frame,
            area,
            &title,
            self.focus == ResultFocus::Detail,
            |frame, inner| {
                let total = wrapped_line_count(&lines, inner.width);
                let visible = usize::from(inner.height).max(1);
                let maximum = total.saturating_sub(visible);

                self.detail_offset = self.detail_offset.min(maximum);
                frame.render_widget(
                    Paragraph::new(Text::from(
                        lines
                            .iter()
                            .map(|line| Line::from(line.as_str()))
                            .collect::<Vec<_>>(),
                    ))
                    .style(NEUTRAL.c200)
                    .wrap(Wrap { trim: false })
                    .scroll((u16::try_from(self.detail_offset).unwrap_or(u16::MAX), 0)),
                    inner,
                );

                render_scrollbar(
                    frame,
                    area,
                    total,
                    visible,
                    self.detail_offset,
                    self.focus == ResultFocus::Detail,
                );
            },
        );
    }

    fn render_overview(&mut self, frame: &mut Frame<'_>, area: Rect, report: &Report) {
        let values = [
            ("Pages", report.summary.pages),
            ("Issues", report.summary.issues.total),
            ("Links", report.summary.links.total),
            ("Images", report.summary.images.total),
        ];

        for (card, (label, value)) in
            overview_metric_areas(Rect::new(area.x, area.y, area.width, 3.min(area.height)))
                .into_iter()
                .zip(values)
        {
            let block = Block::new()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(NEUTRAL.c700)
                .padding(Padding::horizontal(1));
            let inner = block.inner(card);

            frame.render_widget(block, card);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{label}: "), NEUTRAL.c500),
                    Span::styled(value.to_string(), Style::new().fg(NEUTRAL.c50).bold()),
                ])),
                inner,
            );
        }

        let details_area = Rect::new(
            area.x,
            area.y.saturating_add(3),
            area.width,
            area.height.saturating_sub(3),
        );
        let lines = overview_lines(report);

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(NEUTRAL.c700)
            .padding(Padding::horizontal(1));
        let inner = block.inner(details_area);

        frame.render_widget(block, details_area);
        let total = wrapped_line_count(&lines, inner.width);
        let visible = usize::from(inner.height).max(1);

        self.overview_offset = self.overview_offset.min(total.saturating_sub(visible));
        frame.render_widget(
            Paragraph::new(Text::from(
                lines
                    .iter()
                    .map(|line| Line::from(line.as_str()))
                    .collect::<Vec<_>>(),
            ))
            .style(NEUTRAL.c200)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(self.overview_offset).unwrap_or(u16::MAX), 0)),
            inner,
        );

        render_scrollbar(
            frame,
            details_area,
            total,
            visible,
            self.overview_offset,
            false,
        );
    }

    fn change_tab(&mut self, tab: ResultTab, report: &Report) {
        self.tab = tab;
        self.query.clear();
        self.query_cursor = 0;
        self.selected = 0;
        self.list_offset = 0;
        self.detail_offset = 0;
        self.detail_open = false;
        self.list_scrollbar_interaction = ScrollBarInteraction::new();
        self.detail_scrollbar_interaction = ScrollBarInteraction::new();
        self.overview_scrollbar_interaction = ScrollBarInteraction::new();
        self.active_scrollbar = None;
        self.focus = if tab == ResultTab::Overview {
            ResultFocus::Detail
        } else {
            ResultFocus::List
        };

        self.refresh(report);
    }

    fn refresh(&mut self, report: &Report) {
        self.items = select_items(report, self.tab, &self.query, self.current_filter());
        if self.items.is_empty() {
            self.selected = 0;
            self.list_offset = 0;
        } else {
            self.selected = self.selected.min(self.items.len() - 1);
        }

        self.detail_offset = 0;
    }

    fn cycle_filter(&mut self) {
        let length = self.available_filters().len();
        let index = match self.tab {
            ResultTab::Issues => &mut self.filters.issues,
            ResultTab::Pages => &mut self.filters.pages,
            ResultTab::Links => &mut self.filters.links,
            ResultTab::Images => &mut self.filters.images,
            ResultTab::Overview => return,
        };

        *index = (*index + 1) % length;
        self.selected = 0;
        self.list_offset = 0;
    }

    fn current_filter(&self) -> &'static str {
        let (filters, index) = match self.tab {
            ResultTab::Issues => (self.available_filters(), self.filters.issues),
            ResultTab::Pages => (self.available_filters(), self.filters.pages),
            ResultTab::Links => (self.available_filters(), self.filters.links),
            ResultTab::Images => (self.available_filters(), self.filters.images),
            ResultTab::Overview => return "none",
        };
        filters[index]
    }

    fn available_filters(&self) -> &'static [&'static str] {
        match self.tab {
            ResultTab::Issues => &["all", "error", "warning", "info"],
            ResultTab::Pages => &["all", "healthy", "non-2xx", "failed"],
            ResultTab::Links => &[
                "all",
                "healthy",
                "broken",
                "blocked",
                "redirected",
                "skipped",
            ],
            ResultTab::Images => &[
                "all",
                "healthy",
                "broken",
                "blocked",
                "invalid",
                "redirected",
                "skipped",
            ],
            ResultTab::Overview => &["none"],
        }
    }

    fn tab_index(&self) -> usize {
        ResultTab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0)
    }

    fn remove_query_character(&mut self) -> bool {
        let start = text_byte_index(&self.query, self.query_cursor);
        let end = text_byte_index(&self.query, self.query_cursor + 1);

        if start < end {
            self.query.replace_range(start..end, "");
            true
        } else {
            false
        }
    }

    fn move_vertical(&mut self, delta: i32, area: Rect) {
        if self.tab == ResultTab::Overview {
            self.overview_offset = offset_by(self.overview_offset, delta);
        } else if self.focus == ResultFocus::Detail {
            self.detail_offset = offset_by(self.detail_offset, delta);
        } else if !self.items.is_empty() {
            self.selected = offset_by(self.selected, delta).min(self.items.len() - 1);
            let visible = usize::from(area.height.saturating_sub(RESULT_HEADER_ROWS + 3)).max(1);
            self.ensure_selection_visible(visible);
            self.detail_offset = 0;
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        if self.tab == ResultTab::Overview {
            self.overview_offset = if end { usize::MAX } else { 0 };
        } else if self.focus == ResultFocus::Detail {
            self.detail_offset = if end { usize::MAX } else { 0 };
        } else if !self.items.is_empty() {
            self.selected = if end { self.items.len() - 1 } else { 0 };
            self.detail_offset = 0;
        }
    }

    fn reset_selection(&mut self) {
        self.selected = 0;
        self.list_offset = 0;
        self.detail_offset = 0;
    }

    fn handle_scrollbars_mouse(&mut self, mouse: MouseEvent, report: &Report, area: Rect) -> bool {
        let target = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => None,
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                let Some(target) = self.active_scrollbar else {
                    return false;
                };
                Some(target)
            }
            _ => return false,
        };
        let releasing = matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left));

        if self.tab == ResultTab::Overview {
            if target.is_some_and(|target| target != ResultScrollbar::Overview) {
                if releasing {
                    self.active_scrollbar = None;
                }
                return false;
            }

            let overview_area = Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                area.height.saturating_sub(1),
            );
            let details_area = Rect::new(
                overview_area.x,
                overview_area.y.saturating_add(3),
                overview_area.width,
                overview_area.height.saturating_sub(3),
            );
            let inner = pane_inner(details_area);
            let total = wrapped_line_count(&overview_lines(report), inner.width);
            let visible = usize::from(inner.height).max(1);

            let handled = handle_scrollbar_mouse(
                mouse,
                details_area,
                total,
                visible,
                &mut self.overview_offset,
                &mut self.overview_scrollbar_interaction,
            );
            if handled && target.is_none() {
                self.active_scrollbar = Some(ResultScrollbar::Overview);
            }
            if releasing {
                self.active_scrollbar = None;
            }
            return handled;
        }

        if target == Some(ResultScrollbar::Overview) {
            if releasing {
                self.active_scrollbar = None;
            }
            return false;
        }

        let panes = Rect::new(
            area.x,
            area.y.saturating_add(RESULT_HEADER_ROWS),
            area.width,
            area.height.saturating_sub(RESULT_HEADER_ROWS),
        );
        let (list_area, detail_area) = pane_areas(panes);

        if area.width < SPLIT_PANE_WIDTH && self.detail_open {
            if target == Some(ResultScrollbar::List) {
                if releasing {
                    self.active_scrollbar = None;
                }
                return false;
            }

            let inner = pane_inner(list_area);
            let total = wrapped_line_count(&self.detail_lines(report), inner.width);
            let visible = usize::from(inner.height).max(1);
            let handled = handle_scrollbar_mouse(
                mouse,
                list_area,
                total,
                visible,
                &mut self.detail_offset,
                &mut self.detail_scrollbar_interaction,
            );
            if handled {
                self.focus = ResultFocus::Detail;
                if target.is_none() {
                    self.active_scrollbar = Some(ResultScrollbar::Detail);
                }
            }
            if releasing {
                self.active_scrollbar = None;
            }
            return handled;
        }

        let list_visible = usize::from(pane_inner(list_area).height.saturating_sub(1)).max(1);
        let previous_list_offset = self.list_offset;
        let list_handled = target != Some(ResultScrollbar::Detail)
            && handle_scrollbar_mouse(
                mouse,
                list_area,
                self.items.len(),
                list_visible,
                &mut self.list_offset,
                &mut self.list_scrollbar_interaction,
            );

        if list_handled {
            self.focus = ResultFocus::List;

            if target.is_none() {
                self.active_scrollbar = Some(ResultScrollbar::List);
            }

            if previous_list_offset != self.list_offset && !self.items.is_empty() {
                let selected_row = self
                    .selected
                    .saturating_sub(previous_list_offset)
                    .min(list_visible.saturating_sub(1));
                self.selected = (self.list_offset + selected_row).min(self.items.len() - 1);
                self.detail_offset = 0;
            }
        }

        let detail_handled = if target != Some(ResultScrollbar::List)
            && let Some(detail_area) = detail_area
        {
            let inner = pane_inner(detail_area);
            let total = wrapped_line_count(&self.detail_lines(report), inner.width);
            let visible = usize::from(inner.height).max(1);
            let handled = handle_scrollbar_mouse(
                mouse,
                detail_area,
                total,
                visible,
                &mut self.detail_offset,
                &mut self.detail_scrollbar_interaction,
            );

            if handled {
                self.focus = ResultFocus::Detail;
                if target.is_none() {
                    self.active_scrollbar = Some(ResultScrollbar::Detail);
                }
            }

            handled
        } else {
            false
        };

        if releasing {
            self.active_scrollbar = None;
        }

        list_handled || detail_handled
    }

    fn scroll_mouse(&mut self, delta: i32, x: u16, y: u16, area: Rect) {
        if self.tab == ResultTab::Overview {
            if y > 0 {
                self.overview_offset = offset_by(self.overview_offset, delta);
            }
            return;
        }

        if y < RESULT_HEADER_ROWS {
            return;
        }

        let panes = Rect::new(
            0,
            RESULT_HEADER_ROWS,
            area.width,
            area.height.saturating_sub(RESULT_HEADER_ROWS),
        );
        let (list, detail) = pane_areas(panes);
        let point = Position::new(x, y);
        if list.contains(point) && !(area.width < SPLIT_PANE_WIDTH && self.detail_open) {
            self.focus = ResultFocus::List;
            if !self.items.is_empty() {
                self.selected = offset_by(self.selected, delta).min(self.items.len() - 1);
                self.detail_offset = 0;
            }
        } else if detail.is_some_and(|detail| detail.contains(point))
            || (area.width < SPLIT_PANE_WIDTH && self.detail_open)
        {
            self.focus = ResultFocus::Detail;
            self.detail_offset = offset_by(self.detail_offset, delta);
        }
    }

    fn ensure_selection_visible(&mut self, visible: usize) {
        if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if self.selected >= self.list_offset + visible {
            self.list_offset = self.selected + 1 - visible;
        }
    }

    fn detail_lines(&self, report: &Report) -> Vec<String> {
        self.items.get(self.selected).map_or_else(
            || vec!["No item selected.".to_owned()],
            |item| create_detail_lines(item, report),
        )
    }
}

fn tab_at(x: u16) -> Option<ResultTab> {
    let mut left = TAB_PADDING;

    for (index, tab) in ResultTab::ALL.into_iter().enumerate() {
        let width = (format!("{} {}", index + 1, tab.label()).chars().count() as u16).max(1);
        if x >= left && x < left.saturating_add(width) {
            return Some(tab);
        }
        left = left.saturating_add(width + 2);
    }
    None
}

fn pane_areas(area: Rect) -> (Rect, Option<Rect>) {
    if area.width < SPLIT_PANE_WIDTH {
        return (area, None);
    }

    let left_width = area.width.saturating_sub(1) / 2;
    let left = Rect::new(area.x, area.y, left_width, area.height);
    let right = Rect::new(
        left.right().saturating_add(1),
        area.y,
        area.right().saturating_sub(left.right().saturating_add(1)),
        area.height,
    );

    (left, Some(right))
}

fn overview_metric_areas(area: Rect) -> [Rect; 4] {
    const CARD_COUNT: u16 = 4;
    const GAP_COUNT: u16 = CARD_COUNT - 1;

    let available = area.width.saturating_sub(GAP_COUNT);
    let width = available / CARD_COUNT;
    let extra = available % CARD_COUNT;
    let mut x = area.x;

    std::array::from_fn(|index| {
        let card_width = width + u16::from((index as u16) < extra);
        let card = Rect::new(x, area.y, card_width, area.height);
        x = x.saturating_add(card_width).saturating_add(1);
        card
    })
}

fn select_items(report: &Report, tab: ResultTab, query: &str, filter: &str) -> Vec<ResultItem> {
    let query = query.trim().to_lowercase();
    let includes = |values: &[&str]| {
        query.is_empty()
            || values
                .iter()
                .any(|value| value.to_lowercase().contains(&query))
    };

    match tab {
        ResultTab::Overview => Vec::new(),
        ResultTab::Issues => report
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| filter == "all" || issue.severity.as_str() == filter)
            .filter(|(_, issue)| {
                includes(&[&issue.message, issue.code.as_str(), &issue.target.url])
            })
            .map(|(index, issue)| ResultItem::issue(index, issue))
            .collect(),
        ResultTab::Pages => report
            .pages
            .iter()
            .enumerate()
            .filter(|(_, page)| matches_page_filter(page, filter))
            .filter(|(_, page)| {
                includes(&[
                    &page.url,
                    page.title.as_deref().unwrap_or_default(),
                    &page
                        .status_code
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ])
            })
            .map(|(index, page)| ResultItem::page(index, page))
            .collect(),
        ResultTab::Links => report
            .links
            .iter()
            .enumerate()
            .filter(|(_, link)| matches_link_filter(link, filter))
            .filter(|(_, link)| includes(&[&link.url, &describe_link(link)]))
            .map(|(index, link)| ResultItem::link(index, link))
            .collect(),
        ResultTab::Images => report
            .images
            .iter()
            .enumerate()
            .filter(|(_, image)| matches_image_filter(image, filter))
            .filter(|(_, image)| includes(&[&image.url, &describe_image(image)]))
            .map(|(index, image)| ResultItem::image(index, image))
            .collect(),
    }
}

fn matches_page_filter(page: &Page, filter: &str) -> bool {
    match filter {
        "failed" => page.status_code.is_none(),
        "healthy" => page
            .status_code
            .is_some_and(|status| (200..300).contains(&status)),
        "non-2xx" => page
            .status_code
            .is_some_and(|status| !(200..300).contains(&status)),
        _ => true,
    }
}

fn matches_link_filter(link: &Link, filter: &str) -> bool {
    let blocked = link.result.kind == ResultKind::Blocked;
    let skipped = link.result.kind == ResultKind::Skipped;
    match filter {
        "healthy" => !link.is_broken() && !blocked && !link.is_redirected() && !skipped,
        "broken" => link.is_broken(),
        "blocked" => blocked,
        "redirected" => link.is_redirected(),
        "skipped" => skipped,
        _ => true,
    }
}

fn matches_image_filter(image: &Image, filter: &str) -> bool {
    let blocked = image.result.kind == ResultKind::Blocked;
    let skipped = image.result.kind == ResultKind::Skipped;
    match filter {
        "healthy" => {
            !image.is_broken()
                && !blocked
                && !image.is_invalid()
                && !image.is_redirected()
                && !skipped
        }
        "broken" => image.is_broken(),
        "blocked" => blocked,
        "invalid" => image.is_invalid(),
        "redirected" => image.is_redirected(),
        "skipped" => skipped,
        _ => true,
    }
}

fn describe_link(link: &Link) -> String {
    describe_resource(
        link.result.kind,
        link.result.status_code,
        link.result.final_url.as_deref(),
        link.result.reason,
        None,
        link.is_redirected(),
    )
}

fn describe_image(image: &Image) -> String {
    describe_resource(
        image.result.kind,
        image.result.status_code,
        image.result.final_url.as_deref(),
        image.result.reason,
        Some(
            image
                .result
                .content_type
                .as_deref()
                .unwrap_or("(missing Content-Type)"),
        ),
        image.is_redirected(),
    )
}

fn describe_resource(
    kind: ResultKind,
    status: Option<u16>,
    final_url: Option<&str>,
    reason: Option<FailureReason>,
    content_type: Option<&str>,
    redirected: bool,
) -> String {
    let mut description = match kind {
        ResultKind::Response => format!(
            "HTTP {}{}",
            status.map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
            content_type.map_or_else(String::new, |value| format!(" {value}")),
        ),
        ResultKind::Blocked => format!(
            "BLOCKED: {} (HTTP {})",
            reason.map_or("unknown", FailureReason::as_str),
            status.map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
        ),
        other => format!(
            "{}: {}",
            other.as_str(),
            reason.map_or("unknown", FailureReason::as_str)
        ),
    };

    if redirected {
        description.push_str(" -> ");
        description.push_str(final_url.unwrap_or_default());
    }

    description
}

fn create_detail_lines(item: &ResultItem, report: &Report) -> Vec<String> {
    match item.source {
        ResultSource::Issue(index) => {
            let issue = &report.issues[index];
            let mut lines = vec![
                issue.message.clone(),
                String::new(),
                format!("Severity: {}", issue.severity.as_str()),
                format!("Code: {}", issue.code.as_str()),
                format!("Target: {}", issue.target.target_type.as_str()),
                format!("URL: {}", issue.target.url),
            ];

            match issue.target.target_type {
                TargetType::Link => {
                    if let Some(link) = report
                        .links
                        .iter()
                        .find(|link| link.url == issue.target.url)
                    {
                        lines.extend(link_occurrence_lines(&link.found_on));
                    }
                }
                TargetType::Image => {
                    if let Some(image) = report
                        .images
                        .iter()
                        .find(|image| image.url == issue.target.url)
                    {
                        lines.extend(image_occurrence_lines(&image.found_on));
                    }
                }
                TargetType::Page => {}
            }

            lines
        }
        ResultSource::Page(index) => {
            let page = &report.pages[index];
            vec![
                page.title
                    .clone()
                    .unwrap_or_else(|| "(untitled page)".to_owned()),
                String::new(),
                format!("URL: {}", page.url),
                format!("Depth: {}", page.depth),
                format!(
                    "Status: {}",
                    page.status_code
                        .map_or_else(|| "failed".to_owned(), |value| value.to_string())
                ),
                format_option("Content-Type", page.content_type.as_deref(), "unknown"),
                format_option("Description", page.description.as_deref(), "(missing)"),
                format!(
                    "H1: {}",
                    if page.headings.h1.is_empty() {
                        "(missing)".to_owned()
                    } else {
                        page.headings.h1.join(" | ")
                    }
                ),
                format!("Images: {}", page.images.len()),
                String::new(),
                "Open Graph".to_owned(),
                format_option("Title", page.open_graph.title.as_deref(), "(missing)"),
                format_option(
                    "Description",
                    page.open_graph.description.as_deref(),
                    "(missing)",
                ),
                format_option("Image", page.open_graph.image.as_deref(), "(missing)"),
                format_option("URL", page.open_graph.url.as_deref(), "(missing)"),
                format_option("Type", page.open_graph.object_type.as_deref(), "(missing)"),
                format_option(
                    "Site name",
                    page.open_graph.site_name.as_deref(),
                    "(missing)",
                ),
                format_option("Locale", page.open_graph.locale.as_deref(), "(missing)"),
            ]
        }
        ResultSource::Link(index) => {
            let link = &report.links[index];
            let mut lines = vec![link.url.clone(), String::new(), describe_link(link)];

            add_result_lines(
                &mut lines,
                link.result.kind,
                link.result.status_code,
                link.result.final_url.as_deref(),
                link.result.reason,
                None,
            );

            lines.extend(link_occurrence_lines(&link.found_on));
            lines
        }
        ResultSource::Image(index) => {
            let image = &report.images[index];
            let mut lines = vec![image.url.clone(), String::new(), describe_image(image)];

            add_result_lines(
                &mut lines,
                image.result.kind,
                image.result.status_code,
                image.result.final_url.as_deref(),
                image.result.reason,
                Some(image.result.content_type.as_deref().unwrap_or("(missing)")),
            );

            lines.extend(image_occurrence_lines(&image.found_on));
            lines
        }
    }
}

fn add_result_lines(
    lines: &mut Vec<String>,
    kind: ResultKind,
    status: Option<u16>,
    final_url: Option<&str>,
    reason: Option<FailureReason>,
    content_type: Option<&str>,
) {
    if matches!(kind, ResultKind::Response | ResultKind::Blocked) {
        lines.push(format!(
            "Status: {}",
            status.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
        ));
        lines.push(format!("Final URL: {}", final_url.unwrap_or("(missing)")));
        if let Some(content_type) = content_type {
            lines.push(format!("Content-Type: {content_type}"));
        }
    }

    if kind != ResultKind::Response {
        lines.push(format!(
            "Reason: {}",
            reason.map_or("unknown", FailureReason::as_str)
        ));
    }
}

fn link_occurrence_lines(occurrences: &[LinkOccurrence]) -> Vec<String> {
    let mut lines = Vec::new();

    if !occurrences.is_empty() {
        lines.extend([String::new(), "Found on:".to_owned()]);
    }

    for occurrence in occurrences {
        lines.extend([
            format!("- {}", occurrence.page_url),
            format!("  Original: {}", occurrence.original_url),
            format!(
                "  <{}> {}",
                occurrence.element,
                if occurrence.text.is_empty() {
                    "(no text)"
                } else {
                    &occurrence.text
                }
            ),
        ]);
    }

    lines
}

fn image_occurrence_lines(occurrences: &[ImageOccurrence]) -> Vec<String> {
    let mut lines = Vec::new();

    if !occurrences.is_empty() {
        lines.extend([String::new(), "Found on:".to_owned()]);
    }

    for occurrence in occurrences {
        lines.extend([
            format!("- {}", occurrence.page_url),
            format!("  Original: {}", occurrence.original_url),
            format!(
                "  Source: <{}> {}",
                occurrence.element, occurrence.attribute
            ),
            format!(
                "  Descriptor: {}",
                occurrence.descriptor.as_deref().unwrap_or("(none)")
            ),
            format!(
                "  Alt: {}",
                occurrence.alt.as_deref().unwrap_or("(missing)")
            ),
        ]);
    }

    lines
}

fn format_option(label: &str, value: Option<&str>, fallback: &str) -> String {
    format!("{label}: {}", value.unwrap_or(fallback))
}

fn overview_lines(report: &Report) -> Vec<String> {
    vec![
        "Issue severity".to_owned(),
        format!("  Error: {}", report.summary.issues.error),
        format!("  Warning: {}", report.summary.issues.warning),
        format!("  Info: {}", report.summary.issues.info),
        String::new(),
        "Link summary".to_owned(),
        format!("  Checked: {}", report.summary.links.checked),
        format!("  Broken: {}", report.summary.links.broken),
        format!("  Blocked: {}", report.summary.links.blocked),
        format!("  Redirected: {}", report.summary.links.redirected),
        String::new(),
        "Image summary".to_owned(),
        format!("  Checked: {}", report.summary.images.checked),
        format!("  Broken: {}", report.summary.images.broken),
        format!("  Blocked: {}", report.summary.images.blocked),
        format!("  Invalid: {}", report.summary.images.invalid),
        format!("  Redirected: {}", report.summary.images.redirected),
        String::new(),
        format!("Audited {}", report.url),
        format!("Completed {}", completed_timestamp(report.audited_at)),
    ]
}

fn completed_timestamp(value: time::OffsetDateTime) -> String {
    let zone = if value.offset().is_utc() {
        "UTC".to_owned()
    } else {
        value.offset().to_string()
    };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {zone}",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
    )
}

fn wrapped_line_count(lines: &[String], width: u16) -> usize {
    let width = usize::from(width).max(1);

    lines
        .iter()
        .map(|line| Line::from(line.as_str()).width().max(1).div_ceil(width))
        .sum()
}

fn offset_by(offset: usize, delta: i32) -> usize {
    if delta < 0 {
        offset.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        offset.saturating_add(delta as usize)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    use scoutly::{
        FailureReason, Image, ImageResult, Issue, IssueCode, IssueTarget, Link, LinkResult,
        ResultKind, Severity, TargetType,
    };

    use super::{
        ResultFocus, ResultItem, ResultTab, ResultsAction, ResultsState, completed_timestamp,
        create_detail_lines, describe_image, describe_link, matches_image_filter,
        matches_link_filter, matches_page_filter, offset_by, overview_lines, pane_areas,
        select_items, tab_at, wrapped_line_count,
    };
    use crate::tui::tests::{large_report, sample_report};

    fn varied_report() -> scoutly::Report {
        let mut report = sample_report();

        let mut failed_page = report.pages[0].clone();

        failed_page.url = "https://example.com/failed".to_owned();
        failed_page.title = None;
        failed_page.status_code = None;
        failed_page.content_type = None;
        failed_page.description = None;
        failed_page.headings.h1.clear();

        report.pages.push(failed_page);

        let mut non_success_page = report.pages[0].clone();

        non_success_page.url = "https://example.com/error".to_owned();
        non_success_page.status_code = Some(503);

        report.pages.push(non_success_page);

        report.links.extend([
            Link {
                url: "https://example.com/blocked".to_owned(),
                result: LinkResult {
                    kind: ResultKind::Blocked,
                    status_code: Some(403),
                    final_url: Some("https://example.com/challenge".to_owned()),
                    reason: Some(FailureReason::AntiBotChallenge),
                },
                found_on: Vec::new(),
            },
            Link {
                url: "mailto:test@example.com".to_owned(),
                result: LinkResult {
                    kind: ResultKind::Skipped,
                    status_code: None,
                    final_url: None,
                    reason: Some(FailureReason::UnsupportedProtocol),
                },
                found_on: Vec::new(),
            },
            Link {
                url: "https://example.com/old".to_owned(),
                result: LinkResult {
                    kind: ResultKind::Response,
                    status_code: Some(301),
                    final_url: Some("https://example.com/new".to_owned()),
                    reason: None,
                },
                found_on: Vec::new(),
            },
            Link {
                url: "https://example.com/timeout".to_owned(),
                result: LinkResult {
                    kind: ResultKind::Failed,
                    status_code: None,
                    final_url: None,
                    reason: Some(FailureReason::RequestTimedOut),
                },
                found_on: vec![scoutly::LinkOccurrence {
                    page_url: "https://example.com/".to_owned(),
                    original_url: "/timeout".to_owned(),
                    element: "a".to_owned(),
                    text: String::new(),
                }],
            },
            Link {
                url: "https://example.com/healthy".to_owned(),
                result: LinkResult {
                    kind: ResultKind::Response,
                    status_code: Some(200),
                    final_url: Some("https://example.com/healthy".to_owned()),
                    reason: None,
                },
                found_on: Vec::new(),
            },
        ]);

        report.images.extend([
            Image {
                url: "https://example.com/blocked.png".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Blocked,
                    status_code: Some(403),
                    final_url: Some("https://example.com/challenge.png".to_owned()),
                    content_type: Some("image/png".to_owned()),
                    reason: Some(FailureReason::AntiBotChallenge),
                },
                found_on: Vec::new(),
            },
            Image {
                url: "data:image/png;base64,AA".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Skipped,
                    status_code: None,
                    final_url: None,
                    content_type: None,
                    reason: Some(FailureReason::UnsupportedProtocol),
                },
                found_on: Vec::new(),
            },
            Image {
                url: "bad image".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Invalid,
                    status_code: None,
                    final_url: None,
                    content_type: None,
                    reason: Some(FailureReason::InvalidUrl),
                },
                found_on: Vec::new(),
            },
            Image {
                url: "https://example.com/missing.png".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Response,
                    status_code: Some(404),
                    final_url: None,
                    content_type: Some("image/png".to_owned()),
                    reason: None,
                },
                found_on: Vec::new(),
            },
            Image {
                url: "https://example.com/old.png".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Response,
                    status_code: Some(200),
                    final_url: Some("https://example.com/new.png".to_owned()),
                    content_type: Some("image/png".to_owned()),
                    reason: None,
                },
                found_on: vec![scoutly::ImageOccurrence {
                    page_url: "https://example.com/".to_owned(),
                    original_url: "/old.png".to_owned(),
                    element: "source".to_owned(),
                    attribute: "srcset".to_owned(),
                    descriptor: Some("2x".to_owned()),
                    alt: Some("Large image".to_owned()),
                }],
            },
            Image {
                url: "https://example.com/healthy.png".to_owned(),
                result: ImageResult {
                    kind: ResultKind::Response,
                    status_code: Some(200),
                    final_url: Some("https://example.com/healthy.png".to_owned()),
                    content_type: Some("image/png".to_owned()),
                    reason: None,
                },
                found_on: Vec::new(),
            },
        ]);

        report.issues.extend([
            Issue {
                code: IssueCode::Redirect,
                severity: Severity::Info,
                message: "Link redirected".to_owned(),
                target: IssueTarget {
                    target_type: TargetType::Link,
                    url: "https://example.com/old".to_owned(),
                },
            },
            Issue {
                code: IssueCode::MissingTitle,
                severity: Severity::Warning,
                message: "Page is missing a title".to_owned(),
                target: IssueTarget {
                    target_type: TargetType::Page,
                    url: "https://example.com/failed".to_owned(),
                },
            },
        ]);

        report.refresh_summary();

        report
    }

    #[test]
    fn keyboard_switches_tabs_filters_search_and_narrow_detail() {
        let report = sample_report();
        let mut state = ResultsState::new(&report);
        let area = ratatui::layout::Rect::new(0, 0, 78, 24);

        state.handle_key(
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Issues);

        state.handle_key(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
            &report,
            area,
        );
        assert_eq!(state.current_filter(), "error");

        state.handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &report,
            area,
        );
        assert!(state.detail_open);

        state.handle_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &report,
            area,
        );
        assert!(!state.detail_open);

        assert_eq!(
            state.handle_key(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                &report,
                area
            ),
            ResultsAction::NewAudit
        );
    }

    #[test]
    fn search_and_selection_keep_deterministic_report_order() {
        let report = large_report(30);
        let mut state = ResultsState::new(&report);
        let area = ratatui::layout::Rect::new(0, 0, 118, 20);

        state.change_tab(ResultTab::Links, &report);

        for _ in 0..5 {
            state.handle_key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &report,
                area,
            );
        }
        assert_eq!(state.selected, 5);

        state.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &report,
            area,
        );

        for character in "link-2".chars() {
            state.handle_key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &report,
                area,
            );
        }
        assert_eq!(state.items.len(), 10);
        assert_eq!(state.selected, 0);

        state.handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &report,
            area,
        );
        state.handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &report,
            area,
        );
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn mouse_routes_tabs_controls_rows_and_split_panes() {
        let report = large_report(30);
        let mut state = ResultsState::new(&report);
        let area = ratatui::layout::Rect::new(1, 3, 118, 26);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 37,
                row: 3,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Links);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.focus, super::ResultFocus::Search);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 110,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.current_filter(), "healthy");

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 9,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.selected, 0);
        assert_eq!(state.focus, super::ResultFocus::List);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 70,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.focus, super::ResultFocus::Detail);
        assert!(state.detail_offset > 0);
    }

    #[test]
    fn result_list_scrollbar_drag_updates_offset_and_selection() {
        let report = large_report(100);
        let mut state = ResultsState::new(&report);
        let area = ratatui::layout::Rect::new(1, 3, 118, 26);

        state.change_tab(ResultTab::Links, &report);

        let panes = ratatui::layout::Rect::new(
            area.x,
            area.y + super::RESULT_HEADER_ROWS,
            area.width,
            area.height - super::RESULT_HEADER_ROWS,
        );
        let (list_area, _) = super::pane_areas(panes);
        let scrollbar_column = list_area.right() - 1;

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: scrollbar_column,
                row: list_area.y + 1,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: scrollbar_column,
                row: list_area.bottom() - 2,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );

        assert!(state.list_offset > 0);
        assert!(state.selected >= state.list_offset);
        assert_eq!(state.focus, super::ResultFocus::List);
    }

    #[test]
    fn overview_metric_clicks_ignore_the_inter_card_gap() {
        let report = sample_report();
        let area = ratatui::layout::Rect::new(1, 3, 98, 26);
        let mut state = ResultsState::new(&report);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 25,
                row: 4,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Overview);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 4,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Pages);
    }

    #[test]
    fn image_details_preserve_missing_content_type_and_target_occurrence_kind() {
        let mut report = sample_report();

        let image_url = report.images[0].url.clone();
        report.images[0].result.content_type = None;
        report.links[0].url.clone_from(&image_url);

        assert_eq!(
            describe_image(&report.images[0]),
            "HTTP 200 (missing Content-Type)"
        );
        let image_lines = create_detail_lines(&ResultItem::image(0, &report.images[0]), &report);

        assert!(
            image_lines
                .iter()
                .any(|line| line == "Content-Type: (missing)")
        );

        let (issue_index, issue) = report
            .issues
            .iter()
            .enumerate()
            .find(|(_, issue)| issue.target.target_type == scoutly::TargetType::Image)
            .unwrap();
        let issue_lines = create_detail_lines(&ResultItem::issue(issue_index, issue), &report);

        assert!(issue_lines.iter().any(|line| line == "  Source: <img> src"));
        assert!(!issue_lines.iter().any(|line| line == "  <a> Broken"));
    }

    #[test]
    fn resource_filters_and_descriptions_cover_all_result_states() {
        let report = varied_report();

        for filter in ["all", "healthy", "non-2xx", "failed"] {
            assert!(
                report
                    .pages
                    .iter()
                    .any(|page| matches_page_filter(page, filter))
            );
        }

        for filter in [
            "all",
            "healthy",
            "broken",
            "blocked",
            "redirected",
            "skipped",
        ] {
            assert!(
                report
                    .links
                    .iter()
                    .any(|link| matches_link_filter(link, filter)),
                "missing link filter {filter}"
            );
        }

        for filter in [
            "all",
            "healthy",
            "broken",
            "blocked",
            "invalid",
            "redirected",
            "skipped",
        ] {
            assert!(
                report
                    .images
                    .iter()
                    .any(|image| matches_image_filter(image, filter)),
                "missing image filter {filter}"
            );
        }

        assert!(describe_link(&report.links[1]).starts_with("BLOCKED:"));
        assert!(describe_link(&report.links[2]).starts_with("skipped:"));
        assert!(describe_image(&report.images[5]).contains("->"));

        assert_eq!(
            select_items(&report, ResultTab::Overview, "", "none").len(),
            0
        );
        assert_eq!(
            select_items(&report, ResultTab::Pages, "FAILED", "all").len(),
            1
        );
        assert_eq!(
            select_items(&report, ResultTab::Links, "timeout", "all").len(),
            1
        );
        assert_eq!(
            select_items(&report, ResultTab::Images, "INVALID", "all").len(),
            1
        );
        assert_eq!(
            select_items(&report, ResultTab::Issues, "redirect", "info").len(),
            1
        );
    }

    #[test]
    fn detail_lines_cover_pages_resources_occurrences_and_empty_selection() {
        let report = varied_report();
        let failed_page = create_detail_lines(&ResultItem::page(1, &report.pages[1]), &report);

        assert!(failed_page.iter().any(|line| line == "(untitled page)"));
        assert!(failed_page.iter().any(|line| line == "Status: failed"));
        assert!(failed_page.iter().any(|line| line == "H1: (missing)"));

        let failed_link = create_detail_lines(&ResultItem::link(4, &report.links[4]), &report);

        assert!(
            failed_link
                .iter()
                .any(|line| line == "Reason: request-timed-out")
        );
        assert!(failed_link.iter().any(|line| line == "  <a> (no text)"));

        let redirected_image =
            create_detail_lines(&ResultItem::image(5, &report.images[5]), &report);

        assert!(
            redirected_image
                .iter()
                .any(|line| line == "  Descriptor: 2x")
        );
        assert!(
            redirected_image
                .iter()
                .any(|line| line == "  Alt: Large image")
        );

        let page_issue = create_detail_lines(&ResultItem::issue(3, &report.issues[3]), &report);

        assert!(page_issue.iter().any(|line| line == "Target: page"));

        let mut empty = ResultsState::new(&scoutly::Report {
            pages: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            issues: Vec::new(),
            ..report.clone()
        });

        empty.change_tab(ResultTab::Pages, &report);
        empty.items.clear();

        assert_eq!(empty.detail_lines(&report), vec!["No item selected."]);
    }

    #[test]
    fn keyboard_navigation_edits_unicode_search_and_visits_every_tab() {
        let report = varied_report();
        let area = Rect::new(0, 0, 80, 14);
        let mut state = ResultsState::new(&report);

        state.handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Images);

        state.handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &report,
            area,
        );
        assert_eq!(state.tab, ResultTab::Overview);

        for (key, tab) in [
            ('2', ResultTab::Issues),
            ('3', ResultTab::Pages),
            ('4', ResultTab::Links),
            ('5', ResultTab::Images),
        ] {
            state.handle_key(
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                &report,
                area,
            );
            assert_eq!(state.tab, tab);
        }

        state.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &report,
            area,
        );

        for character in "圖ab".chars() {
            state.handle_key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &report,
                area,
            );
        }

        state.handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &report,
            area,
        );

        assert_eq!(state.query, "圖b");

        state.handle_key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &report,
            area,
        );

        assert_eq!(state.query, "b");

        state.handle_key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &report,
            area,
        );

        assert_eq!(state.focus, ResultFocus::List);

        state.handle_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &report,
            area,
        );

        assert!(state.query.is_empty());

        state.handle_key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &report,
            area,
        );

        state.handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &report,
            area,
        );

        assert_eq!(
            state.handle_key(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                &report,
                area
            ),
            ResultsAction::Quit
        );
    }

    #[test]
    fn result_rendering_covers_overview_lists_search_and_narrow_details() {
        let report = varied_report();

        for (tab, width) in [
            (ResultTab::Overview, 120),
            (ResultTab::Issues, 120),
            (ResultTab::Pages, 120),
            (ResultTab::Links, 120),
            (ResultTab::Images, 80),
        ] {
            let backend = TestBackend::new(width, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut state = ResultsState::new(&report);

            state.change_tab(tab, &report);

            if tab != ResultTab::Overview {
                state.focus = ResultFocus::Search;
                state.query = "example".to_owned();
                state.query_cursor = state.query.chars().count();
                state.refresh(&report);
            }

            if width < 100 {
                state.detail_open = true;
                state.focus = ResultFocus::Detail;
            }

            terminal
                .draw(|frame| state.render(frame, Rect::new(0, 0, width, 24), &report))
                .unwrap();

            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .any(|cell| !cell.symbol().is_empty())
            );
        }
    }

    #[test]
    fn geometry_timestamp_wrapping_and_offsets_handle_edges() {
        assert_eq!(tab_at(1), Some(ResultTab::Overview));
        assert_eq!(tab_at(u16::MAX), None);
        assert_eq!(pane_areas(Rect::new(2, 3, 99, 10)).1, None);
        assert!(pane_areas(Rect::new(2, 3, 100, 10)).1.is_some());
        assert_eq!(offset_by(2, -5), 0);
        assert_eq!(offset_by(2, 5), 7);
        assert_eq!(
            wrapped_line_count(&["abcdef".to_owned(), String::new()], 3),
            3
        );
        assert_eq!(
            completed_timestamp(time::OffsetDateTime::UNIX_EPOCH),
            "1970-01-01 00:00:00 UTC"
        );

        let offset = time::UtcOffset::from_hms(8, 0, 0).unwrap();

        assert!(
            completed_timestamp(time::OffsetDateTime::UNIX_EPOCH.to_offset(offset))
                .ends_with("+08:00:00")
        );
        assert!(
            overview_lines(&sample_report())
                .iter()
                .any(|line| line.starts_with("Audited "))
        );
    }

    #[test]
    fn result_state_edge_navigation_covers_focus_filters_and_empty_rendering() {
        let report = varied_report();
        let narrow = Rect::new(0, 0, 80, 14);
        let wide = Rect::new(0, 0, 120, 20);
        let mut state = ResultsState::new(&report);

        state.change_tab(ResultTab::Links, &report);
        state.handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &report,
            wide,
        );
        assert_eq!(state.focus, ResultFocus::Detail);

        state.handle_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &report,
            wide,
        );
        assert_eq!(state.focus, ResultFocus::List);

        state.handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &report,
            wide,
        );
        state.handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &report,
            wide,
        );
        assert_eq!(state.focus, ResultFocus::List);

        state.handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &report,
            narrow,
        );
        assert!(state.detail_open);

        state.handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &report,
            narrow,
        );
        assert!(!state.detail_open);

        state.handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &report,
            narrow,
        );
        state.handle_key(
            KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
            &report,
            narrow,
        );

        state.focus = ResultFocus::Search;
        state.query = "x".to_owned();
        state.query_cursor = 0;
        state.handle_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &report,
            wide,
        );
        assert_eq!(state.query, "x");

        state.change_tab(ResultTab::Pages, &report);
        state.cycle_filter();
        assert_eq!(state.current_filter(), "healthy");
        state.change_tab(ResultTab::Images, &report);
        state.cycle_filter();
        assert_eq!(state.current_filter(), "healthy");
        state.change_tab(ResultTab::Overview, &report);
        state.cycle_filter();
        assert_eq!(state.current_filter(), "none");
        state.move_vertical(3, wide);
        assert_eq!(state.overview_offset, 3);
        state.move_to_edge(true);
        assert_eq!(state.overview_offset, usize::MAX);
        state.move_to_edge(false);
        assert_eq!(state.overview_offset, 0);

        state.change_tab(ResultTab::Pages, &report);
        state.items.clear();
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| state.render(frame, wide, &report))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("No matching results."));
        assert!(rendered.contains("Details"));
    }

    #[test]
    fn result_mouse_edges_cover_outside_scroll_and_narrow_detail_routing() {
        let report = varied_report();
        let area = Rect::new(1, 3, 78, 20);
        let mut state = ResultsState::new(&report);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 100,
                row: 100,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 2,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Moved,
                column: 2,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );

        state.change_tab(ResultTab::Links, &report);
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: area.y + super::RESULT_HEADER_ROWS + 2,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert!(state.detail_open);
        assert_eq!(state.focus, ResultFocus::Detail);

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 2,
                row: area.y + super::RESULT_HEADER_ROWS + 2,
                modifiers: KeyModifiers::NONE,
            },
            &report,
            area,
        );
        assert!(state.detail_offset > 0);
    }
}
