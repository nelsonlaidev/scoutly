package tui

import (
	"image/color"
	"math"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestSetupParsesFieldsIntoAuditOptions(t *testing.T) {
	initial := audit.DefaultOptions()
	initial.Rules = audit.Rules{"title_too_short": audit.RuleLevelWarning}
	state := newSetupState(initial)
	state.values.url = "https://example.com"
	state.values.maxDepth = "2"
	state.values.maxPages = "50"
	state.values.timeout = "1500"
	state.values.rateLimit = "2.5"
	state.values.images = false

	target, options, errorsByField := parseSetupValues(*state.values)
	if len(errorsByField) != 0 {
		t.Fatalf("parseSetupValues() errors = %#v", errorsByField)
	}
	if target != "https://example.com" {
		t.Fatalf("target = %q", target)
	}
	if options.MaxDepth != 2 ||
		options.MaxPages != 50 ||
		options.Timeout != 1500*time.Millisecond ||
		options.RateLimit != 2.5 ||
		options.Images ||
		options.Rules["title_too_short"] != audit.RuleLevelWarning {
		t.Fatalf("options = %#v", options)
	}
}

func TestSetupReportsParsingAndValidationErrors(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "example.com"
	state.values.maxDepth = "not-a-number"
	state.values.maxPages = "0"
	state.values.timeout = "9223372036855"
	state.values.rateLimit = "NaN"

	_, _, errorsByField := parseSetupValues(*state.values)
	for _, id := range []fieldID{
		fieldURL,
		fieldMaxDepth,
		fieldMaxPages,
		fieldTimeout,
		fieldRateLimit,
	} {
		if errorsByField[id] == "" {
			t.Errorf("errors[%q] is empty: %#v", id, errorsByField)
		}
	}
}

func TestSetupInputValidationUsesAuditOptionRules(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "https://example.com"

	if err := validateSetupInput(state.values, fieldConcurrency)("0"); err == nil {
		t.Fatal("validate concurrency error = nil")
	}
	if err := validateSetupInput(state.values, fieldConcurrency)("1"); err != nil {
		t.Fatalf("validate concurrency error = %v", err)
	}
	if state.values.concurrency != "20" {
		t.Fatalf("validation mutated concurrency to %q", state.values.concurrency)
	}
}

func TestSetupRendersEveryAuditOptionWithoutExpansion(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()

	content := state.form.View()
	for _, label := range []string{
		"Website URL",
		"Maximum depth",
		"Maximum pages",
		"Maximum sitemap documents",
		"Timeout (ms)",
		"Maximum redirects",
		"Concurrency",
		"User agent",
		"Rate limit (requests/sec)",
		"Keep URL fragments",
		"Ignore redirect issues",
		"Respect robots.txt",
		"Discover XML sitemaps",
		"Check discovered images",
		"Run audit",
	} {
		if !strings.Contains(content, label) {
			t.Errorf("setup view does not contain %q:\n%s", label, content)
		}
	}
	if strings.Contains(content, "Advanced options") {
		t.Fatalf("setup view still contains expansion control:\n%s", content)
	}
	if strings.Contains(content, "Start with these settings") {
		t.Fatalf("Run audit control still renders a title:\n%s", content)
	}
}

func TestSetupRequiresFinalRunAuditControl(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "https://example.com"
	_ = state.form.Init()

	for range len(state.fieldOrder) - 2 {
		_ = state.form.NextGroup()
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldImages) {
		t.Fatalf("focused field = %q, want %q", key, fieldImages)
	}

	_ = state.form.NextGroup()
	if state.form.State != huh.StateNormal {
		t.Fatalf("form state = %v before Run audit", state.form.State)
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldRun) {
		t.Fatalf("focused field = %q, want %q", key, fieldRun)
	}

	_ = state.form.NextGroup()
	if state.form.State != huh.StateCompleted {
		t.Fatalf("form state = %v after Run audit", state.form.State)
	}
}

func TestSetupHidesEmbeddedFormHelp(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()

	content := ansi.Strip(state.form.View())
	for _, help := range []string{"enter next", "shift+tab back"} {
		if strings.Contains(content, help) {
			t.Fatalf("setup form still renders embedded help %q:\n%s", help, content)
		}
	}
}

func TestSetupUsesDynamicTwoColumnGrid(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()

	content := ansi.Strip(state.form.View())
	lineContaining := func(label string) string {
		for line := range strings.SplitSeq(content, "\n") {
			if strings.Contains(line, label) {
				return line
			}
		}
		return ""
	}
	for _, pair := range [][2]string{
		{"Website URL", "Maximum depth"},
		{"Discover XML sitemaps", "Check discovered images"},
	} {
		line := lineContaining(pair[0])
		if line == "" || !strings.Contains(line, pair[1]) {
			t.Errorf("fields %q and %q are not in the same row:\n%s", pair[0], pair[1], content)
		}
	}
	if strings.Contains(content, "\n\n\n") {
		t.Fatalf("setup grid contains empty rows:\n%s", content)
	}
}

func TestSetupRunAuditControlFillsFinalRow(t *testing.T) {
	const width = 100
	state := newSetupState(audit.DefaultOptions())
	state.resize(width, 80)
	_ = state.form.Init()

	content := ansi.Strip(state.form.View())
	for y, line := range strings.Split(content, "\n") {
		if !strings.Contains(line, runButtonLabel) {
			continue
		}
		if got := lipgloss.Width(line); got != width {
			t.Fatalf("Run audit row width = %d, want %d:\n%s", got, width, content)
		}
		click, ok := state.fieldClickAt(width-1, y)
		if !ok || !click.submit {
			t.Fatalf("right edge of Run audit row is not clickable: click=%#v ok=%t", click, ok)
		}
		return
	}
	t.Fatalf("setup view does not contain %q:\n%s", runButtonLabel, content)
}

func TestSetupRendersConfirmOptionsBelowTitles(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()

	content := ansi.Strip(state.form.View())
	if count := strings.Count(content, "Yes"); count != 5 {
		t.Fatalf("affirmative option count = %d, want 5:\n%s", count, content)
	}
	if count := strings.Count(content, "No"); count != 5 {
		t.Fatalf("negative option count = %d, want 5:\n%s", count, content)
	}
	lines := strings.Split(content, "\n")
	for _, title := range []string{
		"Keep URL fragments",
		"Ignore redirect issues",
		"Respect robots.txt",
		"Discover XML sitemaps",
		"Check discovered images",
	} {
		titleLine := -1
		for index, line := range lines {
			if strings.Contains(line, title) {
				titleLine = index
				break
			}
		}
		if titleLine < 0 ||
			titleLine+1 >= len(lines) ||
			!strings.Contains(lines[titleLine+1], "Yes") ||
			!strings.Contains(lines[titleLine+1], "No") {
			t.Errorf("confirm %q does not render options directly below its title:\n%s", title, content)
		}
	}
}

func TestSetupShowsScrollBoxOnlyOnVerticalOverflow(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(78, 12)
	_ = state.form.Init()
	state.syncViewport(true)

	if !state.overflow {
		t.Fatal("setup overflow = false, want true")
	}
	content := state.view(newTheme(true))
	if got := lipgloss.Width(content); got != 78 {
		t.Errorf("scroll box width = %d, want 78", got)
	}
	if got := lipgloss.Height(content); got != 12 {
		t.Errorf("scroll box height = %d, want 12", got)
	}
	if strings.ContainsAny(content, "↑↓") {
		t.Fatalf("scroll box renders an arrow indicator at the top:\n%s", content)
	}
	lines := strings.Split(ansi.Strip(content), "\n")
	if len(lines) < 3 || !strings.HasSuffix(lines[1], scrollbarThumb) {
		t.Fatalf("scrollbar thumb is not at the top of the right border:\n%s", content)
	}

	state.viewport.GotoBottom()
	content = state.view(newTheme(true))
	if strings.ContainsAny(content, "↑↓") {
		t.Fatalf("scroll box renders an arrow indicator at the bottom:\n%s", content)
	}
	lines = strings.Split(ansi.Strip(content), "\n")
	if len(lines) < 3 || !strings.HasSuffix(lines[len(lines)-2], scrollbarThumb) {
		t.Fatalf("scrollbar thumb is not at the bottom of the right border:\n%s", content)
	}

	state.resize(100, 80)
	if state.overflow {
		t.Fatal("setup overflow = true after growing viewport")
	}
}

func TestSetupScrollFollowsFocusedRow(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "https://example.com"
	state.resize(78, 12)
	_ = state.form.Init()

	for range len(state.fieldOrder) - 1 {
		_ = state.form.NextGroup()
	}
	state.syncViewport(true)
	if state.viewport.YOffset() == 0 {
		t.Fatal("viewport offset = 0 after focusing the last row")
	}
	if content := ansi.Strip(state.view(newTheme(true))); !strings.Contains(content, "Run audit") {
		t.Fatalf("focused last row is not visible:\n%s", content)
	}

	for range len(state.fieldOrder) - 1 {
		_ = state.form.PrevGroup()
	}
	state.syncViewport(true)
	if state.viewport.YOffset() != 0 {
		t.Fatalf("viewport offset = %d after returning to the first row", state.viewport.YOffset())
	}
}

func TestSetupScrollBoxHandlesPageKeysWithoutMovingFormFocus(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(78, 12)
	_ = state.form.Init()
	state.syncViewport(true)
	focused := state.form.GetFocusedField().GetKey()

	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyPgDown})
	if state.viewport.YOffset() == 0 {
		t.Fatal("Page Down did not scroll the overflowing setup form")
	}
	if got := state.form.GetFocusedField().GetKey(); got != focused {
		t.Fatalf("focused field = %q after Page Down, want %q", got, focused)
	}

	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeyPgUp})
	if state.viewport.YOffset() != 0 {
		t.Fatalf("viewport offset = %d after Page Up", state.viewport.YOffset())
	}
}

func TestSetupSpaceTogglesBooleanOptions(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(78, 20)
	_ = state.form.Init()

	for range 9 {
		_ = state.form.NextGroup()
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldKeepFragments) {
		t.Fatalf("focused field = %q, want %q", key, fieldKeepFragments)
	}

	before := state.values.keepFragments
	_, _ = state.update(tea.KeyPressMsg{Code: tea.KeySpace})
	if state.values.keepFragments == before {
		t.Fatal("Space did not toggle keep fragments")
	}
}

func TestSetupResetPreservesValuesAndRestartsForm(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "https://example.com"
	state.form.State = huh.StateCompleted

	if cmd := state.reset(78, 20); cmd == nil {
		t.Fatal("reset command = nil")
	}
	if state.values.url != "https://example.com" {
		t.Fatalf("URL after reset = %q", state.values.url)
	}
	if state.form.State != huh.StateNormal {
		t.Fatalf("form state = %v", state.form.State)
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldURL) {
		t.Fatalf("focused field = %q, want %q", key, fieldURL)
	}
}

func TestSetupResetWithValidationErrorsFocusesFirstInvalidField(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.values.url = "https://example.com"
	state.values.maxDepth = "invalid"
	state.values.concurrency = "0"
	_, _, errorsByField := parseSetupValues(*state.values)

	if cmd := state.resetWithValidationErrors(78, 20, errorsByField); cmd == nil {
		t.Fatal("reset command = nil")
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldMaxDepth) {
		t.Fatalf("focused field = %q, want %q", key, fieldMaxDepth)
	}
	if err := state.form.GetFocusedField().Error(); err == nil {
		t.Fatal("focused field error = nil")
	}
	if got, want := state.validationError, "Enter a whole number"; got != want {
		t.Fatalf("validation error = %q, want %q", got, want)
	}
}

func TestSetupTabShowsValidationErrorWithoutLeavingInvalidField(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()

	action, _ := state.update(tea.KeyPressMsg{Code: tea.KeyTab})
	if action != setupNone {
		t.Fatalf("Tab action = %v, want setupNone", action)
	}
	if key := state.form.GetFocusedField().GetKey(); key != string(fieldURL) {
		t.Fatalf("focused field = %q, want %q", key, fieldURL)
	}
	if got, want := state.validationError, "Enter a valid HTTP or HTTPS URL"; got != want {
		t.Fatalf("validation error = %q, want %q", got, want)
	}
	lines := strings.Split(ansi.Strip(state.view(newTheme(true))), "\n")
	if !strings.Contains(lines[len(lines)-1], state.validationError) {
		t.Fatalf("validation error is not below the form:\n%s", strings.Join(lines, "\n"))
	}
}

func TestSetupTabMovesToNextFieldWhenCurrentValueIsValid(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(100, 80)
	_ = state.form.Init()
	_, _ = state.update(tea.PasteMsg{Content: "https://example.com"})

	_, command := state.update(tea.KeyPressMsg{Code: tea.KeyTab})
	steps := 0
	var runCommand func(tea.Cmd)
	runCommand = func(command tea.Cmd) {
		if command == nil ||
			state.form.GetFocusedField().GetKey() != string(fieldURL) {
			return
		}
		steps++
		if steps > 20 {
			t.Fatal("Tab command did not settle")
		}
		message := command()
		if batch, ok := message.(tea.BatchMsg); ok {
			for _, batchedCommand := range batch {
				runCommand(batchedCommand)
			}
			return
		}
		_, nextCommand := state.update(message)
		runCommand(nextCommand)
	}
	runCommand(command)

	if key := state.form.GetFocusedField().GetKey(); key != string(fieldMaxDepth) {
		t.Fatalf("focused field = %q, want %q", key, fieldMaxDepth)
	}
	if state.validationError != "" {
		t.Fatalf("validation error = %q, want empty", state.validationError)
	}
}

func TestSetupValidationErrorFillsAvailableWidth(t *testing.T) {
	for _, test := range []struct {
		name   string
		width  int
		height int
	}{
		{name: "without overflow", width: 100, height: 80},
		{name: "with overflow", width: 78, height: 12},
	} {
		t.Run(test.name, func(t *testing.T) {
			state := newSetupState(audit.DefaultOptions())
			_, _, errorsByField := parseSetupValues(*state.values)
			_ = state.resetWithValidationErrors(test.width, test.height, errorsByField)

			view := state.view(newTheme(true))
			styledLines := strings.Split(view, "\n")
			wantError := state.theme.Theme(false).Focused.ErrorMessage.
				Width(test.width).
				Render(state.validationError)
			if got := styledLines[len(styledLines)-1]; got != wantError {
				t.Fatalf("validation error style changed:\ngot:  %q\nwant: %q", got, wantError)
			}

			content := ansi.Strip(view)
			if count := strings.Count(content, state.validationError); count != 1 {
				t.Fatalf("validation error count = %d, want 1:\n%s", count, content)
			}
			lines := strings.Split(content, "\n")
			for index, line := range lines {
				if !strings.Contains(line, state.validationError) {
					continue
				}
				if got := lipgloss.Width(line); got != test.width {
					t.Fatalf("validation error width = %d, want %d:\n%s", got, test.width, content)
				}
				if index != len(lines)-1 {
					t.Fatalf("validation error row = %d, want %d:\n%s", index, len(lines)-1, content)
				}
				if _, _, ok := state.formCoordinates(0, index); ok {
					t.Fatal("validation error banner is treated as form content")
				}
				return
			}
			t.Fatalf("setup view does not contain validation error:\n%s", content)
		})
	}
}

func TestSetupBackgroundColorUpdatesSharedTheme(t *testing.T) {
	state := newSetupState(audit.DefaultOptions())
	state.resize(78, 20)
	_ = state.form.Init()
	theme := state.theme

	_, _ = state.update(tea.BackgroundColorMsg{Color: color.White})
	if state.theme.dark {
		t.Fatal("setup theme remained dark after a light background response")
	}

	_ = state.reset(78, 20)
	if state.theme != theme {
		t.Fatal("setup reset replaced the shared theme")
	}

	_, _ = state.update(tea.BackgroundColorMsg{Color: color.Black})
	if !state.theme.dark {
		t.Fatal("setup theme remained light after a dark background response")
	}
}

func TestMaximumTimeoutConstantMatchesDurationRange(t *testing.T) {
	if maxTimeoutMilliseconds != math.MaxInt64/int64(time.Millisecond) {
		t.Fatalf("maxTimeoutMilliseconds = %d", maxTimeoutMilliseconds)
	}
}
