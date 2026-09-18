use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{
    Style,
    palette::tailwind::{NEUTRAL, RED},
};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use scoutly::{Options, parse_target};
use tui_scrollbar::ScrollBarInteraction;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::text_byte_index;
use super::title_case;
use super::{handle_scrollbar_mouse, render_scrollbar};

const GRID_COLUMNS: usize = 2;
const FIELD_ROWS: usize = 3;
const CELL_PADDING_COLUMNS: usize = 2;
const INPUT_PREFIX_COLUMNS: usize = 2;

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SetupField {
    Url,
    MaxDepth,
    MaxPages,
    MaxSitemapDocuments,
    Timeout,
    MaxRedirects,
    Concurrency,
    UserAgent,
    RateLimit,
    KeepFragments,
    IgnoreRedirects,
    RespectRobots,
    Sitemaps,
    Images,
    Run,
}

impl SetupField {
    const ALL: [Self; 15] = [
        Self::Url,
        Self::MaxDepth,
        Self::MaxPages,
        Self::MaxSitemapDocuments,
        Self::Timeout,
        Self::MaxRedirects,
        Self::Concurrency,
        Self::UserAgent,
        Self::RateLimit,
        Self::KeepFragments,
        Self::IgnoreRedirects,
        Self::RespectRobots,
        Self::Sitemaps,
        Self::Images,
        Self::Run,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Url => "Website URL",
            Self::MaxDepth => "Maximum depth",
            Self::MaxPages => "Maximum pages",
            Self::MaxSitemapDocuments => "Maximum sitemap documents",
            Self::Timeout => "Timeout (ms)",
            Self::MaxRedirects => "Maximum redirects",
            Self::Concurrency => "Concurrency",
            Self::UserAgent => "User agent",
            Self::RateLimit => "Rate limit (requests/sec)",
            Self::KeepFragments => "Keep URL fragments",
            Self::IgnoreRedirects => "Ignore redirect issues",
            Self::RespectRobots => "Respect robots.txt",
            Self::Sitemaps => "Discover XML sitemaps",
            Self::Images => "Check discovered images",
            Self::Run => "",
        }
    }

    const fn option_field(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::MaxDepth => "max_depth",
            Self::MaxPages => "max_pages",
            Self::MaxSitemapDocuments => "max_sitemap_documents",
            Self::Timeout => "timeout",
            Self::MaxRedirects => "max_redirects",
            Self::Concurrency => "concurrency",
            Self::UserAgent => "user_agent",
            Self::RateLimit => "rate_limit",
            Self::KeepFragments => "keep_fragments",
            Self::IgnoreRedirects => "ignore_redirects",
            Self::RespectRobots => "respect_robots",
            Self::Sitemaps => "sitemaps",
            Self::Images => "images",
            Self::Run => "run",
        }
    }

    const fn is_boolean(self) -> bool {
        matches!(
            self,
            Self::KeepFragments
                | Self::IgnoreRedirects
                | Self::RespectRobots
                | Self::Sitemaps
                | Self::Images
        )
    }

    const fn is_input(self) -> bool {
        !self.is_boolean() && !matches!(self, Self::Run)
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
struct SetupValues {
    url: String,
    max_depth: String,
    max_pages: String,
    max_sitemap_documents: String,
    timeout: String,
    max_redirects: String,
    concurrency: String,
    user_agent: String,
    rate_limit: String,
    keep_fragments: bool,
    ignore_redirects: bool,
    respect_robots: bool,
    sitemaps: bool,
    images: bool,
}

impl SetupValues {
    fn from_options(options: &Options) -> Self {
        Self {
            url: String::new(),
            max_depth: options.max_depth.to_string(),
            max_pages: options.max_pages.to_string(),
            max_sitemap_documents: options.max_sitemap_documents.to_string(),
            timeout: options.timeout.as_millis().to_string(),
            max_redirects: options.max_redirects.to_string(),
            concurrency: options.concurrency.to_string(),
            user_agent: options.user_agent.clone(),
            rate_limit: if options.rate_limit == 0.0 {
                String::new()
            } else {
                options.rate_limit.to_string()
            },
            keep_fragments: options.keep_fragments,
            ignore_redirects: options.ignore_redirects,
            respect_robots: options.respect_robots,
            sitemaps: options.sitemaps,
            images: options.images,
        }
    }
}

#[derive(Debug)]
pub(super) struct SetupState {
    values: SetupValues,
    preserved: Options,
    focused: SetupField,
    cursor: usize,
    scroll: usize,
    scrollbar_interaction: ScrollBarInteraction,
    scroll_to_focus: bool,
    errors: BTreeMap<SetupField, String>,
}

#[derive(Debug)]
pub(super) enum SetupAction {
    None,
    Start {
        target: String,
        options: Box<Options>,
    },
}

impl SetupState {
    pub(super) fn new(options: Options) -> Self {
        Self {
            values: SetupValues::from_options(&options),
            preserved: options,
            focused: SetupField::Url,
            cursor: 0,
            scroll: 0,
            scrollbar_interaction: ScrollBarInteraction::new(),
            scroll_to_focus: true,
            errors: BTreeMap::new(),
        }
    }

    pub(super) fn target(&self) -> &str {
        self.values.url.trim()
    }

    pub(super) fn reset(&mut self) {
        self.focused = SetupField::Url;
        self.cursor = self.values.url.chars().count();
        self.scroll = 0;
        self.scrollbar_interaction = ScrollBarInteraction::new();
        self.scroll_to_focus = true;
        self.errors.clear();
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, viewport_height: u16) -> SetupAction {
        match key.code {
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.move_focus(-1, viewport_height);
            }
            KeyCode::BackTab => self.move_focus(-1, viewport_height),
            KeyCode::Tab => self.move_focus(1, viewport_height),
            KeyCode::Enter => {
                if self.field() == SetupField::Run {
                    return self.submit();
                }
                self.move_focus(1, viewport_height);
            }
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(viewport_height as usize),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(viewport_height as usize),
            KeyCode::Left if self.field().is_boolean() => self.set_boolean(true),
            KeyCode::Right if self.field().is_boolean() => self.set_boolean(false),
            KeyCode::Char(' ') if self.field().is_boolean() => self.toggle_boolean(),
            KeyCode::Char('h' | 'l') if self.field().is_boolean() => self.toggle_boolean(),
            KeyCode::Left if self.field().is_input() => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Right if self.field().is_input() => {
                self.cursor = (self.cursor + 1).min(self.input().chars().count());
            }
            KeyCode::Home if self.field().is_input() => self.cursor = 0,
            KeyCode::End if self.field().is_input() => self.cursor = self.input().chars().count(),
            KeyCode::Backspace if self.field().is_input() => self.remove_before_cursor(),
            KeyCode::Delete if self.field().is_input() => self.remove_at_cursor(),
            KeyCode::Char(character)
                if self.field().is_input()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert_character(character);
            }
            _ => {}
        }

        SetupAction::None
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> SetupAction {
        let content_height = self.content_height();
        let visible_height = if content_height > usize::from(area.height) {
            usize::from(area.height.saturating_sub(2)).max(1)
        } else {
            usize::from(area.height).max(1)
        };

        if handle_scrollbar_mouse(
            mouse,
            area,
            content_height,
            visible_height,
            &mut self.scroll,
            &mut self.scrollbar_interaction,
        ) {
            self.scroll_to_focus = false;
            return SetupAction::None;
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_sub(3);
                self.scroll_to_focus = false;
                return SetupAction::None;
            }
            MouseEventKind::ScrollDown => {
                self.scroll = self.scroll.saturating_add(3);
                self.scroll_to_focus = false;
                return SetupAction::None;
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return SetupAction::None,
        }

        if !area.contains(Position::new(mouse.column, mouse.row)) {
            return SetupAction::None;
        }

        let inner = if self.content_height() > area.height as usize {
            Block::new().borders(Borders::ALL).inner(area)
        } else {
            area
        };

        if !inner.contains(Position::new(mouse.column, mouse.row)) {
            return SetupAction::None;
        }

        let content_y = usize::from(mouse.row - inner.y) + self.scroll;
        let row = content_y / FIELD_ROWS;
        let column_width = usize::from(inner.width).div_ceil(GRID_COLUMNS);
        let column = usize::from(mouse.column - inner.x) / column_width.max(1);
        let index = row * GRID_COLUMNS + column;

        if index >= SetupField::ALL.len() {
            return SetupAction::None;
        }

        self.focused = SetupField::ALL[index];
        self.cursor = self.input().chars().count();
        self.scroll_to_focus = false;
        let field = self.field();

        if field == SetupField::Run {
            return self.submit();
        }

        if field.is_boolean() && content_y % FIELD_ROWS == 1 {
            let cell_x = usize::from(mouse.column - inner.x) % column_width.max(1);
            self.set_boolean(cell_x < column_width / 2);
        }

        SetupAction::None
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let details = self.scope_details();
        let content_height = content_height(details.len());
        let overflow = content_height > area.height as usize;

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(NEUTRAL.c700);
        let viewport = if overflow { block.inner(area) } else { area };
        let visible_height = usize::from(viewport.height).max(1);

        if self.scroll_to_focus {
            self.ensure_focus_visible(visible_height);
            self.scroll_to_focus = false;
        }

        self.scroll = self
            .scroll
            .min(content_height.saturating_sub(visible_height));

        if overflow {
            frame.render_widget(block, area);
        }

        let text = self.render_text(viewport.width, &details);

        frame.render_widget(
            Paragraph::new(text)
                .style(NEUTRAL.c200)
                .scroll((u16::try_from(self.scroll).unwrap_or(u16::MAX), 0)),
            viewport,
        );

        if overflow {
            render_scrollbar(
                frame,
                area,
                content_height,
                visible_height,
                self.scroll,
                false,
            );
        }

        self.render_cursor(frame, viewport);
    }

    fn render_text<'a>(&self, width: u16, details: &'a [String]) -> Text<'a> {
        let left_width = usize::from(width).div_ceil(GRID_COLUMNS);
        let right_width = usize::from(width).saturating_sub(left_width);

        let mut lines = Vec::new();

        for row in 0..SetupField::ALL.len().div_ceil(GRID_COLUMNS) {
            let left = SetupField::ALL.get(row * GRID_COLUMNS).copied();
            let right = SetupField::ALL.get(row * GRID_COLUMNS + 1).copied();

            lines.push(self.paired_line(left, right, left_width, right_width, true));
            lines.push(self.paired_line(left, right, left_width, right_width, false));
            lines.push(self.error_line(left, right, left_width, right_width));
        }

        if !details.is_empty() {
            lines.push(Line::from(Span::styled(
                "Loaded settings (read-only)",
                NEUTRAL.c500,
            )));
            lines.extend(
                details
                    .iter()
                    .map(|line| Line::from(Span::styled(line.as_str(), NEUTRAL.c500))),
            );
        }

        Text::from(lines)
    }

    fn paired_line(
        &self,
        left: Option<SetupField>,
        right: Option<SetupField>,
        left_width: usize,
        right_width: usize,
        title: bool,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(2);

        for (field, width) in [(left, left_width), (right, right_width)] {
            let Some(field) = field else {
                spans.push(Span::raw(" ".repeat(width)));
                continue;
            };
            let value = if title {
                if field == SetupField::Run {
                    String::new()
                } else {
                    format!("  {}", field.label())
                }
            } else {
                format!("  {}", self.display_value(field))
            };
            let style = if self.focused == field {
                if title {
                    Style::new().fg(NEUTRAL.c50).bold()
                } else {
                    Style::new().fg(NEUTRAL.c200)
                }
            } else {
                Style::new().fg(NEUTRAL.c500)
            };
            let style = if field == SetupField::Run && !title && self.focused == field {
                Style::new().fg(NEUTRAL.c50).bold()
            } else {
                style
            };
            spans.push(Span::styled(fit_width(&value, width), style));
        }

        Line::from(spans)
    }

    fn error_line(
        &self,
        left: Option<SetupField>,
        right: Option<SetupField>,
        left_width: usize,
        right_width: usize,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(2);

        for (field, width) in [(left, left_width), (right, right_width)] {
            let message = field
                .and_then(|field| self.errors.get(&field))
                .map_or_else(String::new, |message| format!("  ! {message}"));
            spans.push(Span::styled(
                fit_width(&message, width),
                Style::new().fg(RED.c400).bold(),
            ));
        }

        Line::from(spans)
    }

    fn render_cursor(&self, frame: &mut Frame<'_>, viewport: Rect) {
        let field = self.field();
        if !field.is_input() {
            return;
        }

        let row = self.focused.index() / GRID_COLUMNS;
        let content_y = row * FIELD_ROWS + 1;
        if content_y < self.scroll || content_y >= self.scroll + usize::from(viewport.height) {
            return;
        }

        let column = self.focused.index() % GRID_COLUMNS;
        let cell_width = usize::from(viewport.width).div_ceil(GRID_COLUMNS);
        let cursor_byte = text_byte_index(self.input(), self.cursor);
        let cursor_width = Line::from(&self.input()[..cursor_byte]).width();
        let x = usize::from(viewport.x)
            + column * cell_width
            + CELL_PADDING_COLUMNS
            + INPUT_PREFIX_COLUMNS
            + cursor_width
                .min(cell_width.saturating_sub(CELL_PADDING_COLUMNS + INPUT_PREFIX_COLUMNS + 1));
        let y = usize::from(viewport.y) + content_y - self.scroll;

        if x < usize::from(viewport.right()) && y < usize::from(viewport.bottom()) {
            frame.set_cursor_position(Position::new(x as u16, y as u16));
        }
    }

    fn display_value(&self, field: SetupField) -> String {
        match field {
            SetupField::Url => format!(
                "> {}",
                if self.values.url.is_empty() {
                    "example.com"
                } else {
                    &self.values.url
                }
            ),
            SetupField::MaxDepth => format!("> {}", self.values.max_depth),
            SetupField::MaxPages => format!("> {}", self.values.max_pages),
            SetupField::MaxSitemapDocuments => {
                format!("> {}", self.values.max_sitemap_documents)
            }
            SetupField::Timeout => format!("> {}", self.values.timeout),
            SetupField::MaxRedirects => format!("> {}", self.values.max_redirects),
            SetupField::Concurrency => format!("> {}", self.values.concurrency),
            SetupField::UserAgent => format!("> {}", self.values.user_agent),
            SetupField::RateLimit => format!(
                "> {}",
                if self.values.rate_limit.is_empty() {
                    "disabled"
                } else {
                    &self.values.rate_limit
                }
            ),
            SetupField::Run => "[ Run audit ]".to_owned(),
            field => {
                if self.boolean(field) {
                    "[Yes]   No".to_owned()
                } else {
                    " Yes   [No]".to_owned()
                }
            }
        }
    }

    fn scope_details(&self) -> Vec<String> {
        let mut details = Vec::new();

        if !self.preserved.include_paths.is_empty() {
            details.push(format!(
                "Include paths: {}",
                self.preserved.include_paths.join(", ")
            ));
        }

        if !self.preserved.exclude_paths.is_empty() {
            details.push(format!(
                "Exclude paths: {}",
                self.preserved.exclude_paths.join(", ")
            ));
        }

        for ignore in &self.preserved.ignore_rules {
            details.push(format!(
                "Ignore {}: {}",
                ignore
                    .rules
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                ignore.url_prefix
            ));
        }

        details
    }

    fn submit(&mut self) -> SetupAction {
        match self.parse() {
            Ok((target, options)) => SetupAction::Start {
                target,
                options: Box::new(options),
            },
            Err(errors) => {
                self.errors = errors;
                if let Some(field) = SetupField::ALL
                    .iter()
                    .copied()
                    .find(|field| self.errors.contains_key(field))
                {
                    self.focused = field;
                    self.cursor = self.input().chars().count();
                    self.scroll_to_focus = true;
                }
                SetupAction::None
            }
        }
    }

    fn parse(&self) -> Result<(String, Options), BTreeMap<SetupField, String>> {
        let mut errors = BTreeMap::new();
        let target = self.values.url.trim().to_owned();
        let parsed_target = parse_target(&target).ok();

        if parsed_target.is_none() {
            errors.insert(SetupField::Url, "Enter a valid website URL".to_owned());
        }

        let mut options = self.preserved.clone();
        parse_integer(
            &self.values.max_depth,
            SetupField::MaxDepth,
            &mut errors,
            |value| options.max_depth = value,
        );
        parse_integer(
            &self.values.max_pages,
            SetupField::MaxPages,
            &mut errors,
            |value| options.max_pages = value,
        );
        parse_integer(
            &self.values.max_sitemap_documents,
            SetupField::MaxSitemapDocuments,
            &mut errors,
            |value| options.max_sitemap_documents = value,
        );
        parse_integer(
            &self.values.max_redirects,
            SetupField::MaxRedirects,
            &mut errors,
            |value| options.max_redirects = value,
        );
        parse_integer(
            &self.values.concurrency,
            SetupField::Concurrency,
            &mut errors,
            |value| options.concurrency = value,
        );

        match self.values.timeout.trim().parse::<u64>() {
            Ok(0) => {
                errors.insert(SetupField::Timeout, "Must be greater than 0".to_owned());
            }
            Ok(value) => options.timeout = Duration::from_millis(value),
            Err(_) => {
                errors.insert(SetupField::Timeout, "Enter a whole number".to_owned());
            }
        }

        let rate_limit = self.values.rate_limit.trim();
        if rate_limit.is_empty() {
            options.rate_limit = 0.0;
        } else {
            match rate_limit.parse::<f64>() {
                Ok(value) => options.rate_limit = value,
                Err(_) => {
                    errors.insert(
                        SetupField::RateLimit,
                        "Enter a number or leave blank".to_owned(),
                    );
                }
            }
        }

        options.user_agent = self.values.user_agent.clone();
        options.keep_fragments = self.values.keep_fragments;
        options.ignore_redirects = self.values.ignore_redirects;
        options.respect_robots = self.values.respect_robots;
        options.sitemaps = self.values.sitemaps;
        options.images = self.values.images;

        if let Err(validation) = options.validate() {
            for error in validation.fields {
                let field = SetupField::ALL
                    .into_iter()
                    .find(|field| error.field == field.option_field())
                    .unwrap_or(SetupField::Run);
                errors
                    .entry(field)
                    .or_insert_with(|| title_case(&error.message));
            }
        }

        if let Some(url) = parsed_target
            && !options.allows_page(&url)
        {
            errors.insert(
                SetupField::Url,
                "Choose a start URL within the loaded include/exclude paths".to_owned(),
            );
        }

        if errors.is_empty() {
            Ok((target, options))
        } else {
            Err(errors)
        }
    }

    fn move_focus(&mut self, direction: isize, viewport_height: u16) {
        let count = SetupField::ALL.len() as isize;
        let index = (self.focused.index() as isize + direction).rem_euclid(count) as usize;

        self.focused = SetupField::ALL[index];
        self.cursor = self.input().chars().count();
        self.scroll_to_focus = true;

        self.ensure_focus_visible(usize::from(viewport_height).max(1));
    }

    fn ensure_focus_visible(&mut self, visible_height: usize) {
        let top = self.focused.index() / GRID_COLUMNS * FIELD_ROWS;
        let bottom = top + FIELD_ROWS;

        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + visible_height {
            self.scroll = bottom.saturating_sub(visible_height);
        }
    }

    fn content_height(&self) -> usize {
        content_height(self.scope_details().len())
    }

    fn field(&self) -> SetupField {
        self.focused
    }

    fn input(&self) -> &str {
        match self.field() {
            SetupField::Url => &self.values.url,
            SetupField::MaxDepth => &self.values.max_depth,
            SetupField::MaxPages => &self.values.max_pages,
            SetupField::MaxSitemapDocuments => &self.values.max_sitemap_documents,
            SetupField::Timeout => &self.values.timeout,
            SetupField::MaxRedirects => &self.values.max_redirects,
            SetupField::Concurrency => &self.values.concurrency,
            SetupField::UserAgent => &self.values.user_agent,
            SetupField::RateLimit => &self.values.rate_limit,
            _ => "",
        }
    }

    fn input_mut(&mut self) -> &mut String {
        match self.field() {
            SetupField::Url => &mut self.values.url,
            SetupField::MaxDepth => &mut self.values.max_depth,
            SetupField::MaxPages => &mut self.values.max_pages,
            SetupField::MaxSitemapDocuments => &mut self.values.max_sitemap_documents,
            SetupField::Timeout => &mut self.values.timeout,
            SetupField::MaxRedirects => &mut self.values.max_redirects,
            SetupField::Concurrency => &mut self.values.concurrency,
            SetupField::UserAgent => &mut self.values.user_agent,
            SetupField::RateLimit => &mut self.values.rate_limit,
            _ => unreachable!("only input setup fields are editable as text"),
        }
    }

    fn insert_character(&mut self, character: char) {
        let byte = text_byte_index(self.input(), self.cursor);

        self.input_mut().insert(byte, character);
        self.cursor += 1;
        self.errors.remove(&self.field());
    }

    fn remove_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }

        self.cursor -= 1;
        self.remove_at_cursor();
    }

    fn remove_at_cursor(&mut self) {
        let start = text_byte_index(self.input(), self.cursor);
        let end = text_byte_index(self.input(), self.cursor + 1);

        if start < end {
            self.input_mut().replace_range(start..end, "");
            self.errors.remove(&self.field());
        }
    }

    fn boolean(&self, field: SetupField) -> bool {
        match field {
            SetupField::KeepFragments => self.values.keep_fragments,
            SetupField::IgnoreRedirects => self.values.ignore_redirects,
            SetupField::RespectRobots => self.values.respect_robots,
            SetupField::Sitemaps => self.values.sitemaps,
            SetupField::Images => self.values.images,
            _ => false,
        }
    }

    fn toggle_boolean(&mut self) {
        self.set_boolean(!self.boolean(self.field()));
    }

    fn set_boolean(&mut self, value: bool) {
        match self.field() {
            SetupField::KeepFragments => self.values.keep_fragments = value,
            SetupField::IgnoreRedirects => self.values.ignore_redirects = value,
            SetupField::RespectRobots => self.values.respect_robots = value,
            SetupField::Sitemaps => self.values.sitemaps = value,
            SetupField::Images => self.values.images = value,
            _ => return,
        }

        self.errors.remove(&self.field());
    }
}

fn content_height(detail_count: usize) -> usize {
    SetupField::ALL.len().div_ceil(GRID_COLUMNS) * FIELD_ROWS
        + usize::from(detail_count > 0)
        + detail_count
}

fn parse_integer<T: FromStr>(
    input: &str,
    field: SetupField,
    errors: &mut BTreeMap<SetupField, String>,
    assign: impl FnOnce(T),
) {
    match input.trim().parse::<T>() {
        Ok(value) => assign(value),
        Err(_) => {
            errors.insert(field, "Enter a whole number".to_owned());
        }
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let value_width = UnicodeWidthStr::width(value);
    if value_width <= width {
        return format!("{value}{}", " ".repeat(width - value_width));
    }

    let ellipsis_width = UnicodeWidthChar::width('…').unwrap_or(1);
    let mut fitted = String::new();
    let mut fitted_width = 0;

    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if fitted_width + character_width + ellipsis_width > width {
            break;
        }
        fitted.push(character);
        fitted_width += character_width;
    }

    fitted.push('…');

    fitted
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;
    use ratatui::{Terminal, backend::TestBackend};
    use scoutly::Options;
    use unicode_width::UnicodeWidthStr;

    use super::{SetupAction, SetupField, SetupState, fit_width};

    #[test]
    fn setup_uses_example_domain_as_a_placeholder_only() {
        let state = SetupState::new(Options::default());

        assert!(state.target().is_empty());
        assert!(state.parse().is_err());
        assert_eq!(state.display_value(SetupField::Url), "> example.com");
    }

    #[test]
    fn setup_parses_values_and_preserves_read_only_policy() {
        let options = Options {
            include_paths: vec!["/docs".into()],
            exclude_paths: vec!["/docs/archive".into()],
            ..Options::default()
        };

        let mut state = SetupState::new(options.clone());
        state.values.url = "  example.com/docs  ".into();
        state.values.max_depth = "2".into();
        state.values.rate_limit = "2.5".into();

        let (target, parsed) = state.parse().unwrap();

        assert_eq!(state.target(), "example.com/docs");
        assert_eq!(target, "example.com/docs");
        assert_eq!(parsed.max_depth, 2);
        assert_eq!(parsed.rate_limit, 2.5);
        assert_eq!(parsed.include_paths, options.include_paths);
        assert_eq!(parsed.exclude_paths, options.exclude_paths);
    }

    #[test]
    fn setup_reports_first_validation_error_and_does_not_start() {
        let mut state = SetupState::new(Options::default());
        state.values.url = "not a url".into();
        state.values.max_pages = "0".into();

        let action = state.submit();

        assert!(matches!(action, SetupAction::None));
        assert_eq!(state.field(), SetupField::Url);
        assert!(state.errors.contains_key(&SetupField::Url));
        assert!(state.errors.contains_key(&SetupField::MaxPages));
    }

    #[test]
    fn setup_text_and_boolean_controls_are_editable() {
        let mut state = SetupState::new(Options::default());

        state.values.url.clear();
        state.cursor = 0;
        state.handle_key(KeyEvent::new(KeyCode::Char('例'), KeyModifiers::NONE), 16);
        assert_eq!(state.values.url, "例");
        state.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), 16);
        assert!(state.values.url.is_empty());

        state.focused = SetupField::Images;

        assert!(state.values.images);
        state.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), 16);
        assert!(!state.values.images);
    }

    #[test]
    fn setup_scrollbar_drag_scrolls_without_refocusing_the_form() {
        let mut state = SetupState::new(Options::default());
        let area = Rect::new(1, 3, 68, 16);
        let scrollbar_column = area.right() - 1;

        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: scrollbar_column,
                row: area.y + 1,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: scrollbar_column,
                row: area.bottom() - 3,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );

        assert!(state.scroll > 0);

        let dragged_offset = state.scroll;
        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| state.render(frame, area)).unwrap();

        assert_eq!(state.scroll, dragged_offset);
    }

    #[test]
    fn fit_width_uses_terminal_columns_for_wide_characters() {
        let fitted = fit_width("網站 audit", 6);

        assert_eq!(UnicodeWidthStr::width(fitted.as_str()), 6);
        assert!(fitted.ends_with('…'));
    }
}
