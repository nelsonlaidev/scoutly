package tui

import (
	"context"
	"errors"
	"image/color"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"charm.land/bubbles/v2/table"
	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestShortHelpKeyMaps(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	if len(state.statusKeymap.ShortHelp()) != 2 {
		t.Fatal("status short help does not contain both bindings")
	}
	if len(state.resizeKeymap.ShortHelp()) != 1 {
		t.Fatal("resize short help does not contain the quit binding")
	}
	if len(state.setup.keymap.ShortHelp()) != 0 {
		t.Fatal("setup short help should be empty")
	}
	state.running = newRunningState(context.Background(), func() {}, make(chan tea.Msg), "", 10, 10)
	if len(state.running.keymap.ShortHelp()) != 1 {
		t.Fatal("running short help does not contain the cancel binding")
	}
	results := newResultsState(resultTestReport(), 118, 30, newTheme(true))
	if len(results.keymap.ShortHelp()) != 7 {
		t.Fatal("results short help does not contain every binding")
	}
}

func TestModelInitializationAndGlobalMessages(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	if command := state.Init(); command == nil {
		t.Fatal("Init() command = nil")
	}

	updated, _ := state.Update(tea.WindowSizeMsg{Width: 90, Height: 25})
	state = updated.(model)
	if state.width != 90 || state.height != 25 {
		t.Fatalf("window size = %dx%d", state.width, state.height)
	}

	updated, _ = state.Update(tea.BackgroundColorMsg{Color: color.White})
	state = updated.(model)
	if state.darkBackground {
		t.Fatal("light background was classified as dark")
	}

	if updated, _ := state.Update(keyRune('c')); updated.(model).screen != screenSetup {
		t.Fatal("ordinary setup key changed the screen")
	}
	if _, command := state.Update(tea.KeyPressMsg{Code: 'c', Mod: tea.ModCtrl}); command == nil {
		t.Fatal("Ctrl+C outside a running audit did not quit")
	}

	state.screen = screen(255)
	if got := state.activeKeyMap(); len(got.FullHelp()) != 1 {
		t.Fatal("unknown screen did not use resize key map")
	}
	_ = state.View()
}

func TestModelIgnoresStaleAuditMessages(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	if updated, command := state.Update(auditProgressMsg{}); updated.(model).screen != screenSetup || command != nil {
		t.Fatal("setup accepted a stale progress message")
	}
	if updated, command := state.Update(auditFinishedMsg{report: resultTestReport()}); updated.(model).screen != screenSetup || command != nil {
		t.Fatal("setup accepted a stale finished message")
	}
	if updated, command := state.Update(elapsedTickMsg{}); updated.(model).screen != screenSetup || command != nil {
		t.Fatal("setup accepted a stale elapsed message")
	}
}

func TestModelAcceptsCurrentAuditProgressAndStatusRetry(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	events := make(chan tea.Msg)
	state.screen = screenRunning
	state.running = newRunningState(context.Background(), func() {}, events, "target", 80, 20)
	updated, command := state.Update(auditProgressMsg{progress: audit.Progress{CurrentURL: "https://example.com"}})
	state = updated.(model)
	if command == nil || len(state.running.activity) != 1 {
		t.Fatal("current progress was not recorded")
	}
	if updated, command = state.Update(struct{}{}); updated.(model).screen != screenRunning || command != nil {
		t.Fatal("running screen reacted to an unrelated message")
	}

	state.screen = screenCanceled
	_ = state.View()
	updated, command = state.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	state = updated.(model)
	if state.screen != screenSetup || command == nil {
		t.Fatal("status retry did not reset setup")
	}
}

func TestModelRoutesRemainingScreenActions(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.resize(118, 30, newTheme(true))

	state.setup.form.State = huh.StateAborted
	if _, command := state.Update(struct{}{}); command == nil {
		t.Fatal("aborted setup did not return quit command")
	}

	state.screen = screenRunning
	state.running = newRunningState(context.Background(), func() {}, make(chan tea.Msg), "target", 118, 30)
	if updated, command := state.Update(tea.MouseWheelMsg{X: -1, Y: 0, Button: tea.MouseWheelUp}); updated.(model).screen != screenRunning || command != nil {
		t.Fatal("out-of-bounds running wheel changed state")
	}

	report := resultTestReport()
	state.screen = screenResults
	state.report = report
	state.results = newResultsState(report, 118, 30, newTheme(true))
	if _, command := state.Update(keyRune('q')); command == nil {
		t.Fatal("results quit did not return a command")
	}
	if _, command := state.Update(tea.MouseWheelMsg{X: 1, Y: 1, Button: tea.MouseWheelDown}); command != nil {
		t.Fatal("results wheel above content returned a command")
	}
	updated, command := state.Update(keyRune('n'))
	state = updated.(model)
	if state.screen != screenSetup || command == nil {
		t.Fatal("results new audit did not reset setup")
	}

	for _, targetScreen := range []screen{screenCanceled, screenFailed} {
		state.screen = targetScreen
		if updated, command = state.Update(struct{}{}); updated.(model).screen != targetScreen || command != nil {
			t.Fatalf("screen %v reacted to non-key input", targetScreen)
		}
		if updated, command = state.Update(keyRune('x')); updated.(model).screen != targetScreen || command != nil {
			t.Fatalf("screen %v reacted to an unknown key", targetScreen)
		}
		if _, command = state.Update(keyRune('q')); command == nil {
			t.Fatalf("screen %v did not quit", targetScreen)
		}
	}
}

func TestModelFinishWithoutReportAndResizeActiveScreens(t *testing.T) {
	state := newModel(context.Background(), audit.DefaultOptions())
	state.screen = screenRunning
	state.running = newRunningState(context.Background(), func() {}, make(chan tea.Msg), "target", 80, 20)
	state.finishAudit(auditFinishedMsg{})
	if state.screen != screenFailed || state.failureMessage != "The audit completed without a report." {
		t.Fatalf("finishAudit() = screen %v, message %q", state.screen, state.failureMessage)
	}

	state.screen = screenRunning
	state.running = newRunningState(context.Background(), func() {}, make(chan tea.Msg), "target", 80, 20)
	state.resize(70, 20, newTheme(true))
	if state.width != 70 || state.height != 20 || state.running.width != state.contentWidth() {
		t.Fatalf("running resize = %#v", state)
	}

	state.screen = screenResults
	state.report = resultTestReport()
	state.results = newResultsState(state.report, 80, 20, newTheme(true))
	state.resize(100, 25, newTheme(false))
	if state.results.width != state.contentWidth() {
		t.Fatal("results were not resized with the model")
	}
}

func TestResultsKeyboardNavigationEdges(t *testing.T) {
	report := resultTestReport()
	state := newResultsState(report, 118, 30, newTheme(true))

	for _, key := range []tea.KeyPressMsg{keyRune('2'), {Code: tea.KeyRight}, {Code: tea.KeyLeft}} {
		if _, _ = state.update(key, report); state.tab == "" {
			t.Fatal("navigation produced an empty tab")
		}
	}
	if action, _ := state.update(keyRune('q'), report); action != resultQuit {
		t.Fatalf("q action = %v", action)
	}
	if action, _ := state.update(keyRune('n'), report); action != resultNewAudit {
		t.Fatalf("n action = %v", action)
	}

	state.changeTab(tabOverview, report)
	if _, command := state.update(keyRune('/'), report); command != nil || state.focus == focusSearch {
		t.Fatal("overview opened search")
	}
	if _, _ = state.update(keyRune('f'), report); state.currentFilter() != "none" {
		t.Fatal("overview changed a filter")
	}
	if _, _ = state.update(keyRune('1'), report); state.tab != tabOverview {
		t.Fatal("numeric overview navigation failed")
	}

	state.changeTab(tabLinks, report)
	if _, command := state.update(keyRune('/'), report); command == nil || state.focus != focusSearch {
		t.Fatal("search did not receive focus")
	}
	for _, key := range []tea.KeyPressMsg{{Code: tea.KeyEscape}, {Code: tea.KeyTab}, {Code: tea.KeyEnter}} {
		state.focus = focusSearch
		if _, _ = state.update(key, report); state.focus != focusList {
			t.Fatalf("%s did not close search", key.String())
		}
	}

	state.focus = focusList
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyTab}, report); state.focus != focusDetail {
		t.Fatal("tab did not focus detail")
	}
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyTab}, report); state.focus != focusList {
		t.Fatal("tab did not return to list")
	}

	state.width = 80
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEnter}, report); !state.detailOpen || state.focus != focusDetail {
		t.Fatal("enter did not open narrow detail")
	}
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEnter}, report); state.detailOpen || state.focus != focusList {
		t.Fatal("second enter did not close narrow detail")
	}
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEnter}, report); !state.detailOpen {
		t.Fatal("third enter did not reopen narrow detail")
	}
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEscape}, report); state.detailOpen || state.focus != focusList {
		t.Fatal("escape did not close narrow detail")
	}
	state.focus = focusDetail
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEscape}, report); state.focus != focusList {
		t.Fatal("escape did not leave detail")
	}
	state.query.SetValue("broken")
	if _, _ = state.update(tea.KeyPressMsg{Code: tea.KeyEscape}, report); state.query.Value() != "" {
		t.Fatal("escape did not clear query")
	}

	state.focus = focusDetail
	_, _ = state.update(tea.MouseWheelMsg{X: 0, Y: 0, Button: tea.MouseWheelDown}, report)
	state.focus = focusList
	state.table.SetCursor(0)
	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyDown}, report)

	if got := tabIndex(resultTab("missing")); got != 0 {
		t.Fatalf("missing tab index = %d", got)
	}
}

func TestResultFiltersAndDescriptionsCoverEveryKind(t *testing.T) {
	status := 204
	redirect := "https://example.com/final"
	contentType := "image/webp"
	report := resultTestReport()
	report.Pages = append(report.Pages, audit.Page{URL: "https://example.com/failed", StatusCode: nil})
	report.Links = append(report.Links,
		audit.Link{URL: "https://example.com/blocked", Result: audit.LinkResult{Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &redirect, Reason: audit.FailureAntiBotChallenge}},
		audit.Link{URL: "https://example.com/weird", Result: audit.LinkResult{Kind: audit.ResultInvalid, Reason: audit.FailureInvalidURL}},
	)
	report.Images = append(report.Images,
		audit.Image{URL: "https://example.com/redirect.webp", Result: audit.ImageResult{Kind: audit.ResultResponse, StatusCode: &status, FinalURL: &redirect, ContentType: &contentType}},
		audit.Image{URL: "https://example.com/blocked.webp", Result: audit.ImageResult{Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &redirect, ContentType: &contentType, Reason: audit.FailureAntiBotChallenge}},
		audit.Image{URL: "bad image", Result: audit.ImageResult{Kind: audit.ResultInvalid, Reason: audit.FailureInvalidURL}},
		audit.Image{URL: "data:image/png;base64,x", Result: audit.ImageResult{Kind: audit.ResultSkipped, Reason: audit.FailureUnsupportedProtocol}},
	)
	report.Issues = append(report.Issues,
		audit.Issue{Code: audit.IssueBrokenImage, Severity: audit.SeverityError, Message: "broken", Target: audit.IssueTarget{Type: audit.TargetImage, URL: "bad image"}},
	)

	for _, filter := range []string{"failed", "healthy", "non-2xx", "other"} {
		_ = selectResultItems(report, tabPages, "", filter)
	}
	for _, filter := range []string{"healthy", "broken", "blocked", "redirected", "skipped", "other"} {
		_ = selectResultItems(report, tabLinks, "", filter)
	}
	for _, filter := range []string{"healthy", "broken", "blocked", "invalid", "redirected", "skipped", "other"} {
		_ = selectResultItems(report, tabImages, "", filter)
	}
	_ = selectResultItems(report, resultTab("missing"), "", "all")
	_ = selectResultItems(nil, tabIssues, "", "all")
	for _, tab := range []resultTab{tabIssues, tabPages, tabLinks, tabImages} {
		if got := selectResultItems(report, tab, "query-that-does-not-match", "all"); len(got) != 0 {
			t.Fatalf("query unexpectedly matched %q: %#v", tab, got)
		}
	}

	for _, link := range report.Links {
		_ = describeLink(link)
	}
	for _, image := range report.Images {
		_ = describeImage(image)
	}
}

func TestDetailLinesCoverEveryResourceShape(t *testing.T) {
	report := resultTestReport()
	status := 403
	finalURL := "https://example.com/final"
	contentType := "image/png"
	items := []resultItem{
		{kind: resultIssue, issue: audit.Issue{Target: audit.IssueTarget{Type: audit.TargetLink, URL: report.Links[0].URL}}},
		{kind: resultIssue, issue: audit.Issue{Target: audit.IssueTarget{Type: audit.TargetImage, URL: report.Images[0].URL}}},
		{kind: resultIssue, issue: audit.Issue{Target: audit.IssueTarget{Type: audit.TargetPage, URL: "missing"}}},
		{kind: resultPage, page: audit.Page{URL: "page", Depth: 1}},
		{kind: resultLink, link: audit.Link{URL: "blocked", Result: audit.LinkResult{Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &finalURL, Reason: audit.FailureAntiBotChallenge}}},
		{kind: resultLink, link: audit.Link{URL: "failed", Result: audit.LinkResult{Kind: audit.ResultFailed, Reason: audit.FailureConnectionFailed}}},
		{kind: resultImage, image: audit.Image{URL: "blocked", Result: audit.ImageResult{Kind: audit.ResultBlocked, StatusCode: &status, FinalURL: &finalURL, ContentType: &contentType, Reason: audit.FailureAntiBotChallenge}}},
		{kind: resultImage, image: audit.Image{URL: "failed", Result: audit.ImageResult{Kind: audit.ResultFailed, Reason: audit.FailureConnectionFailed}}},
		{kind: resultKind("missing")},
	}
	for _, item := range items {
		if lines := createDetailLines(item, report); len(lines) == 0 {
			t.Fatalf("empty detail for %#v", item)
		}
	}
	if intValue(nil, "fallback") != "fallback" || intValue(&status, "") != "403" {
		t.Fatal("intValue did not handle both pointer states")
	}
}

func TestResultMouseAndRenderingEdges(t *testing.T) {
	report := resultManyLinksReport(30)
	theme := newTheme(true)
	state := newResultsState(report, 118, 20, theme)
	state.changeTab(tabLinks, report)

	if command := state.updateMouseWheel(tea.Mouse{X: -1, Y: 0, Button: tea.MouseWheelUp}, report); command != nil {
		t.Fatal("out-of-bounds wheel returned a command")
	}
	state.tab = tabOverview
	if command := state.updateMouseWheel(tea.Mouse{X: 0, Y: 0, Button: tea.MouseWheelDown}, report); command != nil {
		t.Fatal("wheel above overview returned a command")
	}
	state.tab = tabLinks
	state.items = nil
	if command := state.updateMouseWheel(tea.Mouse{X: 1, Y: resultListHeaderRows, Button: tea.MouseWheelDown}, report); command != nil {
		t.Fatal("empty list wheel returned a command")
	}
	state.refresh(report)
	for _, button := range []tea.MouseButton{tea.MouseWheelUp, tea.MouseWheelDown, tea.MouseLeft} {
		_ = state.updateMouseWheel(tea.Mouse{X: 1, Y: state.listDataTop(), Button: button}, report)
	}
	_ = state.updateMouseWheel(tea.Mouse{X: state.width - 1, Y: state.listDataTop(), Button: tea.MouseWheelDown}, report)

	if pane := state.mousePaneAt(-1, 0); pane != resultMousePaneNone {
		t.Fatalf("negative mouse pane = %v", pane)
	}
	state.width = 80
	state.detailOpen = false
	if state.mousePaneAt(1, resultListHeaderRows) != resultMousePaneList {
		t.Fatal("narrow list pane not detected")
	}
	state.detailOpen = true
	if state.mousePaneAt(1, resultListHeaderRows) != resultMousePaneDetail {
		t.Fatal("narrow detail pane not detected")
	}
	state.width = 118
	if state.mousePaneAt((state.width-1)/2, resultListHeaderRows) != resultMousePaneNone {
		t.Fatal("wide pane divider was clickable")
	}

	if _, ok := resultTabAt(10_000, resultTabRow); ok {
		t.Fatal("result tab hit outside labels")
	}
	if _, ok := overviewMetricTabAt(10_000, resultContentTop, 100); ok {
		t.Fatal("overview metric hit outside width")
	}
	firstMetricWidth := overviewMetricWidths(100)[0]
	if _, ok := overviewMetricTabAt(firstMetricWidth, resultContentTop, 100); ok {
		t.Fatal("overview metric gap was clickable")
	}
	if _, ok := state.visibleResultIndex(-1, theme); ok {
		t.Fatal("negative visible row was accepted")
	}
	if selectedVisibleResultRow([]string{"plain"}, theme) != -1 {
		t.Fatal("plain row was classified as selected")
	}
	plainTheme := theme
	plainTheme.table.Selected = table.DefaultStyles().Selected
	_ = selectedVisibleResultRow([]string{"plain"}, plainTheme)
	brokenState := newResultsState(report, 118, 20, theme)
	brokenState.changeTab(tabLinks, report)
	brokenState.table.SetHeight(100)
	if _, ok := brokenState.visibleResultIndex(99, theme); ok {
		t.Fatal("row beyond rendered table lines was accepted")
	}
	brokenState.table.SetRows([]table.Row{{"", ""}})
	brokenState.items = []resultItem{{kind: resultLink}}
	if _, ok := brokenState.visibleResultIndex(0, theme); ok {
		t.Fatal("blank rendered row was accepted")
	}
	brokenState.table.SetRows([]table.Row{{"different", "row"}})
	brokenState.items = []resultItem{{kind: resultLink}}
	mismatchedTheme := theme
	mismatchedTheme.table.Selected = table.DefaultStyles().Selected
	mismatchedTheme.table.Cell = mismatchedTheme.table.Cell.PaddingLeft(1)
	_, _ = brokenState.visibleResultIndex(0, mismatchedTheme)

	state.items = nil
	state.tab = tabLinks
	state.width = 80
	state.detailOpen = true
	_ = state.render(report, theme)
	state.width = 118
	state.detailOpen = false
	_ = state.render(report, theme)
	_ = renderOverview(nil, 20, theme)
	if overviewMetricValue(audit.Summary{}, resultTab("missing")) != 0 {
		t.Fatal("unknown overview metric was non-zero")
	}
}

func TestSetupStateAndMouseEdges(t *testing.T) {
	options := audit.DefaultOptions()
	options.RateLimit = 2.5
	state := newSetupState(options)
	if state.values.rateLimit != "2.5" {
		t.Fatalf("rate limit = %q", state.values.rateLimit)
	}

	if command := state.resetWithValidationErrors(80, 20, nil); command == nil {
		t.Fatal("reset without validation errors returned nil")
	}
	state.validationError = "fallback"
	if command := state.resetWithValidationErrors(80, 20, map[fieldID]string{fieldURL: "fallback"}); command == nil {
		t.Fatal("reset with validation errors returned nil")
	}
	state.values.url = "https://example.com"
	if command := state.resetWithValidationErrors(80, 20, map[fieldID]string{fieldKeepFragments: "fallback"}); command == nil || state.validationError != "fallback" {
		t.Fatalf("fallback validation error = %q", state.validationError)
	}

	state.resize(100, 80)
	state.validationError = "old"
	state.values.url = "https://example.com"
	_, _ = state.update(tea.BackgroundColorMsg{Color: color.White})
	if state.theme.dark {
		t.Fatal("setup theme stayed dark")
	}
	state.form.State = huh.StateAborted
	if action, _ := state.update(struct{}{}); action != setupQuit {
		t.Fatalf("aborted form action = %v", action)
	}

	state = newSetupState(audit.DefaultOptions())
	state.resize(70, 20)
	_, _ = state.update(tea.MouseWheelMsg{X: 1, Y: 1, Button: tea.MouseWheelDown})
	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyPgDown})
	state.ensureFocusedRowVisible("")
	state = newSetupState(audit.DefaultOptions())
	state.ensureFocusedRowVisible("")
	_ = state.focusField(fieldMaxPages)
	state.ensureFocusedRowVisible("")
	state.fieldOrder = nil
	state.ensureFocusedRowVisible("row")

	if action, command := state.updateClick(-1, 0); action != setupNone || command != nil {
		t.Fatal("invalid direct setup click changed state")
	}
	if _, ok := state.fieldClickAt(-1, 0); ok {
		t.Fatal("negative field click was accepted")
	}
	if command := state.focusField(fieldID("missing")); command != nil {
		t.Fatal("missing field returned a focus command")
	}
	current := fieldID(state.form.GetFocusedField().GetKey())
	if command := state.focusField(current); command == nil {
		t.Fatal("current field focus command = nil")
	}

	for _, id := range []fieldID{fieldKeepFragments, fieldIgnoreRedirects, fieldRespectRobots, fieldSitemaps, fieldImages} {
		if state.confirmValue(id) == nil {
			t.Fatalf("confirmValue(%q) = nil", id)
		}
	}
	if state.confirmValue(fieldURL) != nil {
		t.Fatal("text field returned a confirmation pointer")
	}
	style := newHuhTheme(true).Focused.FocusedButton
	if buttonContains("nothing", "Yes", 0, 0, 10, style) {
		t.Fatal("missing button label matched")
	}
	if buttonContains("Yes", "Yes", 0, 10, 20, style) {
		t.Fatal("button outside cell matched")
	}
	_ = buttonContains("Yes Yes", "Yes", 5, 2, 20, style)

	wide := newSetupState(audit.DefaultOptions())
	wide.resize(101, 80)
	firstRow, _, _ := strings.Cut(ansi.Strip(wide.form.View()), "\n\n")
	gapY := len(strings.Split(firstRow, "\n"))
	if _, ok := wide.fieldClickAt(0, gapY); ok {
		t.Fatal("blank row gap was clickable")
	}
	if _, ok := wide.fieldClickAt(100, 0); ok {
		t.Fatal("third grid column was accepted")
	}
}

func TestCycleEveryResultFilter(t *testing.T) {
	state := newResultsState(resultTestReport(), 118, 30, newTheme(true))
	for _, tab := range []resultTab{tabIssues, tabPages, tabLinks, tabImages, resultTab("missing")} {
		state.tab = tab
		state.cycleFilter()
		_ = state.availableFilters()
	}
}

func TestParseSetupValuesRemainingErrors(t *testing.T) {
	base := *newSetupState(audit.DefaultOptions()).values
	base.url = "https://example.com"
	for _, test := range []struct {
		name  string
		value string
		field fieldID
		set   func(*setupValues, string)
	}{
		{name: "timeout syntax", value: "x", field: fieldTimeout, set: func(v *setupValues, s string) { v.timeout = s }},
		{name: "timeout non-positive", value: "0", field: fieldTimeout, set: func(v *setupValues, s string) { v.timeout = s }},
		{name: "timeout too large", value: "9223372036855", field: fieldTimeout, set: func(v *setupValues, s string) { v.timeout = s }},
		{name: "rate syntax", value: "x", field: fieldRateLimit, set: func(v *setupValues, s string) { v.rateLimit = s }},
	} {
		t.Run(test.name, func(t *testing.T) {
			values := base
			test.set(&values, test.value)
			_, _, validationErrors := parseSetupValues(values)
			if validationErrors[test.field] == "" {
				t.Fatalf("errors = %#v", validationErrors)
			}
		})
	}
}

func TestRunningCommandAndCancellationEdges(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	events := make(chan tea.Msg)
	state := newRunningState(ctx, nil, events, "target", 40, 10)
	state.cancelAudit()
	state.canceling = true
	state.cancel = cancel
	state.cancelAudit()

	if command := state.updateMouseWheel(tea.Mouse{X: 0, Y: 0, Button: tea.MouseLeft}); command != nil {
		t.Fatal("invalid running wheel returned a command")
	}
	if command := state.updateMouseWheel(tea.Mouse{X: 0, Y: runningSummaryRows + currentPaneRows, Button: tea.MouseLeft}); command != nil {
		t.Fatal("non-wheel running mouse returned a command")
	}

	cancel()
	if message := waitAuditEvent(ctx, events)(); !errors.Is(message.(auditFinishedMsg).err, context.Canceled) {
		t.Fatalf("canceled wait message = %#v", message)
	}
	if sendAuditEvent(ctx, events, struct{}{}) {
		t.Fatal("sendAuditEvent succeeded after cancellation")
	}

	liveCtx, liveCancel := context.WithCancel(context.Background())
	liveEvents := make(chan tea.Msg, 1)
	if !sendAuditEvent(liveCtx, liveEvents, "message") {
		t.Fatal("sendAuditEvent failed with a buffered channel")
	}
	if got := waitAuditEvent(liveCtx, liveEvents)(); got != "message" {
		t.Fatalf("waitAuditEvent() = %#v", got)
	}
	liveCancel()

	tick := elapsedTick(liveEvents)
	if tick == nil {
		t.Fatal("elapsedTick() command = nil")
	}
	if message, ok := tick().(elapsedTickMsg); !ok || message.events != liveEvents {
		t.Fatalf("elapsed tick = %#v", message)
	}
}

func TestStartAuditStopsBlockedProgressSendAfterCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		response.Header().Set("Content-Type", "text/html")
		_, _ = response.Write([]byte("<html><title>Home</title></html>"))
	}))
	defer server.Close()

	options := audit.DefaultOptions()
	options.MaxDepth = 0
	options.MaxPages = 1
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.Concurrency = 1

	ctx, cancel := context.WithCancel(context.Background())
	events := make(chan tea.Msg)
	if _, ok := startAudit(ctx, server.URL, options, events)().(auditProgressMsg); !ok {
		cancel()
		t.Fatal("first audit event was not progress")
	}
	cancel()
	if message := waitAuditEvent(ctx, events)(); !errors.Is(message.(auditFinishedMsg).err, context.Canceled) {
		t.Fatalf("canceled audit event = %#v", message)
	}
	time.Sleep(10 * time.Millisecond)
}

func TestLayoutScrollbarTextAndThemeEdges(t *testing.T) {
	theme := newTheme(true)
	_ = renderShell(0, 0, screenSetup, "target", "footer", "content", theme)
	if renderScrollbar(0, 1, 1, 0, false, theme) != "" {
		t.Fatal("zero-height scrollbar was not empty")
	}
	for _, input := range [][2]string{{"", "bar"}, {"view", ""}} {
		if got := renderScrollbarOnRightBorder(input[0], input[1]); got != input[0] {
			t.Fatalf("renderScrollbarOnRightBorder(%q, %q) = %q", input[0], input[1], got)
		}
	}
	_ = renderScrollbarOnRightBorder("top\n\nbottom", "x")
	_ = renderScrollbarOnRightBorder("top\nmiddle\nbottom", "x\ny\nz")
	if titleCase("") != "" || titleCase("a") != "A" {
		t.Fatal("titleCase edge behavior is incorrect")
	}
	for _, state := range []screen{screenSetup, screenRunning, screenResults, screenCanceled, screenFailed} {
		_ = theme.status(state)
	}
	view := ansi.Strip(renderStatus("title", strings.Repeat("long words ", 20), theme.colors.error, 10, 3))
	if !strings.Contains(view, "…") {
		t.Fatalf("truncated status does not contain ellipsis:\n%s", view)
	}
}

func TestRunReturnsCanceledContext(t *testing.T) {
	ctx, cancel := context.WithCancelCause(context.Background())
	want := errors.New("stop TUI")
	cancel(want)
	err := Run(ctx, audit.DefaultOptions())
	if err == nil {
		t.Fatal("Run(canceled context) error = nil")
	}
}

func TestRunWithTerminalReturnsContextCause(t *testing.T) {
	terminal, err := os.OpenFile("/dev/tty", os.O_RDWR, 0)
	if err != nil {
		t.Skip("test process has no controlling terminal")
	}
	defer func() { _ = terminal.Close() }()

	originalStdin, originalStdout := os.Stdin, os.Stdout
	os.Stdin, os.Stdout = terminal, terminal
	defer func() { os.Stdin, os.Stdout = originalStdin, originalStdout }()

	ctx, cancel := context.WithCancelCause(context.Background())
	want := errors.New("terminal test stopped")
	time.AfterFunc(100*time.Millisecond, func() { cancel(want) })
	if err := Run(ctx, audit.DefaultOptions()); !errors.Is(err, want) {
		t.Fatalf("Run() error = %v, want context cause", err)
	}
}

func TestRunWithTerminalMapsInterrupt(t *testing.T) {
	terminal, err := os.OpenFile("/dev/tty", os.O_RDWR, 0)
	if err != nil {
		t.Skip("test process has no controlling terminal")
	}
	defer func() { _ = terminal.Close() }()

	originalStdin, originalStdout := os.Stdin, os.Stdout
	os.Stdin, os.Stdout = terminal, terminal
	defer func() { os.Stdin, os.Stdout = originalStdin, originalStdout }()

	time.AfterFunc(100*time.Millisecond, func() {
		process, findErr := os.FindProcess(os.Getpid())
		if findErr == nil {
			_ = process.Signal(os.Interrupt)
		}
	})
	if err := Run(context.Background(), audit.DefaultOptions()); !errors.Is(err, context.Canceled) {
		t.Fatalf("Run() error = %v, want context.Canceled", err)
	}
}
