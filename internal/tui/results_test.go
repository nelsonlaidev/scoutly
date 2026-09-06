package tui

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"charm.land/bubbles/v2/table"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestResultSelectorsApplyQueriesAndFilters(t *testing.T) {
	report := resultTestReport()
	state := newResultsState(report, 118, 30, newTheme(true))

	state.changeTab(tabIssues, report)
	if len(state.items) != 3 {
		t.Fatalf("issue count = %d", len(state.items))
	}
	state.filters.issues = "error"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].issue.Severity != audit.SeverityError {
		t.Fatalf("error items = %#v", state.items)
	}

	state.changeTab(tabPages, report)
	state.filters.pages = "healthy"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].page.URL != "https://example.com/" {
		t.Fatalf("healthy pages = %#v", state.items)
	}

	state.changeTab(tabLinks, report)
	state.filters.links = "redirected"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].link.URL != "https://example.com/old" {
		t.Fatalf("redirected links = %#v", state.items)
	}

	state.query.SetValue("SKIPPED")
	state.filters.links = "all"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].link.Result.Kind != audit.ResultSkipped {
		t.Fatalf("searched links = %#v", state.items)
	}

	state.changeTab(tabImages, report)
	state.filters.images = "invalid"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].image.URL != "https://example.com/not-image" {
		t.Fatalf("invalid images = %#v", state.items)
	}
}

func TestFragmentDoesNotCountAsLinkRedirect(t *testing.T) {
	finalURL := "https://example.com/page"
	link := audit.Link{
		URL: "https://example.com/page#section",
		Result: audit.LinkResult{
			Kind:     audit.ResultResponse,
			FinalURL: &finalURL,
		},
	}
	if linkRedirected(link) {
		t.Fatal("linkRedirected() = true for fragment-only difference")
	}
}

func TestResultSelectorsClassifyBlockedResources(t *testing.T) {
	status := 403
	linkURL := "https://example.com/blocked"
	imageURL := "https://example.com/blocked.png"
	reason := audit.FailureAntiBotChallenge
	report := &audit.Report{
		Summary: audit.Summary{
			Links:  audit.LinkSummary{Total: 1, Checked: 1, Blocked: 1},
			Images: audit.ImageSummary{Total: 1, Checked: 1, Blocked: 1},
		},
		Links: []audit.Link{{
			URL: linkURL,
			Result: audit.LinkResult{
				Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &linkURL, Reason: reason,
			},
		}},
		Images: []audit.Image{{
			URL: imageURL,
			Result: audit.ImageResult{
				Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &imageURL, Reason: reason,
			},
		}},
	}
	state := newResultsState(report, 118, 30, newTheme(true))
	overview := renderOverview(report, 118, newTheme(true))
	if got := strings.Count(overview, "Blocked: 1"); got != 2 {
		t.Fatalf("overview blocked counts = %d:\n%s", got, overview)
	}

	state.changeTab(tabLinks, report)
	state.filters.links = "blocked"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].link.URL != linkURL {
		t.Fatalf("blocked links = %#v", state.items)
	}
	if description := describeLink(state.items[0].link); !strings.Contains(description, "BLOCKED: anti-bot-challenge") {
		t.Fatalf("blocked link description = %q", description)
	}
	state.filters.links = "healthy"
	state.refresh(report)
	if len(state.items) != 0 {
		t.Fatalf("healthy links include blocked result: %#v", state.items)
	}

	state.changeTab(tabImages, report)
	state.filters.images = "blocked"
	state.refresh(report)
	if len(state.items) != 1 || state.items[0].image.URL != imageURL {
		t.Fatalf("blocked images = %#v", state.items)
	}
	if description := describeImage(state.items[0].image); !strings.Contains(description, "BLOCKED: anti-bot-challenge") {
		t.Fatalf("blocked image description = %q", description)
	}
	state.filters.images = "healthy"
	state.refresh(report)
	if len(state.items) != 0 {
		t.Fatalf("healthy images include blocked result: %#v", state.items)
	}
}

func TestOverviewMetricsFillAvailableWidth(t *testing.T) {
	report := resultTestReport()
	theme := newTheme(true)
	for _, width := range []int{66, 78, 118} {
		overview := ansi.Strip(renderOverview(report, width, theme))
		rows := strings.Split(overview, "\n")
		if len(rows) < 3 {
			t.Fatalf("width=%d overview does not contain metric boxes:\n%s", width, overview)
		}
		if strings.Count(rows[0], "╭") != 4 || strings.Count(rows[2], "╰") != 4 {
			t.Fatalf("width=%d overview does not contain four metric boxes:\n%s", width, overview)
		}
		if len(rows) < 6 || strings.Count(rows[3], "╭") != 1 {
			t.Fatalf("width=%d overview details are not in an adjacent rounded box:\n%s", width, overview)
		}
		for _, row := range rows[:3] {
			if got := ansi.StringWidth(row); got != width {
				t.Fatalf("width=%d metric row width = %d:\n%s", width, got, overview)
			}
		}
		if got := ansi.StringWidth(rows[3]); got != width {
			t.Fatalf("width=%d detail box width = %d:\n%s", width, got, overview)
		}

		state := newResultsState(report, width, 24, theme)
		resultLines := strings.Split(ansi.Strip(state.render(report, theme)), "\n")
		if len(resultLines) < 2 || !strings.Contains(resultLines[1], "╭") {
			t.Fatalf("width=%d overview has spacing before metric cards:\n%s", width, state.render(report, theme))
		}
	}
}

func TestResultsRenderScrollbarsForOverflowingContent(t *testing.T) {
	report := resultManyLinksReport(30)
	theme := newTheme(true)
	state := newResultsState(report, 118, 12, theme)

	overview := ansi.Strip(state.render(report, theme))
	if !strings.Contains(overview, scrollbarThumb) {
		t.Fatalf("overflowing overview does not render a scrollbar thumb:\n%s", overview)
	}
	if got := lipgloss.Width(overview); got != state.width {
		t.Fatalf("overview width = %d, want %d:\n%s", got, state.width, overview)
	}

	state.changeTab(tabLinks, report)
	if got := lipgloss.Width(state.table.View()); got != state.table.Width() {
		t.Fatalf("table width = %d, want %d", got, state.table.Width())
	}
	list := ansi.Strip(state.render(report, theme))
	if !strings.Contains(list, scrollbarThumb) {
		t.Fatalf("overflowing result list does not render a scrollbar thumb:\n%s", list)
	}
	if got := lipgloss.Width(list); got != state.width {
		t.Fatalf("result list width = %d, want %d:\n%s", got, state.width, list)
	}
	lines := strings.Split(list, "\n")
	paneTitleRow := resultListHeaderRows + 1
	tableHeaderRow := paneTitleRow + paneTitleRows
	if len(lines) <= tableHeaderRow {
		t.Fatalf("result list does not contain its table header:\n%s", list)
	}
	leftPaneWidth := (state.width - 1) / 2
	paneTitle := ansi.Cut(lines[paneTitleRow], 0, leftPaneWidth)
	tableHeader := ansi.Cut(lines[tableHeaderRow], 0, leftPaneWidth)
	if !strings.HasSuffix(paneTitle, " │") {
		t.Fatalf("scrollbar overlaps the pane title row:\n%s", list)
	}
	if !strings.HasSuffix(tableHeader, scrollbarThumb+"│") {
		t.Fatalf("scrollbar does not start beside the table header:\n%s", list)
	}
}

func TestResultsKeyboardNavigationAndNarrowDetail(t *testing.T) {
	report := resultTestReport()
	state := newResultsState(report, 78, 24, newTheme(true))

	action, _ := state.update(keyRune('2'), report)
	if action != resultNone || state.tab != tabIssues || state.focus != focusList {
		t.Fatalf("after tab switch: action=%v tab=%q focus=%v", action, state.tab, state.focus)
	}
	if columns := state.table.Columns(); len(columns) == 0 || columns[0].Title != "Issues" {
		t.Fatalf("table columns = %#v", columns)
	}

	_, _ = state.update(keyRune('f'), report)
	if state.filters.issues != "error" {
		t.Fatalf("issue filter = %q", state.filters.issues)
	}

	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEnter}, report)
	if !state.detailOpen || state.focus != focusDetail {
		t.Fatalf("detailOpen=%t focus=%v", state.detailOpen, state.focus)
	}

	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEscape}, report)
	if state.detailOpen || state.focus != focusList {
		t.Fatalf("detailOpen=%t focus=%v", state.detailOpen, state.focus)
	}

	action, _ = state.update(keyRune('n'), report)
	if action != resultNewAudit {
		t.Fatalf("action = %v, want resultNewAudit", action)
	}
}

func TestResultsSearchAndPaneFocus(t *testing.T) {
	report := resultTestReport()
	state := newResultsState(report, 118, 30, newTheme(true))
	state.changeTab(tabLinks, report)

	_, _ = state.update(keyRune('/'), report)
	if state.focus != focusSearch {
		t.Fatalf("focus = %v, want search", state.focus)
	}
	for _, character := range "broken" {
		_, _ = state.update(keyRune(character), report)
	}
	if state.query.Value() != "broken" || len(state.items) != 1 {
		t.Fatalf("query=%q items=%d", state.query.Value(), len(state.items))
	}

	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEnter}, report)
	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyTab}, report)
	if state.focus != focusDetail {
		t.Fatalf("focus = %v, want detail", state.focus)
	}
}

func TestResultsMouseClicksTabsOverviewAndControls(t *testing.T) {
	report := resultTestReport()
	theme := newTheme(true)
	state := newResultsState(report, 118, 30, theme)

	_ = state.updateMouseClick(tea.Mouse{
		X: resultTabX(tabIssues), Y: resultTabRow, Button: tea.MouseLeft,
	}, report, theme)
	if state.tab != tabIssues {
		t.Fatalf("tab = %q after Issues click, want %q", state.tab, tabIssues)
	}

	metricX := 0
	for index, metric := range overviewMetrics {
		state.changeTab(tabOverview, report)
		_ = state.updateMouseClick(tea.Mouse{
			X: metricX, Y: resultContentTop, Button: tea.MouseLeft,
		}, report, theme)
		if state.tab != metric.tab {
			t.Fatalf("metric %d selected tab %q, want %q", index, state.tab, metric.tab)
		}
		metricX += overviewMetricWidths(state.width)[index] + 1
	}

	state.changeTab(tabLinks, report)
	state.query.SetValue("keep this query")
	state.filters.links = "broken"
	_ = state.updateMouseClick(tea.Mouse{
		X: resultTabX(tabLinks), Y: resultTabRow, Button: tea.MouseLeft,
	}, report, theme)
	if state.query.Value() != "keep this query" || state.filters.links != "broken" {
		t.Fatalf(
			"active tab click reset state: query=%q filter=%q",
			state.query.Value(),
			state.filters.links,
		)
	}

	_ = state.updateMouseClick(tea.Mouse{
		X: 1, Y: resultContentTop + 1, Button: tea.MouseLeft,
	}, report, theme)
	if state.focus != focusSearch {
		t.Fatalf("focus = %v after search click, want %v", state.focus, focusSearch)
	}

	filterX := resultSearchFieldWidth(state.width) + len(resultControlGap)
	_ = state.updateMouseClick(tea.Mouse{
		X: filterX, Y: resultContentTop + 1, Button: tea.MouseLeft,
	}, report, theme)
	if state.filters.links != "blocked" || state.focus != focusList {
		t.Fatalf(
			"filter click: filter=%q focus=%v, want blocked/list",
			state.filters.links,
			state.focus,
		)
	}

	before := state.filters.links
	_ = state.updateMouseClick(tea.Mouse{
		X:      resultSearchFieldWidth(state.width),
		Y:      resultContentTop + 1,
		Button: tea.MouseLeft,
	}, report, theme)
	_ = state.updateMouseClick(tea.Mouse{
		X: filterX, Y: resultContentTop + 1, Button: tea.MouseRight,
	}, report, theme)
	if state.filters.links != before {
		t.Fatalf("gap or right click changed filter to %q, want %q", state.filters.links, before)
	}
}

func TestResultsMouseSelectsScrolledRowsAndOpensNarrowDetail(t *testing.T) {
	report := resultManyLinksReport(30)
	theme := newTheme(true)
	state := newResultsState(report, 118, 20, theme)
	state.changeTab(tabLinks, report)

	for range 18 {
		_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyDown}, report)
	}
	tableLines := strings.Split(ansi.Strip(state.table.View()), "\n")
	if len(tableLines) < 2 {
		t.Fatalf("table has no visible data rows:\n%s", state.table.View())
	}
	want := -1
	for index := range report.Links {
		if strings.Contains(tableLines[1], fmt.Sprintf("link-%02d", index)) {
			want = index
			break
		}
	}
	if want < 0 {
		t.Fatalf("could not identify first visible row: %q", tableLines[1])
	}

	_ = state.updateMouseClick(tea.Mouse{
		X: 1, Y: state.listDataTop(), Button: tea.MouseLeft,
	}, report, theme)
	if state.selected != want || state.table.Cursor() != want || state.focus != focusList {
		t.Fatalf(
			"wide row click: selected=%d cursor=%d focus=%v, want %d/list",
			state.selected,
			state.table.Cursor(),
			state.focus,
			want,
		)
	}
	leftWidth := (state.width - 1) / 2
	_ = state.updateMouseClick(tea.Mouse{
		X: leftWidth + 1, Y: resultListHeaderRows, Button: tea.MouseLeft,
	}, report, theme)
	if state.focus != focusDetail || state.selected != want {
		t.Fatalf("detail pane click: focus=%v selected=%d", state.focus, state.selected)
	}
	_ = state.updateMouseClick(tea.Mouse{
		X: 1, Y: resultListHeaderRows, Button: tea.MouseLeft,
	}, report, theme)
	if state.focus != focusList || state.selected != want {
		t.Fatalf("list pane click: focus=%v selected=%d", state.focus, state.selected)
	}

	narrow := newResultsState(report, 78, 20, theme)
	narrow.changeTab(tabLinks, report)
	_ = narrow.updateMouseClick(tea.Mouse{
		X: 1, Y: narrow.listDataTop() + 1, Button: tea.MouseLeft,
	}, report, theme)
	if narrow.selected != 1 || !narrow.detailOpen || narrow.focus != focusDetail {
		t.Fatalf(
			"narrow row click: selected=%d detailOpen=%t focus=%v",
			narrow.selected,
			narrow.detailOpen,
			narrow.focus,
		)
	}
}

func TestVisibleResultIndexValidatesRowsAndFallsBackWithoutSelectionStyle(t *testing.T) {
	report := resultManyLinksReport(30)
	theme := newTheme(true)
	state := newResultsState(report, 118, 20, theme)
	state.changeTab(tabLinks, report)

	for _, row := range []int{-1, state.table.Height()} {
		if index, ok := state.visibleResultIndex(row, theme); ok {
			t.Fatalf("visibleResultIndex(%d) = %d, true", row, index)
		}
	}
	empty := state
	empty.items = nil
	if index, ok := empty.visibleResultIndex(0, theme); ok {
		t.Fatalf("empty visibleResultIndex(0) = %d, true", index)
	}

	for range 18 {
		_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyDown}, report)
	}
	lines := strings.Split(ansi.Strip(state.table.View()), "\n")
	if len(lines) <= resultTableHeadRow {
		t.Fatalf("table has no visible rows:\n%s", state.table.View())
	}
	want := -1
	for index, item := range state.items {
		if strings.Contains(lines[resultTableHeadRow], item.label) {
			want = index
			break
		}
	}
	if want < 0 {
		t.Fatalf("could not identify first visible row: %q", lines[resultTableHeadRow])
	}

	fallbackTheme := theme
	fallbackTheme.table.Selected = lipgloss.NewStyle()
	if index, ok := state.visibleResultIndex(0, fallbackTheme); !ok || index != want {
		t.Fatalf("fallback visibleResultIndex(0) = %d, %t, want %d, true", index, ok, want)
	}
}

func TestResultMouseRowNormalization(t *testing.T) {
	lines := []string{
		"\x1b[31mfirst\x1b[0m  ",
		"second ",
		"\x1b[32m   \x1b[0m",
		"ignored",
	}
	rows := normalizedVisibleResultRows(lines)
	if len(rows) != 2 || rows[0] != "first" || rows[1] != "second" {
		t.Fatalf("normalized visible rows = %#v", rows)
	}

	row := table.Row{"abcdef", "ignored", "xy", "extra"}
	columns := []table.Column{{Width: 4}, {Width: 0}, {Width: 3}}
	if got := normalizedResultRow(row, columns, newTheme(true)); got != "abc… xy" {
		t.Fatalf("normalized result row = %q, want %q", got, "abc… xy")
	}
}

func TestResultsMouseWheelRoutesToPointedPane(t *testing.T) {
	report := resultManyLinksReport(30)
	theme := newTheme(true)
	state := newResultsState(report, 118, 16, theme)

	_ = state.updateMouseWheel(tea.Mouse{
		X: 1, Y: resultContentTop, Button: tea.MouseWheelDown,
	}, report)
	if state.overview.YOffset() == 0 {
		t.Fatal("overview wheel did not scroll overview")
	}
	overviewOffset := state.overview.YOffset()
	_ = state.updateMouseClick(tea.Mouse{
		X: 1, Y: resultContentTop, Button: tea.MouseLeft,
	}, report, theme)
	if state.tab != tabOverview {
		t.Fatal("clicking scrolled overview content activated a hidden metric card")
	}
	_ = state.updateMouseWheel(tea.Mouse{
		X: 1, Y: resultTabRow, Button: tea.MouseWheelDown,
	}, report)
	if state.overview.YOffset() != overviewOffset {
		t.Fatal("wheel over tabs scrolled overview")
	}

	state.changeTab(tabLinks, report)
	_ = state.setFocus(focusSearch)
	_ = state.updateMouseWheel(tea.Mouse{
		X: 1, Y: resultListHeaderRows, Button: tea.MouseWheelDown,
	}, report)
	if state.table.Cursor() != state.detail.MouseWheelDelta || state.focus != focusList {
		t.Fatalf(
			"list wheel: cursor=%d focus=%v, want %d/list",
			state.table.Cursor(),
			state.focus,
			state.detail.MouseWheelDelta,
		)
	}

	leftWidth := (state.width - 1) / 2
	listCursor := state.table.Cursor()
	_ = state.updateMouseWheel(tea.Mouse{
		X: leftWidth + 1, Y: resultListHeaderRows + 1 + paneTitleRows, Button: tea.MouseWheelDown,
	}, report)
	if state.detail.YOffset() == 0 || state.table.Cursor() != listCursor || state.focus != focusDetail {
		t.Fatalf(
			"detail wheel: offset=%d cursor=%d focus=%v",
			state.detail.YOffset(),
			state.table.Cursor(),
			state.focus,
		)
	}

	detailOffset := state.detail.YOffset()
	_ = state.updateMouseWheel(tea.Mouse{
		X: leftWidth, Y: resultListHeaderRows + 1 + paneTitleRows, Button: tea.MouseWheelDown,
	}, report)
	if state.detail.YOffset() != detailOffset || state.table.Cursor() != listCursor {
		t.Fatal("wheel over pane gap changed results state")
	}
}

func TestResultsSearchControlsUseResponsiveFields(t *testing.T) {
	report := resultTestReport()
	theme := newTheme(true)
	state := newResultsState(report, 118, 30, theme)
	state.changeTab(tabLinks, report)

	if got := state.query.Width(); got != 83 {
		t.Fatalf("search width = %d, want 83", got)
	}
	controlRows := func() []string {
		t.Helper()
		lines := strings.Split(ansi.Strip(state.render(report, theme)), "\n")
		if len(lines) < 4 {
			t.Fatalf("results view does not contain bordered controls:\n%s", state.render(report, theme))
		}
		return lines[1:4]
	}
	controlTop := func() string {
		t.Helper()
		lines := strings.Split(state.render(report, theme), "\n")
		if len(lines) < 2 {
			t.Fatalf("results view does not contain a control border:\n%s", state.render(report, theme))
		}
		return lines[1]
	}
	filterColumn := func(rows []string, filter string) int {
		t.Helper()
		beforeFilter, _, found := strings.Cut(rows[1], resultFilterLabel+filter)
		if !found {
			t.Fatalf("filter field not found in controls: %q", rows[1])
		}
		return ansi.StringWidth(beforeFilter)
	}
	assertFields := func(rows []string, width int) {
		t.Helper()
		if strings.Count(rows[0], "╭") != 2 || strings.Count(rows[0], "╮") != 2 ||
			strings.Count(rows[2], "╰") != 2 || strings.Count(rows[2], "╯") != 2 {
			t.Fatalf("controls do not contain two rounded fields: %q", rows)
		}
		for _, row := range rows {
			if got := ansi.StringWidth(row); got != width {
				t.Fatalf("control row width = %d, want %d: %q", got, width, row)
			}
		}
	}

	beforeFocusRows := controlRows()
	assertFields(beforeFocusRows, 118)
	searchFieldWidth := state.width - resultFilterFieldWidth - len(resultControlGap)
	wantUnfocusedTop, _, _ := strings.Cut(
		theme.metricBox.
			Width(searchFieldWidth).
			BorderForeground(theme.pane(false).border).
			Render("x"),
		"\n",
	)
	if !strings.HasPrefix(controlTop(), wantUnfocusedTop) {
		t.Fatalf("unfocused search border does not use the default border color")
	}
	beforeFocus := filterColumn(beforeFocusRows, "all")
	_, _ = state.update(keyRune('/'), report)
	afterFocusRows := controlRows()
	assertFields(afterFocusRows, 118)
	wantFocusedTop, _, _ := strings.Cut(
		theme.metricBox.
			Width(searchFieldWidth).
			BorderForeground(theme.pane(true).border).
			Render("x"),
		"\n",
	)
	if !strings.HasPrefix(controlTop(), wantFocusedTop) {
		t.Fatalf("focused search border does not match focused panes")
	}
	afterFocus := filterColumn(afterFocusRows, "all")
	if beforeFocus != afterFocus {
		t.Fatalf("filter column changed on focus: before=%d after=%d", beforeFocus, afterFocus)
	}

	state.resize(report, 66, 20, theme)
	state.filters.links = "redirected"
	if got := state.query.Width(); got != 31 {
		t.Fatalf("narrow search width = %d, want 31", got)
	}
	assertFields(controlRows(), 66)
}

func TestDetailLinesContainReportMetadataAndOccurrences(t *testing.T) {
	report := resultTestReport()
	state := newResultsState(report, 118, 30, newTheme(true))
	state.changeTab(tabIssues, report)

	lines := createDetailLines(state.items[0], report)
	joined := strings.Join(lines, "\n")
	for _, expected := range []string{"Severity: error", "Code: broken-link", "Found on:", "<a> Broken"} {
		if !strings.Contains(joined, expected) {
			t.Errorf("detail missing %q:\n%s", expected, joined)
		}
	}
}

func resultTestReport() *audit.Report {
	ok := 200
	bad := 500
	htmlType := "text/html"
	imageType := "image/png"
	notImageType := "text/plain"
	linkFinal := "https://example.com/destination"
	linkSame := "https://example.com/healthy"
	imageSame := "https://example.com/image.png"
	alt := "Logo"

	return &audit.Report{
		URL:       "https://example.com/",
		AuditedAt: time.Date(2026, 7, 31, 12, 0, 0, 0, time.UTC),
		Summary: audit.Summary{
			Pages: 2,
			Links: audit.LinkSummary{Total: 4, Checked: 3, Broken: 1, Redirected: 1},
			Images: audit.ImageSummary{
				Total: 2, Checked: 2, Invalid: 1,
			},
			Issues: audit.IssueSummary{Total: 3, Error: 1, Warning: 1, Info: 1},
		},
		Issues: []audit.Issue{
			{
				Code: audit.IssueBrokenLink, Severity: audit.SeverityError,
				Message: "Broken link", Target: audit.IssueTarget{
					Type: audit.TargetLink, URL: "https://example.com/broken",
				},
			},
			{
				Code: audit.IssueMissingTitle, Severity: audit.SeverityWarning,
				Message: "Missing title", Target: audit.IssueTarget{
					Type: audit.TargetPage, URL: "https://example.com/failed",
				},
			},
			{
				Code: audit.IssueInvalidImageContentType, Severity: audit.SeverityInfo,
				Message: "Invalid image", Target: audit.IssueTarget{
					Type: audit.TargetImage, URL: "https://example.com/not-image",
				},
			},
		},
		Pages: []audit.Page{
			{
				URL: "https://example.com/", StatusCode: &ok, ContentType: &htmlType,
				Title: new("Home"), Headings: audit.Headings{H1: []string{"Welcome"}},
			},
			{URL: "https://example.com/failed", StatusCode: &bad, Headings: audit.Headings{}},
		},
		Links: []audit.Link{
			{
				URL: "https://example.com/broken",
				Result: audit.LinkResult{
					Kind: audit.ResultFailed, Reason: audit.FailureConnectionFailed,
				},
				FoundOn: []audit.LinkOccurrence{{
					PageURL: "https://example.com/", OriginalURL: "/broken",
					Element: "a", Text: "Broken",
				}},
			},
			{
				URL: "https://example.com/old",
				Result: audit.LinkResult{
					Kind: audit.ResultResponse, StatusCode: &ok, FinalURL: &linkFinal,
				},
			},
			{
				URL: "https://example.com/healthy",
				Result: audit.LinkResult{
					Kind: audit.ResultResponse, StatusCode: &ok, FinalURL: &linkSame,
				},
			},
			{
				URL: "mailto:hello@example.com",
				Result: audit.LinkResult{
					Kind: audit.ResultSkipped, Reason: audit.FailureUnsupportedProtocol,
				},
			},
		},
		Images: []audit.Image{
			{
				URL: "https://example.com/image.png",
				Result: audit.ImageResult{
					Kind: audit.ResultResponse, StatusCode: &ok,
					FinalURL: &imageSame, ContentType: &imageType,
				},
				FoundOn: []audit.ImageOccurrence{{
					PageURL: "https://example.com/", OriginalURL: "/image.png",
					Element: "img", Attribute: "src", Alt: &alt,
				}},
			},
			{
				URL: "https://example.com/not-image",
				Result: audit.ImageResult{
					Kind: audit.ResultResponse, StatusCode: &ok,
					FinalURL:    new("https://example.com/not-image"),
					ContentType: &notImageType,
				},
			},
		},
	}
}

func resultManyLinksReport(count int) *audit.Report {
	report := resultTestReport()
	report.Issues = nil
	report.Links = make([]audit.Link, 0, count)
	report.Summary.Links = audit.LinkSummary{Total: count, Checked: count}
	for index := range count {
		linkURL := fmt.Sprintf("https://example.com/link-%02d", index)
		occurrences := make([]audit.LinkOccurrence, 0, 10)
		for occurrence := range 10 {
			occurrences = append(occurrences, audit.LinkOccurrence{
				PageURL:     fmt.Sprintf("https://example.com/page-%02d", occurrence),
				OriginalURL: fmt.Sprintf("/link-%02d", index),
				Element:     "a",
				Text:        fmt.Sprintf("Link %02d", index),
			})
		}
		report.Links = append(report.Links, audit.Link{
			URL: linkURL,
			Result: audit.LinkResult{
				Kind:       audit.ResultResponse,
				StatusCode: new(200),
				FinalURL:   new(linkURL),
			},
			FoundOn: occurrences,
		})
	}
	return report
}

func resultTabX(wanted resultTab) int {
	left := 0
	for index, tab := range resultTabs {
		if tab == wanted {
			return left
		}
		left += len(fmt.Sprintf("%d %s", index+1, titleCase(string(tab)))) + 2
	}
	return -1
}

func keyRune(value rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: value, Text: string(value)}
}
