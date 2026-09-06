package tui

import (
	"strconv"

	tea "charm.land/bubbletea/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

func (state *resultsState) update(msg tea.Msg, report *audit.Report) (resultAction, tea.Cmd) {
	key, isKey := msg.(tea.KeyPressMsg)
	if state.focus == focusSearch {
		if isKey {
			switch key.String() {
			case "esc", "escape", "tab", "enter":
				return resultNone, state.setFocus(focusList)
			}
		}

		before := state.query.Value()
		updated, cmd := state.query.Update(msg)
		state.query = updated
		if state.query.Value() != before {
			state.selected = 0
			state.refresh(report)
		}
		return resultNone, cmd
	}

	if isKey {
		keyString := key.String()
		switch keyString {
		case "1", "2", "3", "4", "5":
			index, _ := strconv.Atoi(keyString)
			state.changeTab(resultTabs[index-1], report)
			return resultNone, nil
		case "left", "right":
			index := tabIndex(state.tab)
			if keyString == "left" {
				index = (index - 1 + len(resultTabs)) % len(resultTabs)
			} else {
				index = (index + 1) % len(resultTabs)
			}
			state.changeTab(resultTabs[index], report)
			return resultNone, nil
		case "q":
			return resultQuit, nil
		case "n":
			return resultNewAudit, nil
		case "/":
			if state.tab != tabOverview {
				return resultNone, state.setFocus(focusSearch)
			}
		case "f":
			if state.tab != tabOverview {
				state.cycleFilter()
				state.selected = 0
				state.refresh(report)
			}
			return resultNone, nil
		case "tab":
			if state.tab != tabOverview {
				if state.focus == focusList {
					_ = state.setFocus(focusDetail)
				} else {
					_ = state.setFocus(focusList)
				}
			}
			return resultNone, nil
		case "esc", "escape":
			switch {
			case state.width < 100 && state.detailOpen:
				state.detailOpen = false
				_ = state.setFocus(focusList)
			case state.focus == focusDetail:
				_ = state.setFocus(focusList)
			case state.query.Value() != "":
				state.query.SetValue("")
				state.selected = 0
				state.refresh(report)
			}
			return resultNone, nil
		case "enter":
			if state.width < 100 && state.tab != tabOverview && len(state.items) > 0 {
				state.detailOpen = !state.detailOpen
				if state.detailOpen {
					_ = state.setFocus(focusDetail)
				} else {
					_ = state.setFocus(focusList)
				}
			}
			return resultNone, nil
		}
	}

	if state.tab == tabOverview {
		updated, cmd := state.overview.Update(msg)
		state.overview = updated
		return resultNone, cmd
	}
	if state.focus == focusDetail {
		updated, cmd := state.detail.Update(msg)
		state.detail = updated
		return resultNone, cmd
	}

	before := state.table.Cursor()
	updated, cmd := state.table.Update(msg)
	state.table = updated
	state.selected = max(0, state.table.Cursor())
	if state.table.Cursor() != before {
		state.syncDetail(report)
	}
	return resultNone, cmd
}

func (state *resultsState) changeTab(tab resultTab, report *audit.Report) {
	state.tab = tab
	state.query.SetValue("")
	state.selected = 0
	state.detailOpen = false
	state.detail.GotoTop()
	if tab == tabOverview {
		_ = state.setFocus(focusDetail)
	} else {
		_ = state.setFocus(focusList)
	}
	state.syncTableColumns(max(10, state.table.Width()))
	state.refresh(report)
}

func (state *resultsState) cycleFilter() {
	filters := state.availableFilters()
	current := state.currentFilter()
	index := 0
	for candidateIndex, candidate := range filters {
		if candidate == current {
			index = candidateIndex
			break
		}
	}
	next := filters[(index+1)%len(filters)]
	switch state.tab {
	case tabIssues:
		state.filters.issues = next
	case tabPages:
		state.filters.pages = next
	case tabLinks:
		state.filters.links = next
	case tabImages:
		state.filters.images = next
	}
}

func (state resultsState) availableFilters() []string {
	switch state.tab {
	case tabIssues:
		return []string{"all", "error", "warning", "info"}
	case tabPages:
		return []string{"all", "healthy", "non-2xx", "failed"}
	case tabLinks:
		return []string{"all", "healthy", "broken", "blocked", "redirected", "skipped"}
	case tabImages:
		return []string{"all", "healthy", "broken", "blocked", "invalid", "redirected", "skipped"}
	default:
		return []string{"all"}
	}
}

func (state resultsState) currentFilter() string {
	switch state.tab {
	case tabIssues:
		return state.filters.issues
	case tabPages:
		return state.filters.pages
	case tabLinks:
		return state.filters.links
	case tabImages:
		return state.filters.images
	default:
		return "none"
	}
}

func tabIndex(tab resultTab) int {
	for index, candidate := range resultTabs {
		if candidate == tab {
			return index
		}
	}
	return 0
}
