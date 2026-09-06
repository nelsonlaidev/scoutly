package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

type setupFieldClick struct {
	field           fieldID
	confirmValue    bool
	hasConfirmValue bool
	submit          bool
}

func (state *setupState) updateClick(x, y int) (setupAction, tea.Cmd) {
	click, ok := state.fieldClickAt(x, y)
	if !ok {
		return setupNone, nil
	}

	focusCmd := state.focusField(click.field)
	if click.submit {
		return setupStart, focusCmd
	}
	if !click.hasConfirmValue {
		return setupNone, focusCmd
	}

	value := state.confirmValue(click.field)
	if value != nil && *value != click.confirmValue {
		*value = click.confirmValue
		state.validationError = ""
	}
	return setupNone, focusCmd
}

func (state *setupState) focusField(target fieldID) tea.Cmd {
	current := fieldID(state.form.GetFocusedField().GetKey())
	if current == target {
		return state.form.GetFocusedField().Focus()
	}

	targetIndex := -1
	for index, id := range state.fieldOrder {
		if id == target {
			targetIndex = index
		}
	}
	if targetIndex < 0 {
		return nil
	}

	state.form, state.fieldOrder = newSetupForm(state.values, state.theme)
	commands := []tea.Cmd{state.form.Init()}
	for range targetIndex {
		previous := state.form.GetFocusedField()
		moveCmd := state.form.NextGroup()
		commands = append(commands, previous.Blur(), moveCmd)
	}
	state.syncViewport(true)
	return tea.Batch(commands...)
}

func (state setupState) fieldClickAt(x, y int) (setupFieldClick, bool) {
	formX, formY, ok := state.formCoordinates(x, y)
	if !ok {
		return setupFieldClick{}, false
	}

	formView := ansi.Strip(state.form.View())
	rows := strings.Split(formView, "\n\n")
	rowCount := (len(state.fieldOrder) + setupGridColumns - 1) / setupGridColumns
	rowTop := 0
	rowIndex := -1
	for index := range min(rowCount, len(rows)) {
		rowHeight := lipgloss.Height(rows[index])
		if formY >= rowTop && formY < rowTop+rowHeight {
			rowIndex = index
			break
		}
		rowTop += rowHeight + 1
	}
	if rowIndex < 0 {
		return setupFieldClick{}, false
	}

	formWidth := state.width
	if state.overflow {
		formWidth -= state.viewport.Style.GetHorizontalFrameSize()
	}
	firstFieldIndex := rowIndex * setupGridColumns
	groupWidth := formWidth
	columnIndex := 0
	fieldIndex := firstFieldIndex
	if firstFieldIndex+1 < len(state.fieldOrder) {
		groupWidth = max(1, formWidth/setupGridColumns)
		columnIndex = formX / groupWidth
		fieldIndex += columnIndex
		if columnIndex >= setupGridColumns {
			return setupFieldClick{}, false
		}
	}
	if fieldIndex >= len(state.fieldOrder) {
		return setupFieldClick{}, false
	}

	id := state.fieldOrder[fieldIndex]
	click := setupFieldClick{field: id}
	lineIndex := formY - rowTop
	rowLines := strings.Split(rows[rowIndex], "\n")
	if lineIndex >= len(rowLines) {
		return click, true
	}
	line := rowLines[lineIndex]
	cellStart := columnIndex * groupWidth
	cellEnd := cellStart + groupWidth

	themeStyles := newHuhTheme(state.theme.dark)
	styles := themeStyles.Blurred
	if fieldID(state.form.GetFocusedField().GetKey()) == id {
		styles = themeStyles.Focused
	}
	if id == fieldRun {
		click.submit = strings.Contains(line, runButtonLabel) &&
			formX >= cellStart && formX < cellEnd
		return click, true
	}

	value := state.confirmValue(id)
	if value == nil {
		return click, true
	}
	yesStyle, noStyle := styles.BlurredButton, styles.FocusedButton
	if *value {
		yesStyle, noStyle = styles.FocusedButton, styles.BlurredButton
	}
	if buttonContains(line, "Yes", formX, cellStart, cellEnd, yesStyle) {
		click.confirmValue = true
		click.hasConfirmValue = true
	} else if buttonContains(line, "No", formX, cellStart, cellEnd, noStyle) {
		click.hasConfirmValue = true
	}
	return click, true
}

func (state setupState) formCoordinates(x, y int) (int, int, bool) {
	if x < 0 || y < 0 {
		return 0, 0, false
	}
	formHeight := state.height
	if state.validationError != "" {
		formHeight = max(1, formHeight-1)
	}
	if !state.overflow {
		contentHeight := min(formHeight, lipgloss.Height(state.form.View()))
		return x, y, x < state.width && y < contentHeight
	}

	frameLeft := state.viewport.Style.GetPaddingLeft() + 1
	frameRight := state.viewport.Style.GetPaddingRight() + 1
	if x < frameLeft ||
		x >= state.viewport.Width()-frameRight ||
		y < 1 ||
		y >= state.viewport.Height()-1 {
		return 0, 0, false
	}
	return x - frameLeft, y - 1 + state.viewport.YOffset(), true
}

func (state setupState) confirmValue(id fieldID) *bool {
	switch id {
	case fieldKeepFragments:
		return &state.values.keepFragments
	case fieldIgnoreRedirects:
		return &state.values.ignoreRedirects
	case fieldRespectRobots:
		return &state.values.respectRobots
	case fieldSitemaps:
		return &state.values.sitemaps
	case fieldImages:
		return &state.values.images
	default:
		return nil
	}
}

func buttonContains(
	line, label string,
	x, cellStart, cellEnd int,
	style lipgloss.Style,
) bool {
	remaining := line
	consumed := ""
	for {
		beforeLabel, afterLabel, found := strings.Cut(remaining, label)
		if !found {
			return false
		}
		labelStart := lipgloss.Width(consumed + beforeLabel)
		if labelStart >= cellStart && labelStart < cellEnd {
			buttonStart := labelStart - style.GetPaddingLeft()
			buttonEnd := labelStart + lipgloss.Width(label) + style.GetPaddingRight()
			return x >= buttonStart && x < buttonEnd
		}
		consumed += beforeLabel + label
		remaining = afterLabel
	}
}
