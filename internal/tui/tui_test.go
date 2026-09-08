package tui

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestModelStartsCancelsAndFinishesAudit(t *testing.T) {
	rootContext, cancelRoot := context.WithCancel(t.Context())
	defer cancelRoot()

	state := newModel(rootContext, audit.DefaultOptions())
	state.setup.values.url = "https://example.com"
	state.setup.form.State = huh.StateCompleted

	updated, command := state.Update(struct{}{})
	state = updated.(model)
	if state.screen != screenRunning || command == nil {
		t.Fatalf("screen=%v command=%v", state.screen, command)
	}

	updated, _ = state.Update(tea.KeyPressMsg{Code: 'c', Mod: tea.ModCtrl})
	state = updated.(model)
	if !state.running.canceling {
		t.Fatal("running.canceling = false")
	}
	if !errors.Is(state.running.context.Err(), context.Canceled) {
		t.Fatalf("running context error = %v", state.running.context.Err())
	}

	updated, _ = state.Update(auditFinishedMsg{err: context.Canceled})
	state = updated.(model)
	if state.screen != screenCanceled {
		t.Fatalf("screen = %v, want canceled", state.screen)
	}

	updated, _ = state.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	state = updated.(model)
	if state.screen != screenSetup || state.setup.values.url != "https://example.com" {
		t.Fatalf("retry state = %#v", state)
	}
}

func TestModelCompletesIntoResultsAndStartsNewAudit(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.screen = screenRunning
	state.running = runningState{}
	report := resultTestReport()

	updated, _ := state.Update(auditFinishedMsg{report: report})
	state = updated.(model)
	if state.screen != screenResults || state.report != report {
		t.Fatalf("screen=%v report=%p", state.screen, state.report)
	}

	updated, _ = state.Update(keyRune('n'))
	state = updated.(model)
	if state.screen != screenSetup || state.report != nil {
		t.Fatalf("screen=%v report=%p", state.screen, state.report)
	}
}

func TestInteractiveScreensEnableMouseMode(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.resize(70, 20, newTheme(true))
	if !state.setup.overflow {
		t.Fatal("setup overflow = false at minimum terminal size")
	}
	if got := state.View().MouseMode; got != tea.MouseModeCellMotion {
		t.Fatalf("mouse mode = %v, want cell motion", got)
	}

	state.resize(118, 80, newTheme(true))
	if state.setup.overflow {
		t.Fatal("setup overflow = true at large terminal size")
	}
	if got := state.View().MouseMode; got != tea.MouseModeCellMotion {
		t.Fatalf("mouse mode = %v, want cell motion", got)
	}

	state.startAudit("https://example.com", audit.DefaultOptions())
	if got := state.View().MouseMode; got != tea.MouseModeCellMotion {
		t.Fatalf("running mouse mode = %v, want cell motion", got)
	}
	state.finishAudit(auditFinishedMsg{report: resultTestReport()})
	if got := state.View().MouseMode; got != tea.MouseModeCellMotion {
		t.Fatalf("results mouse mode = %v, want cell motion", got)
	}

	state.screen = screenFailed
	if got := state.View().MouseMode; got != tea.MouseModeNone {
		t.Fatalf("failed mouse mode = %v, want none", got)
	}
	state.screen = screenResults
	state.resize(minimumWidth-1, minimumHeight-1, newTheme(true))
	if got := state.View().MouseMode; got != tea.MouseModeNone {
		t.Fatalf("resize warning mouse mode = %v, want none", got)
	}
	before := state.results.tab
	updated, _ := state.Update(tea.MouseClickMsg{
		X:      resultTabX(tabIssues) + shell.bodyPaddingColumns/2,
		Y:      resultTabRow + shell.headerRows,
		Button: tea.MouseLeft,
	})
	state = updated.(model)
	if state.results.tab != before {
		t.Fatal("mouse click changed hidden results under resize warning")
	}
}

func TestModelRoutesResultMouseCoordinatesAndIgnoresFooter(t *testing.T) {
	report := resultTestReport()
	state := newModel(context.Background(), audit.DefaultOptions())
	state.resize(122, 34, newTheme(true))
	state.screen = screenResults
	state.report = report
	state.results = newResultsState(
		report,
		state.contentWidth(),
		state.contentHeight(),
		newTheme(true),
	)

	updated, _ := state.Update(tea.MouseClickMsg{
		X:      resultTabX(tabIssues) + shell.bodyPaddingColumns/2,
		Y:      resultTabRow + shell.headerRows,
		Button: tea.MouseLeft,
	})
	state = updated.(model)
	if state.results.tab != tabIssues {
		t.Fatalf("tab = %q after model-routed click, want %q", state.results.tab, tabIssues)
	}

	before := state.results.tab
	updated, _ = state.Update(tea.MouseClickMsg{
		X:      1,
		Y:      state.height - 1,
		Button: tea.MouseLeft,
	})
	state = updated.(model)
	if state.results.tab != before || state.screen != screenResults {
		t.Fatal("footer click changed results state")
	}
}

func TestSetupClickFocusesInputAndSelectsConfirmOption(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	_ = state.setup.form.Init()
	state.resize(118, 80, newTheme(true))

	clickText := func(text string, afterY int) tea.MouseClickMsg {
		t.Helper()
		lines := strings.Split(ansi.Strip(state.View().Content), "\n")
		for y := max(0, afterY); y < len(lines); y++ {
			if beforeText, _, found := strings.Cut(lines[y], text); found {
				return tea.MouseClickMsg{
					X:      lipgloss.Width(beforeText),
					Y:      y,
					Button: tea.MouseLeft,
				}
			}
		}
		t.Fatalf("could not find %q after row %d:\n%s", text, afterY, state.View().Content)
		return tea.MouseClickMsg{}
	}

	updated, _ := state.Update(clickText("Maximum depth", 0))
	state = updated.(model)
	if key := state.setup.form.GetFocusedField().GetKey(); key != string(fieldMaxDepth) {
		t.Fatalf("focused field = %q, want %q", key, fieldMaxDepth)
	}
	styles := state.setup.theme.Theme(false)
	view := state.setup.form.View()
	if focusedURL := styles.Focused.Title.Render("Website URL"); strings.Contains(view, focusedURL) {
		t.Fatalf("Website URL remained visually focused after clicking Maximum depth:\n%s", view)
	}
	if blurredURL := styles.Blurred.Title.Render("Website URL"); !strings.Contains(view, blurredURL) {
		t.Fatalf("Website URL is not visually blurred after clicking Maximum depth:\n%s", view)
	}
	if focusedDepth := styles.Focused.Title.Render("Maximum depth"); !strings.Contains(view, focusedDepth) {
		t.Fatalf("Maximum depth is not visually focused after clicking it:\n%s", view)
	}

	updated, _ = state.Update(clickText("Maximum pages", 0))
	state = updated.(model)
	if key := state.setup.form.GetFocusedField().GetKey(); key != string(fieldMaxPages) {
		t.Fatalf("focused field = %q, want %q", key, fieldMaxPages)
	}
	view = state.setup.form.View()
	if focusedDepth := styles.Focused.Title.Render("Maximum depth"); strings.Contains(view, focusedDepth) {
		t.Fatalf("Maximum depth remained visually focused after clicking Maximum pages:\n%s", view)
	}
	if blurredDepth := styles.Blurred.Title.Render("Maximum depth"); !strings.Contains(view, blurredDepth) {
		t.Fatalf("Maximum depth is not visually blurred after clicking Maximum pages:\n%s", view)
	}
	if focusedPages := styles.Focused.Title.Render("Maximum pages"); !strings.Contains(view, focusedPages) {
		t.Fatalf("Maximum pages is not visually focused after clicking it:\n%s", view)
	}

	confirmTitle := clickText("Keep URL fragments", 0)
	updated, _ = state.Update(clickText("Yes", confirmTitle.Y+1))
	state = updated.(model)
	if !state.setup.values.keepFragments {
		t.Fatal("clicking Yes directly did not enable Keep URL fragments")
	}
	if key := state.setup.form.GetFocusedField().GetKey(); key != string(fieldKeepFragments) {
		t.Fatalf("focused field = %q, want %q", key, fieldKeepFragments)
	}

	updated, command := state.Update(clickText("No", confirmTitle.Y+1))
	state = updated.(model)
	if state.setup.values.keepFragments {
		t.Fatal("clicking No did not disable Keep URL fragments")
	}
	if command != nil {
		t.Fatal("clicking No returned a command that could advance form focus")
	}
	if key := state.setup.form.GetFocusedField().GetKey(); key != string(fieldKeepFragments) {
		t.Fatalf("focused field = %q after clicking No, want %q", key, fieldKeepFragments)
	}
}

func TestRunAuditButtonValidatesAndStartsOnLeftClick(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	_ = state.setup.form.Init()
	state.resize(78, 20, newTheme(true))
	findButton := func() tea.MouseClickMsg {
		t.Helper()
		state.setup.viewport.GotoBottom()
		lines := strings.Split(ansi.Strip(state.View().Content), "\n")
		for y, line := range lines {
			if beforeLabel, _, found := strings.Cut(line, runButtonLabel); found {
				return tea.MouseClickMsg{
					X:      lipgloss.Width(beforeLabel) - 1,
					Y:      y,
					Button: tea.MouseLeft,
				}
			}
		}
		t.Fatalf("could not find %q button:\n%s", runButtonLabel, state.View().Content)
		return tea.MouseClickMsg{}
	}

	updated, command := state.Update(findButton())
	state = updated.(model)
	if state.screen != screenSetup || command == nil {
		t.Fatalf("invalid click result: screen=%v command=%v", state.screen, command)
	}
	if key := state.setup.form.GetFocusedField().GetKey(); key != string(fieldURL) {
		t.Fatalf("focused field = %q after validation, want %q", key, fieldURL)
	}
	if err := state.setup.form.GetFocusedField().Error(); err == nil {
		t.Fatal("URL field error = nil after validation")
	}
	wantError := "Enter a valid website URL"
	if got := state.setup.validationError; got != wantError {
		t.Fatalf("validation error = %q, want %q", got, wantError)
	}
	view := state.View().Content
	if content := ansi.Strip(view); !strings.Contains(content, wantError) {
		t.Fatalf("setup view does not show validation error:\n%s", content)
	}
	if width := lipgloss.Width(view); width > state.width {
		t.Fatalf("validation view width = %d, want <= %d", width, state.width)
	}
	if height := lipgloss.Height(view); height > state.height {
		t.Fatalf("validation view height = %d, want <= %d", height, state.height)
	}

	updated, _ = state.Update(tea.PasteMsg{Content: "https://example.com"})
	state = updated.(model)
	if state.setup.validationError != "" {
		t.Fatalf("validation error was not cleared: %q", state.setup.validationError)
	}

	leftClick := findButton()
	updated, _ = state.Update(tea.MouseClickMsg{
		X:      leftClick.X,
		Y:      leftClick.Y,
		Button: tea.MouseRight,
	})
	state = updated.(model)
	if state.screen != screenSetup {
		t.Fatalf("right click changed screen to %v", state.screen)
	}
	leftClick = findButton()
	updated, command = state.Update(leftClick)
	state = updated.(model)
	if state.screen != screenRunning || command == nil {
		t.Fatalf("left click result: screen=%v command=%v", state.screen, command)
	}
	state.running.cancelAudit()
}

func TestSetupFooterIsAvailableOnFirstFrame(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	footer := state.footer.FullHelpView(state.activeKeyMap().FullHelp())
	if got := lipgloss.Height(footer); got != 1 {
		t.Fatalf("initial setup footer height = %d, want 1: %s", got, footer)
	}
	for _, help := range []string{"Tab/Enter", "Shift+Tab", "Space", "Ctrl+C"} {
		if !strings.Contains(footer, help) {
			t.Fatalf("initial setup footer does not contain %q: %s", help, footer)
		}
	}
}

func TestFooterTracksScreenTransitionsAndResizeWarning(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.startAudit(
		"https://example.com",
		audit.DefaultOptions(),
	)
	assertFooterContains(
		t,
		state.footer.FullHelpView(state.activeKeyMap().FullHelp()),
		"Ctrl+C",
		"cancel audit",
	)

	state.finishAudit(auditFinishedMsg{report: resultTestReport()})
	assertFooterContains(
		t,
		state.footer.FullHelpView(state.activeKeyMap().FullHelp()),
		"1-5",
		"search",
		"filter",
		"Tab",
		"Arrows",
		"new",
		"quit",
	)

	state.screen = screenRunning
	state.running = newRunningState(
		context.Background(),
		func() {},
		make(chan tea.Msg),
		"https://example.com",
		state.contentWidth(),
		state.contentHeight(),
	)
	state.finishAudit(auditFinishedMsg{err: context.Canceled})
	assertFooterContains(
		t,
		state.footer.FullHelpView(state.activeKeyMap().FullHelp()),
		"Enter",
		"edit and retry",
		"q",
		"quit",
	)

	state.resize(minimumWidth-1, minimumHeight-1, newTheme(true))
	footer := state.footer.FullHelpView(state.activeKeyMap().FullHelp())
	assertFooterContains(t, footer, "Ctrl+C", "quit")
	if strings.Contains(footer, "retry") {
		t.Fatalf("resize footer contains hidden-screen binding: %s", footer)
	}

	state.resize(defaultWidth, defaultHeight, newTheme(true))
	assertFooterContains(
		t,
		state.footer.FullHelpView(state.activeKeyMap().FullHelp()),
		"Enter",
		"edit and retry",
	)
}

func assertFooterContains(t *testing.T, footer string, expected ...string) {
	t.Helper()
	for _, value := range expected {
		if !strings.Contains(footer, value) {
			t.Errorf("footer does not contain %q: %s", value, footer)
		}
	}
}

func TestModelForwardsNonKeyMessagesToResultsSearch(t *testing.T) {
	report := resultTestReport()
	state := newModel(context.Background(), audit.DefaultOptions())
	state.screen = screenResults
	state.report = report
	state.results = newResultsState(report, 118, 30, newTheme(true))
	state.results.changeTab(tabLinks, report)

	updated, _ := state.Update(keyRune('/'))
	state = updated.(model)
	if state.results.focus != focusSearch {
		t.Fatalf("focus = %v, want search", state.results.focus)
	}

	updated, _ = state.Update(tea.PasteMsg{Content: "broken"})
	state = updated.(model)
	if state.results.query.Value() != "broken" || len(state.results.items) != 1 {
		t.Fatalf(
			"query=%q items=%d",
			state.results.query.Value(),
			len(state.results.items),
		)
	}

	_, command := state.Update(textinput.Blink())
	if command == nil {
		t.Fatal("cursor blink command = nil")
	}
}

func TestModelIgnoresElapsedTicksFromPreviousAudit(t *testing.T) {
	currentEvents := make(chan tea.Msg)
	state := newModel(context.Background(), audit.DefaultOptions())
	state.screen = screenRunning
	state.running = newRunningState(
		context.Background(),
		func() {},
		currentEvents,
		"https://example.com",
		78,
		20,
	)

	before := state.running.now
	updated, command := state.Update(elapsedTickMsg{
		now:    before.Add(time.Second),
		events: make(chan tea.Msg),
	})
	state = updated.(model)
	if command != nil || !state.running.now.Equal(before) {
		t.Fatalf("stale tick updated state: command=%v now=%v", command, state.running.now)
	}

	next := before.Add(2 * time.Second)
	updated, command = state.Update(elapsedTickMsg{now: next, events: currentEvents})
	state = updated.(model)
	if command == nil || !state.running.now.Equal(next) {
		t.Fatalf("current tick not accepted: command=%v now=%v", command, state.running.now)
	}
}

func TestModelShowsFailureAndResizeWarning(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.screen = screenRunning
	state.running = runningState{}

	updated, _ := state.Update(auditFinishedMsg{err: errors.New("network unavailable")})
	state = updated.(model)
	if state.screen != screenFailed || state.failureMessage != "network unavailable" {
		t.Fatalf("screen=%v failure=%q", state.screen, state.failureMessage)
	}

	updated, _ = state.Update(tea.WindowSizeMsg{Width: 69, Height: 19})
	state = updated.(model)
	if content := state.View().Content; !strings.Contains(content, "Terminal is too small") {
		t.Fatalf("view does not contain resize warning:\n%s", content)
	}
	before := state.failureMessage
	updated, _ = state.Update(keyRune('n'))
	state = updated.(model)
	if state.screen != screenFailed || state.failureMessage != before {
		t.Fatal("small-terminal input changed hidden screen state")
	}
}

func TestRunningActivityIsDistinctAndBounded(t *testing.T) {
	state := runningState{
		activityViewport: newRunningState(
			context.Background(),
			func() {},
			make(chan tea.Msg),
			"https://example.com",
			78,
			20,
		).activityViewport,
	}

	for index := range maxActivityEntries + 10 {
		state.receiveProgress(audit.Progress{
			Phase:      audit.PhaseCrawl,
			CurrentURL: "https://example.com/" + time.Unix(int64(index), 0).Format("150405"),
		})
	}
	state.receiveProgress(state.progress)

	if len(state.activity) != maxActivityEntries {
		t.Fatalf("activity length = %d", len(state.activity))
	}
	if strings.Contains(state.activity[0], "000000") {
		t.Fatalf("oldest activity was not discarded: %q", state.activity[0])
	}
}

func TestRunningMouseWheelPausesAndResumesActivityFollow(t *testing.T) {
	state := newRunningState(
		context.Background(),
		func() {},
		make(chan tea.Msg),
		"https://example.com",
		78,
		20,
	)
	for index := range 30 {
		state.receiveProgress(audit.Progress{
			Phase:      audit.PhaseCrawl,
			CurrentURL: fmt.Sprintf("https://example.com/page-%02d", index),
		})
	}
	if !state.activityFollowing || !state.activityViewport.AtBottom() {
		t.Fatal("activity did not initially follow the newest entry")
	}

	recentY := runningSummaryRows + currentPaneRows
	bottomOffset := state.activityViewport.YOffset()
	_ = state.updateMouseWheel(tea.Mouse{
		X: 1, Y: recentY, Button: tea.MouseWheelUp,
	})
	if state.activityFollowing || state.activityViewport.YOffset() >= bottomOffset {
		t.Fatalf(
			"wheel up: following=%t offset=%d bottom=%d",
			state.activityFollowing,
			state.activityViewport.YOffset(),
			bottomOffset,
		)
	}

	pausedOffset := state.activityViewport.YOffset()
	_ = state.updateMouseWheel(tea.Mouse{
		X: 1, Y: runningSummaryRows, Button: tea.MouseWheelDown,
	})
	if state.activityViewport.YOffset() != pausedOffset {
		t.Fatal("wheel outside Recent activity changed its offset")
	}
	state.receiveProgress(audit.Progress{
		Phase: audit.PhaseCrawl, CurrentURL: "https://example.com/new",
	})
	if state.activityViewport.YOffset() != pausedOffset || state.activityFollowing {
		t.Fatalf(
			"new event reset paused viewport: offset=%d following=%t",
			state.activityViewport.YOffset(),
			state.activityFollowing,
		)
	}

	for !state.activityFollowing {
		_ = state.updateMouseWheel(tea.Mouse{
			X: 1, Y: recentY, Button: tea.MouseWheelDown,
		})
	}
	state.receiveProgress(audit.Progress{
		Phase: audit.PhaseCrawl, CurrentURL: "https://example.com/newest",
	})
	if !state.activityViewport.AtBottom() {
		t.Fatal("activity did not resume following after returning to the bottom")
	}
}

func TestRunningPausedActivityCompensatesForTrimmedEntries(t *testing.T) {
	state := newRunningState(
		context.Background(),
		func() {},
		make(chan tea.Msg),
		"https://example.com",
		78,
		20,
	)
	for index := range maxActivityEntries {
		state.receiveProgress(audit.Progress{
			Phase:      audit.PhaseCrawl,
			CurrentURL: fmt.Sprintf("https://example.com/page-%03d", index),
		})
	}
	state.activityFollowing = false
	state.activityViewport.SetYOffset(10)
	state.receiveProgress(audit.Progress{
		Phase: audit.PhaseCrawl, CurrentURL: "https://example.com/overflow",
	})
	if got := state.activityViewport.YOffset(); got != 9 {
		t.Fatalf("trimmed paused offset = %d, want 9", got)
	}
	if state.activityFollowing {
		t.Fatal("trimming resumed activity following")
	}
}

func TestRunningRenderShowsActivityScrollbar(t *testing.T) {
	state := newRunningState(
		context.Background(),
		func() {},
		make(chan tea.Msg),
		"https://example.com",
		78,
		20,
	)
	for index := range 30 {
		state.receiveProgress(audit.Progress{
			Phase:      audit.PhaseCrawl,
			CurrentURL: fmt.Sprintf("https://example.com/page-%02d", index),
		})
	}

	view := ansi.Strip(state.render(78, 20, newTheme(true)))
	if !strings.Contains(view, scrollbarThumb) {
		t.Fatalf("overflowing activity does not render a scrollbar thumb:\n%s", view)
	}
	if got := lipgloss.Width(view); got > 78 {
		t.Fatalf("running view width = %d, want <= 78:\n%s", got, view)
	}
}

func TestRunningRenderUsesReadableProgressLayout(t *testing.T) {
	totalLinks := 87
	totalImages := 24
	state := newRunningState(
		context.Background(),
		func() {},
		make(chan tea.Msg),
		"https://example.com",
		78,
		20,
	)
	state.startedAt = time.Unix(0, 0)
	state.now = state.startedAt.Add(42 * time.Second)
	state.progress = audit.Progress{
		Phase:    audit.PhaseLinks,
		Pages:    audit.PageProgress{Crawled: 114, Discovered: 114},
		Sitemaps: audit.SitemapProgress{Fetched: 2},
		Links:    audit.ResourceProgress{Checked: 87, Total: &totalLinks},
		Images:   audit.ResourceProgress{Checked: 24, Total: &totalImages},
	}

	view := state.render(78, 20, newTheme(true))
	lines := strings.Split(ansi.Strip(view), "\n")
	phaseLine := "[x] Robots   [x] Crawl   [x] Sitemaps   [>] Links   [ ] Images   [ ] Report"
	if len(lines) < 4 || lines[0] != phaseLine {
		t.Fatalf("phase line is not the first row:\n%s", view)
	}
	if strings.Contains(view, "Auditing ") {
		t.Fatalf("running view still contains the auditing title:\n%s", view)
	}
	if strings.Contains(view, "[x] Maps") {
		t.Fatalf("phase line still uses Maps:\n%s", view)
	}
	if want := "0:42 · 114/114 pages · 2 sitemaps · 87/87 links · 24/24 images"; lines[1] != "" || lines[2] != want || lines[3] != "" {
		t.Fatalf("progress summary %q is not surrounded by blank rows:\n%s", want, view)
	}
	if got := lipgloss.Width(view); got > 78 {
		t.Fatalf("running view width = %d, want <= 78", got)
	}
	if got := lipgloss.Height(view); got > 20 {
		t.Fatalf("running view height = %d, want <= 20", got)
	}

	state.resize(66, 20)
	narrowView := state.render(66, 20, newTheme(true))
	if got := lipgloss.Width(narrowView); got > 66 {
		t.Fatalf("narrow running view width = %d, want <= 66", got)
	}
	if want := "[x] Robots [x] Crawl [x] Sitemaps [>] Links [ ] Images [ ] Report"; !strings.Contains(ansi.Strip(narrowView), want) {
		t.Fatalf("narrow phase line does not contain %q:\n%s", want, narrowView)
	}
}

func TestAuditCommandStreamsProgressAndReport(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		response.Header().Set("Content-Type", "text/html")
		_, _ = response.Write([]byte("<html><title>Home</title><body><h1>Home</h1></body></html>"))
	}))
	defer server.Close()

	options := audit.DefaultOptions()
	options.MaxDepth = 0
	options.MaxPages = 1
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.Concurrency = 1

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	events := make(chan tea.Msg)
	message := startAudit(ctx, server.URL, options, events)()
	progressCount := 0

	for {
		switch value := message.(type) {
		case auditProgressMsg:
			progressCount++
			message = waitAuditEvent(ctx, events)()
		case auditFinishedMsg:
			if value.err != nil {
				t.Fatalf("audit error = %v", value.err)
			}
			if value.report == nil || value.report.URL == "" {
				t.Fatalf("report = %#v", value.report)
			}
			if progressCount == 0 {
				t.Fatal("progressCount = 0")
			}
			return
		default:
			t.Fatalf("unexpected message %T", message)
		}
	}
}

func TestRunRejectsNilContextAndInvalidOptions(t *testing.T) {
	//nolint:staticcheck // The nil input is intentional and verifies Run's boundary guard.
	if err := Run(nil, audit.DefaultOptions()); err == nil {
		t.Fatal("Run(nil) error = nil")
	}

	options := audit.DefaultOptions()
	options.Concurrency = 0
	if err := Run(context.Background(), options); err == nil {
		t.Fatal("Run(invalid options) error = nil")
	}
}

func TestViewsStayWithinTerminalDimensions(t *testing.T) {
	tests := []struct {
		name   string
		width  int
		height int
	}{
		{name: "minimum", width: 70, height: 20},
		{name: "wide", width: 118, height: 30},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			base := newModel(context.Background(), audit.DefaultOptions())
			base.resize(test.width, test.height, newTheme(true))

			states := []model{base}

			running := base
			running.screen = screenRunning
			running.running = newRunningState(
				context.Background(),
				func() {},
				make(chan tea.Msg),
				"https://example.com",
				running.contentWidth(),
				running.contentHeight(),
			)
			states = append(states, running)

			results := base
			results.screen = screenResults
			results.report = resultTestReport()
			results.results = newResultsState(
				results.report,
				results.contentWidth(),
				results.contentHeight(),
				newTheme(true),
			)
			states = append(states, results)

			failed := base
			failed.screen = screenFailed
			failed.failureMessage = strings.Repeat(
				"https://example.com/a-very-long-path/",
				20,
			) + "\n" + strings.Repeat("secondary failure ", 100)
			states = append(states, failed)

			for _, state := range states {
				content := state.View().Content
				if got := lipgloss.Width(content); got > test.width {
					t.Errorf("screen %v width = %d, want <= %d", state.screen, got, test.width)
				}
				if got := lipgloss.Height(content); got > test.height {
					t.Errorf(
						"screen %v height = %d, body = %d, want <= %d",
						state.screen,
						got,
						lipgloss.Height(state.results.render(state.report, newTheme(true))),
						test.height,
					)
				}
			}
		})
	}
}
