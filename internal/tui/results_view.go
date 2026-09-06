package tui

import (
	"fmt"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

func (state resultsState) render(report *audit.Report, theme theme) string {
	tabs := make([]string, 0, len(resultTabs))
	for index, tab := range resultTabs {
		tabs = append(tabs, theme.focused(tab == state.tab).
			Render(fmt.Sprintf("%d %s", index+1, titleCase(string(tab)))))
	}
	tabLine := strings.Join(tabs, "  ")

	if state.tab == tabOverview {
		overview := state.overview
		overview.SetContent(renderOverview(report, overview.Width(), theme))
		scrollbar := ""
		if state.width > 1 {
			scrollbar = renderScrollbar(
				overview.Height(),
				overview.TotalLineCount(),
				overview.VisibleLineCount(),
				overview.YOffset(),
				true,
				theme,
			)
		}
		return tabLine + "\n" + lipgloss.JoinHorizontal(
			lipgloss.Top,
			overview.View(),
			scrollbar,
		)
	}

	query := state.query.Value()
	queryView := query
	if state.focus == focusSearch {
		queryView = state.query.View()
	} else if query == "" {
		queryView = theme.muted.Render("Press / to search")
	}
	searchFocused := state.focus == focusSearch
	searchColor := theme.colors.muted
	if searchFocused {
		searchColor = theme.colors.accent
	}
	searchBorderColor := theme.pane(searchFocused).border
	queryView = lipgloss.NewStyle().Width(state.query.Width()).Render(queryView)
	searchFieldWidth := resultSearchFieldWidth(state.width)
	searchField := theme.metricBox.
		Width(searchFieldWidth).
		BorderForeground(searchBorderColor).
		Render(
			lipgloss.NewStyle().Foreground(searchColor).Render(resultSearchLabel) +
				queryView,
		)
	filterField := theme.metricBox.
		Width(resultFilterFieldWidth).
		Render(
			theme.muted.Render(resultFilterLabel) +
				theme.accent.Render(state.currentFilter()),
		)
	controls := lipgloss.JoinHorizontal(
		lipgloss.Top,
		searchField,
		resultControlGap,
		filterField,
	)

	paneHeight := max(5, state.height-resultListHeaderRows)
	listContent := state.table.View()
	if len(state.items) == 0 {
		listContent = theme.muted.Render("No matching results.")
	}
	tableOffset, _ := state.visibleResultIndex(0, theme)
	tableScrollbar := renderScrollbar(
		state.table.Height()+resultTableHeadRow,
		len(state.table.Rows()),
		min(len(state.table.Rows()), state.table.Height()),
		tableOffset,
		state.focus == focusList,
		theme,
	)
	detailScrollbar := renderScrollbar(
		state.detail.Height(),
		state.detail.TotalLineCount(),
		state.detail.VisibleLineCount(),
		state.detail.YOffset(),
		state.focus == focusDetail,
		theme,
	)
	selectedTitle := "Details"
	if len(state.items) > 0 {
		selectedTitle += ": " + titleCase(string(state.items[state.selected].kind))
	}

	var panes string
	if state.width < 100 {
		if state.detailOpen {
			panes = renderPane(
				selectedTitle,
				state.detail.View(),
				state.width,
				paneHeight,
				true,
				detailScrollbar,
				theme,
			)
		} else {
			panes = renderPane(
				fmt.Sprintf("Results (%d)", len(state.items)),
				listContent,
				state.width,
				paneHeight,
				state.focus == focusList,
				tableScrollbar,
				theme,
			)
		}
	} else {
		leftWidth := (state.width - 1) / 2
		rightWidth := state.width - leftWidth - 1
		left := renderPane(
			fmt.Sprintf("Results (%d)", len(state.items)),
			listContent,
			leftWidth,
			paneHeight,
			state.focus == focusList,
			tableScrollbar,
			theme,
		)
		right := renderPane(
			selectedTitle,
			state.detail.View(),
			rightWidth,
			paneHeight,
			state.focus == focusDetail,
			detailScrollbar,
			theme,
		)
		panes = lipgloss.JoinHorizontal(lipgloss.Top, left, " ", right)
	}

	return strings.Join([]string{tabLine, controls, panes}, "\n")
}

func resultSearchFieldWidth(width int) int {
	return max(1, width-resultFilterFieldWidth-len(resultControlGap))
}
