package tui

import (
	"fmt"
	"strings"

	"charm.land/bubbles/v2/table"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/nelsonlaidev/scoutly/audit"
)

const (
	resultTabRow       = 0
	resultContentTop   = 1
	resultControlRows  = 3
	resultTableHeadRow = 1
)

type resultMousePane uint8

const (
	resultMousePaneNone resultMousePane = iota
	resultMousePaneList
	resultMousePaneDetail
)

func (state *resultsState) updateMouseClick(
	mouse tea.Mouse,
	report *audit.Report,
	theme theme,
) tea.Cmd {
	if mouse.Button != tea.MouseLeft || !state.containsMouse(mouse) {
		return nil
	}

	if tab, ok := resultTabAt(mouse.X, mouse.Y); ok {
		if tab != state.tab {
			state.changeTab(tab, report)
		}
		return nil
	}

	if state.tab == tabOverview {
		contentY := mouse.Y + state.overview.YOffset()
		if tab, ok := overviewMetricTabAt(mouse.X, contentY, state.overview.Width()); ok {
			state.changeTab(tab, report)
		}
		return nil
	}

	if mouse.Y >= resultContentTop &&
		mouse.Y < resultContentTop+resultControlRows {
		searchWidth := resultSearchFieldWidth(state.width)
		switch {
		case mouse.X < searchWidth:
			return state.setFocus(focusSearch)
		case mouse.X >= searchWidth+len(resultControlGap):
			_ = state.setFocus(focusList)
			state.cycleFilter()
			state.selected = 0
			state.refresh(report)
		}
		return nil
	}

	switch state.mousePaneAt(mouse.X, mouse.Y) {
	case resultMousePaneList:
		_ = state.setFocus(focusList)
		row := mouse.Y - state.listDataTop()
		index, ok := state.visibleResultIndex(row, theme)
		if !ok {
			return nil
		}
		state.selected = index
		state.table.SetCursor(index)
		state.syncDetail(report)
		if state.width < 100 {
			state.detailOpen = true
			_ = state.setFocus(focusDetail)
		}

	case resultMousePaneDetail:
		_ = state.setFocus(focusDetail)
	}
	return nil
}

func (state *resultsState) updateMouseWheel(
	mouse tea.Mouse,
	report *audit.Report,
) tea.Cmd {
	if !state.containsMouse(mouse) {
		return nil
	}

	if state.tab == tabOverview {
		if mouse.Y < resultContentTop {
			return nil
		}
		updated, cmd := state.overview.Update(tea.MouseWheelMsg(mouse))
		state.overview = updated
		return cmd
	}

	switch state.mousePaneAt(mouse.X, mouse.Y) {
	case resultMousePaneList:
		if len(state.items) == 0 {
			return nil
		}
		_ = state.setFocus(focusList)
		before := state.table.Cursor()
		delta := state.detail.MouseWheelDelta
		switch mouse.Button {
		case tea.MouseWheelUp:
			state.table.MoveUp(delta)
		case tea.MouseWheelDown:
			state.table.MoveDown(delta)
		default:
			return nil
		}
		state.selected = max(0, state.table.Cursor())
		if state.table.Cursor() != before {
			state.syncDetail(report)
		}

	case resultMousePaneDetail:
		_ = state.setFocus(focusDetail)
		updated, cmd := state.detail.Update(tea.MouseWheelMsg(mouse))
		state.detail = updated
		return cmd
	}
	return nil
}

func (state resultsState) containsMouse(mouse tea.Mouse) bool {
	return mouse.X >= 0 && mouse.X < state.width && mouse.Y >= 0 && mouse.Y < state.height
}

func (state resultsState) mousePaneAt(x, y int) resultMousePane {
	paneHeight := max(5, state.height-resultListHeaderRows)
	if x < 0 || x >= state.width ||
		y < resultListHeaderRows || y >= resultListHeaderRows+paneHeight {
		return resultMousePaneNone
	}
	if state.width < 100 {
		if state.detailOpen {
			return resultMousePaneDetail
		}
		return resultMousePaneList
	}

	leftWidth := (state.width - 1) / 2
	switch {
	case x < leftWidth:
		return resultMousePaneList
	case x > leftWidth:
		return resultMousePaneDetail
	default:
		return resultMousePaneNone
	}
}

func (state resultsState) listDataTop() int {
	return resultListHeaderRows + 1 + paneTitleRows + resultTableHeadRow
}

func resultTabAt(x, y int) (resultTab, bool) {
	if x < 0 || y != resultTabRow {
		return "", false
	}
	left := 0
	for index, tab := range resultTabs {
		label := fmt.Sprintf("%d %s", index+1, titleCase(string(tab)))
		right := left + lipgloss.Width(label)
		if x >= left && x < right {
			return tab, true
		}
		left = right + 2
	}
	return "", false
}

func overviewMetricTabAt(x, y, width int) (resultTab, bool) {
	if x < 0 || x >= width || y < resultContentTop || y >= resultContentTop+3 {
		return "", false
	}
	left := 0
	for index, metricWidth := range overviewMetricWidths(width) {
		right := left + metricWidth
		if x >= left && x < right {
			return overviewMetrics[index].tab, true
		}
		left = right + 1
	}
	return "", false
}

func (state resultsState) visibleResultIndex(row int, theme theme) (int, bool) {
	if row < 0 || row >= state.table.Height() || len(state.items) == 0 {
		return 0, false
	}

	lines := strings.Split(state.table.View(), "\n")
	if len(lines) <= resultTableHeadRow || row >= len(lines)-resultTableHeadRow {
		return 0, false
	}
	visibleLines := lines[resultTableHeadRow:]
	if strings.TrimSpace(ansi.Strip(visibleLines[row])) == "" {
		return 0, false
	}

	if selectedRow := selectedVisibleResultRow(visibleLines, theme); selectedRow >= 0 {
		index := state.table.Cursor() - selectedRow + row
		if index >= 0 && index < len(state.items) {
			return index, true
		}
	}

	visible := normalizedVisibleResultRows(visibleLines)
	rows := state.table.Rows()
	columns := state.table.Columns()
	for start := 0; start+len(visible) <= len(rows); start++ {
		if state.table.Cursor() < start || state.table.Cursor() >= start+len(visible) {
			continue
		}
		matches := true
		for offset, line := range visible {
			if line != normalizedResultRow(rows[start+offset], columns, theme) {
				matches = false
				break
			}
		}
		if matches && row < len(visible) {
			return start + row, true
		}
	}
	return 0, false
}

func selectedVisibleResultRow(lines []string, theme theme) int {
	probe := theme.table.Selected.Render("x")
	prefix, _, found := strings.Cut(probe, "x")
	if !found || prefix == "" {
		return -1
	}
	for index, line := range lines {
		if strings.Contains(line, prefix) {
			return index
		}
	}
	return -1
}

func normalizedVisibleResultRows(lines []string) []string {
	result := make([]string, 0, len(lines))
	for _, line := range lines {
		normalized := strings.TrimRight(ansi.Strip(line), " ")
		if normalized == "" {
			break
		}
		result = append(result, normalized)
	}
	return result
}

func normalizedResultRow(row table.Row, columns []table.Column, theme theme) string {
	cells := make([]string, 0, len(columns))
	for index, value := range row {
		if index >= len(columns) || columns[index].Width <= 0 {
			continue
		}
		column := columns[index]
		cell := lipgloss.NewStyle().
			Width(column.Width).
			MaxWidth(column.Width).
			Inline(true).
			Render(ansi.Truncate(value, column.Width, "…"))
		cells = append(cells, theme.table.Cell.Render(cell))
	}
	return strings.TrimRight(ansi.Strip(lipgloss.JoinHorizontal(lipgloss.Top, cells...)), " ")
}
