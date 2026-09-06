package tui

import (
	"strings"

	"charm.land/bubbles/v2/key"
	"charm.land/bubbles/v2/table"
	"charm.land/bubbles/v2/textinput"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

type resultTab string

const (
	tabOverview            resultTab = "overview"
	tabIssues              resultTab = "issues"
	tabPages               resultTab = "pages"
	tabLinks               resultTab = "links"
	tabImages              resultTab = "images"
	resultListHeaderRows             = 4
	resultFilterFieldWidth           = 22
	resultControlGap                 = " "
	resultSearchLabel                = "Search: "
	resultFilterLabel                = "Filter: "
)

var resultTabs = []resultTab{tabOverview, tabIssues, tabPages, tabLinks, tabImages}

type resultFocus uint8

const (
	focusList resultFocus = iota
	focusDetail
	focusSearch
)

type resultAction uint8

const (
	resultNone resultAction = iota
	resultQuit
	resultNewAudit
)

type resultKind string

const (
	resultIssue resultKind = "issue"
	resultPage  resultKind = "page"
	resultLink  resultKind = "link"
	resultImage resultKind = "image"
)

type resultItem struct {
	kind        resultKind
	label       string
	description string
	issue       audit.Issue
	page        audit.Page
	link        audit.Link
	image       audit.Image
}

type resultFilters struct {
	issues string
	pages  string
	links  string
	images string
}

type resultsKeyMap struct {
	Tabs   key.Binding
	Search key.Binding
	Filter key.Binding
	Pane   key.Binding
	Move   key.Binding
	New    key.Binding
	Quit   key.Binding
}

func (k resultsKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{k.Tabs, k.Search, k.Filter, k.Pane, k.Move, k.New, k.Quit}
}

func (k resultsKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{
		{k.Tabs},
		{k.Search},
		{k.Filter},
		{k.Pane},
		{k.Move},
		{k.New},
		{k.Quit},
	}
}

type resultsState struct {
	tab        resultTab
	focus      resultFocus
	query      textinput.Model
	filters    resultFilters
	selected   int
	detailOpen bool
	items      []resultItem
	table      table.Model
	detail     viewport.Model
	overview   viewport.Model
	width      int
	height     int
	keymap     resultsKeyMap
}

func newResultsState(report *audit.Report, width, height int, theme theme) resultsState {
	query := textinput.New()
	query.Prompt = ""
	query.Placeholder = "Type to search"

	detail := viewport.New()
	detail.SoftWrap = true
	detail.MouseWheelEnabled = true
	overview := viewport.New()
	overview.SoftWrap = true
	overview.MouseWheelEnabled = true

	state := resultsState{
		tab:      tabOverview,
		focus:    focusDetail,
		query:    query,
		filters:  resultFilters{issues: "all", pages: "all", links: "all", images: "all"},
		table:    table.New(),
		detail:   detail,
		overview: overview,
		keymap: resultsKeyMap{
			Tabs: key.NewBinding(
				key.WithKeys("1", "2", "3", "4", "5"),
				key.WithHelp("1-5", "tabs"),
			),
			Search: key.NewBinding(
				key.WithKeys("/"),
				key.WithHelp("/", "search"),
			),
			Filter: key.NewBinding(
				key.WithKeys("f"),
				key.WithHelp("f", "filter"),
			),
			Pane: key.NewBinding(
				key.WithKeys("tab"),
				key.WithHelp("Tab", "pane"),
			),
			Move: key.NewBinding(
				key.WithKeys("up", "down", "left", "right"),
				key.WithHelp("Arrows", "move"),
			),
			New: key.NewBinding(
				key.WithKeys("n"),
				key.WithHelp("n", "new"),
			),
			Quit: key.NewBinding(
				key.WithKeys("q"),
				key.WithHelp("q", "quit"),
			),
		},
	}
	state.resize(report, width, height, theme)
	return state
}

func (state *resultsState) resize(report *audit.Report, width, height int, theme theme) {
	state.width = max(1, width)
	state.height = max(1, height)
	state.query.SetWidth(max(
		10,
		resultSearchFieldWidth(state.width)-
			paneBorderColumns-
			panePaddingColumns-
			len(resultSearchLabel),
	))
	overviewWidth := state.width
	if overviewWidth > 1 {
		overviewWidth--
	}
	state.overview.SetWidth(overviewWidth)
	state.overview.SetHeight(max(1, state.height-1))
	state.overview.SetContent(renderOverview(report, overviewWidth, theme))

	paneHeight := max(5, state.height-resultListHeaderRows)
	paneWidth := state.width
	if state.width >= 100 {
		paneWidth = max(20, (state.width-1)/2)
	}
	state.table.SetWidth(max(10, paneContentWidth(paneWidth)))
	state.table.SetHeight(max(3, paneContentHeight(paneHeight)))
	state.detail.SetWidth(max(10, paneContentWidth(paneWidth)))
	state.detail.SetHeight(max(1, paneContentHeight(paneHeight)))

	state.syncTableColumns(max(10, paneContentWidth(paneWidth)))
	state.table.SetStyles(theme.table)
	state.refresh(report)
}

func (state *resultsState) syncTableColumns(width int) {
	labelWidth := max(8, width*2/3)
	descriptionWidth := max(6, width-labelWidth-2)
	state.table.SetColumns([]table.Column{
		{Title: titleCase(string(state.tab)), Width: labelWidth},
		{Title: "Status", Width: descriptionWidth},
	})
}

func (state *resultsState) refresh(report *audit.Report) {
	state.items = selectResultItems(
		report,
		state.tab,
		state.query.Value(),
		state.currentFilter(),
	)
	rows := make([]table.Row, 0, len(state.items))
	for _, item := range state.items {
		rows = append(rows, table.Row{item.label, item.description})
	}
	state.table.SetRows(rows)
	if len(rows) == 0 {
		state.selected = 0
	} else {
		state.selected = min(max(0, state.selected), len(rows)-1)
		state.table.SetCursor(state.selected)
	}
	state.syncDetail(report)
}

func (state *resultsState) syncDetail(report *audit.Report) {
	if len(state.items) == 0 {
		state.detail.SetContent("No item selected.")
		state.detail.GotoTop()
		return
	}
	state.selected = min(max(0, state.selected), len(state.items)-1)
	state.detail.SetContent(strings.Join(createDetailLines(state.items[state.selected], report), "\n"))
	state.detail.GotoTop()
}

func (state *resultsState) setFocus(focus resultFocus) tea.Cmd {
	state.query.Blur()
	state.table.Blur()
	state.focus = focus

	switch focus {
	case focusList:
		state.table.Focus()
		return nil
	case focusSearch:
		return state.query.Focus()
	default:
		return nil
	}
}
