package tui

import (
	"strconv"
	"strings"

	"charm.land/bubbles/v2/key"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

type setupAction uint8

const (
	setupNone setupAction = iota
	setupStart
	setupQuit
)

type setupKeyMap struct {
	Move   key.Binding
	Back   key.Binding
	Toggle key.Binding
	Quit   key.Binding
}

func (k setupKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{}
}

func (k setupKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{k.Move}, {k.Back}, {k.Toggle}, {k.Quit}}
}

type setupState struct {
	values          *setupValues
	form            *huh.Form
	theme           *setupTheme
	fieldOrder      []fieldID
	viewport        viewport.Model
	width           int
	height          int
	overflow        bool
	keymap          setupKeyMap
	validationError string
}

func newSetupState(options audit.Options) setupState {
	values := &setupValues{
		maxDepth:            strconv.Itoa(options.MaxDepth),
		maxPages:            strconv.Itoa(options.MaxPages),
		maxSitemapDocuments: strconv.Itoa(options.MaxSitemapDocuments),
		timeout:             strconv.FormatInt(options.Timeout.Milliseconds(), 10),
		maxRedirects:        strconv.Itoa(options.MaxRedirects),
		concurrency:         strconv.Itoa(options.Concurrency),
		userAgent:           options.UserAgent,
		keepFragments:       options.KeepFragments,
		ignoreRedirects:     options.IgnoreRedirects,
		respectRobots:       options.RespectRobots,
		sitemaps:            options.Sitemaps,
		images:              options.Images,
	}
	if options.RateLimit != 0 {
		values.rateLimit = strconv.FormatFloat(options.RateLimit, 'g', -1, 64)
	}

	formTheme := &setupTheme{dark: true}
	form, fieldOrder := newSetupForm(values, formTheme)
	formViewport := viewport.New()
	formViewport.MouseWheelEnabled = true
	formViewport.SetHorizontalStep(0)
	formViewport.Style = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1)

	state := setupState{
		values:     values,
		form:       form,
		theme:      formTheme,
		fieldOrder: fieldOrder,
		viewport:   formViewport,
		keymap: setupKeyMap{
			Move: key.NewBinding(
				key.WithKeys("tab", "enter"),
				key.WithHelp("Tab/Enter", "move"),
			),
			Back: key.NewBinding(
				key.WithKeys("shift+tab"),
				key.WithHelp("Shift+Tab", "back"),
			),
			Toggle: key.NewBinding(
				key.WithKeys("space"),
				key.WithHelp("Space", "toggle"),
			),
			Quit: key.NewBinding(
				key.WithKeys("ctrl+c"),
				key.WithHelp("Ctrl+C", "quit"),
			),
		},
	}
	contentWidth, contentHeight := shell.contentSize(defaultWidth, defaultHeight)
	state.resize(contentWidth, contentHeight)
	return state
}

func (state *setupState) reset(width, height int) tea.Cmd {
	state.form, state.fieldOrder = newSetupForm(state.values, state.theme)
	state.validationError = ""
	state.viewport.GotoTop()
	state.resize(width, height)
	return tea.Batch(state.form.Init(), tea.RequestBackgroundColor)
}

func (state *setupState) resetWithValidationErrors(
	width, height int,
	errorsByField map[fieldID]string,
) tea.Cmd {
	cmd := state.reset(width, height)
	invalidIndex := -1
	for index, id := range state.fieldOrder {
		if errorsByField[id] != "" {
			invalidIndex = index
			break
		}
	}
	if invalidIndex < 0 {
		return cmd
	}

	for range invalidIndex {
		_ = state.form.GetFocusedField().Blur()
		_ = state.form.NextGroup()
	}
	field := state.form.GetFocusedField()
	_ = field.Blur()
	focusCmd := field.Focus()
	if err := field.Error(); err != nil {
		state.validationError = err.Error()
	} else {
		state.validationError = errorsByField[fieldID(field.GetKey())]
	}
	state.syncViewport(true)
	return tea.Batch(cmd, focusCmd)
}

func (state *setupState) resize(width, height int) {
	state.width = max(1, width)
	state.height = max(1, height)
	state.syncViewport(true)
}

func (state *setupState) update(msg tea.Msg) (setupAction, tea.Cmd) {
	if message, ok := msg.(tea.BackgroundColorMsg); ok {
		state.theme.dark = message.IsDark()
	}

	previousFocus := fieldID(state.form.GetFocusedField().GetKey())
	previousValues := *state.values
	var viewportCmd tea.Cmd
	viewportScroll := false
	if state.overflow {
		switch message := msg.(type) {
		case tea.MouseWheelMsg:
			viewportScroll = true
			state.viewport, viewportCmd = state.viewport.Update(message)
		case tea.KeyPressMsg:
			if message.String() == "pgup" || message.String() == "pgdown" {
				viewportScroll = true
				state.viewport, viewportCmd = state.viewport.Update(message)
			}
		}
	}

	updated, cmd := state.form.Update(msg)
	form, ok := updated.(*huh.Form)
	if ok {
		state.form = form
	}
	if *state.values != previousValues {
		state.validationError = ""
	} else if err := state.form.GetFocusedField().Error(); err != nil {
		state.validationError = err.Error()
	}
	focusChanged := previousFocus != fieldID(state.form.GetFocusedField().GetKey())
	state.syncViewport(focusChanged || !viewportScroll)
	cmd = tea.Batch(cmd, viewportCmd)

	switch state.form.State {
	case huh.StateCompleted:
		return setupStart, cmd
	case huh.StateAborted:
		return setupQuit, cmd
	default:
		return setupNone, cmd
	}
}

func (state *setupState) syncViewport(ensureFocus bool) {
	formHeight := state.height
	if state.validationError != "" {
		formHeight = max(1, formHeight-1)
	}
	state.form.WithWidth(state.width).WithHeight(formHeight)
	content := state.form.View()
	state.overflow = lipgloss.Height(content) > formHeight

	state.viewport.SetWidth(state.width)
	state.viewport.SetHeight(formHeight)
	if !state.overflow {
		state.viewport.SetContent(content)
		state.viewport.GotoTop()
		return
	}

	contentWidth := max(1, state.width-state.viewport.Style.GetHorizontalFrameSize())
	contentHeight := max(1, formHeight-state.viewport.Style.GetVerticalFrameSize())
	state.form.WithWidth(contentWidth).WithHeight(contentHeight)
	content = state.form.View()
	state.viewport.SetContent(content)
	if ensureFocus {
		state.ensureFocusedRowVisible(content)
	}
}

func (state *setupState) ensureFocusedRowVisible(content string) {
	focused := fieldID(state.form.GetFocusedField().GetKey())
	focusedIndex := -1
	for index, id := range state.fieldOrder {
		if id == focused {
			focusedIndex = index
			break
		}
	}
	if focusedIndex < 0 {
		return
	}

	rowIndex := focusedIndex / setupGridColumns
	rows := strings.Split(content, "\n\n")
	if rowIndex >= len(rows) {
		return
	}

	rowTop := 0
	for index := range rowIndex {
		// LayoutGrid separates rendered rows with one empty line.
		rowTop += lipgloss.Height(rows[index]) + 1
	}
	rowBottom := rowTop + lipgloss.Height(rows[rowIndex])
	visibleTop := state.viewport.YOffset()
	visibleHeight := state.viewport.Height() - state.viewport.Style.GetVerticalFrameSize()
	switch {
	case rowTop < visibleTop:
		state.viewport.SetYOffset(rowTop)
	case rowBottom > visibleTop+visibleHeight:
		state.viewport.SetYOffset(rowBottom - visibleHeight)
	}
}

func (state setupState) view(theme theme) string {
	var content string
	if !state.overflow {
		content = state.form.View()
	} else {
		formViewport := state.viewport
		formViewport.Style = formViewport.Style.
			BorderForeground(theme.colors.border)
		scrollbar := renderScrollbar(
			formViewport.Height()-formViewport.Style.GetVerticalFrameSize(),
			formViewport.TotalLineCount(),
			formViewport.VisibleLineCount(),
			formViewport.YOffset(),
			true,
			theme,
		)
		content = renderScrollbarOnRightBorder(formViewport.View(), scrollbar)
	}
	if state.validationError == "" {
		return content
	}

	errorMessage := state.theme.Theme(false).Focused.ErrorMessage.
		Width(state.width).
		Render(state.validationError)
	return lipgloss.JoinVertical(lipgloss.Left, content, errorMessage)
}
