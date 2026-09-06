package tui

import (
	"context"
	"errors"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

const (
	defaultWidth  = 80
	defaultHeight = 24
	minimumWidth  = 70
	minimumHeight = 20
)

type screen uint8

const (
	screenSetup screen = iota
	screenRunning
	screenResults
	screenCanceled
	screenFailed
)

type statusKeyMap struct {
	Retry key.Binding
	Quit  key.Binding
}

func (k statusKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{k.Retry, k.Quit}
}

func (k statusKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{k.Retry}, {k.Quit}}
}

type resizeKeyMap struct {
	Quit key.Binding
}

func (k resizeKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{k.Quit}
}

func (k resizeKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{k.Quit}}
}

type model struct {
	context        context.Context
	width          int
	height         int
	darkBackground bool
	screen         screen
	setup          setupState
	running        runningState
	results        resultsState
	report         *audit.Report
	failureMessage string
	footer         help.Model
	statusKeymap   statusKeyMap
	resizeKeymap   resizeKeyMap
}

func newModel(ctx context.Context, options audit.Options) model {
	setup := newSetupState(options)
	state := model{
		context:        ctx,
		width:          defaultWidth,
		height:         defaultHeight,
		darkBackground: true,
		screen:         screenSetup,
		setup:          setup,
		footer:         help.New(),
		statusKeymap: statusKeyMap{
			Retry: key.NewBinding(
				key.WithKeys("enter"),
				key.WithHelp("Enter", "edit and retry"),
			),
			Quit: key.NewBinding(
				key.WithKeys("q"),
				key.WithHelp("q", "quit"),
			),
		},
		resizeKeymap: resizeKeyMap{
			Quit: key.NewBinding(
				key.WithKeys("ctrl+c"),
				key.WithHelp("Ctrl+C", "quit"),
			),
		},
	}
	return state
}

func (m model) Init() tea.Cmd {
	return tea.Batch(tea.RequestBackgroundColor, m.setup.form.Init())
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	theme := newTheme(m.darkBackground)

	switch message := msg.(type) {
	case tea.WindowSizeMsg:
		m.resize(message.Width, message.Height, theme)

	case tea.BackgroundColorMsg:
		m.darkBackground = message.IsDark()
		theme = newTheme(m.darkBackground)
		m.resize(m.width, m.height, theme)

	case auditProgressMsg:
		if m.screen != screenRunning {
			return m, nil
		}
		m.running.receiveProgress(message.progress)
		return m, waitAuditEvent(m.running.context, m.running.events)

	case auditFinishedMsg:
		if m.screen != screenRunning {
			return m, nil
		}
		m.finishAudit(message)
		return m, nil

	case elapsedTickMsg:
		if m.screen != screenRunning || message.events != m.running.events {
			return m, nil
		}
		m.running.now = message.now
		return m, elapsedTick(m.running.events)
	}

	key, isKey := msg.(tea.KeyPressMsg)
	_, isMouse := msg.(tea.MouseMsg)
	if isKey && key.String() == "ctrl+c" {
		if m.screen == screenRunning {
			m.running.cancelAudit()
			return m, nil
		}
		return m, tea.Quit
	}
	if (isKey || isMouse) &&
		(m.width < minimumWidth || m.height < minimumHeight) {
		return m, nil
	}

	switch m.screen {
	case screenSetup:
		var action setupAction
		var cmd tea.Cmd
		if mouseClick, ok := msg.(tea.MouseClickMsg); ok && mouseClick.Button == tea.MouseLeft {
			mouse := contentMouse(mouseClick)
			action, cmd = m.setup.updateClick(
				mouse.X,
				mouse.Y,
			)
		} else if _, ok := msg.(tea.MouseClickMsg); ok {
			return m, nil
		} else {
			action, cmd = m.setup.update(msg)
		}
		switch action {
		case setupStart:
			target, options, errorsByField := parseSetupValues(*m.setup.values)
			if len(errorsByField) > 0 {
				return m, m.setup.resetWithValidationErrors(
					m.contentWidth(),
					m.contentHeight(),
					errorsByField,
				)
			}
			m.startAudit(target, options)
			return m, tea.Batch(
				elapsedTick(m.running.events),
				startAudit(m.running.context, target, options, m.running.events),
			)
		case setupQuit:
			return m, tea.Quit
		default:
			return m, cmd
		}

	case screenRunning:
		if mouseWheel, ok := msg.(tea.MouseWheelMsg); ok {
			return m, m.running.updateMouseWheel(contentMouse(mouseWheel))
		}
		return m, nil

	case screenResults:
		var action resultAction
		var cmd tea.Cmd
		switch message := msg.(type) {
		case tea.MouseClickMsg:
			cmd = m.results.updateMouseClick(contentMouse(message), m.report, theme)
		case tea.MouseWheelMsg:
			cmd = m.results.updateMouseWheel(contentMouse(message), m.report)
		default:
			action, cmd = m.results.update(msg, m.report)
		}
		switch action {
		case resultQuit:
			return m, tea.Quit
		case resultNewAudit:
			return m, m.newAudit()
		default:
			return m, cmd
		}

	case screenCanceled, screenFailed:
		if !isKey {
			return m, nil
		}
		switch key.String() {
		case "enter":
			return m, m.newAudit()
		case "q":
			return m, tea.Quit
		}
	}

	return m, nil
}

func (m model) View() tea.View {
	theme := newTheme(m.darkBackground)
	contentWidth := m.contentWidth()
	contentHeight := m.contentHeight()
	target := m.setup.values.url
	content := ""

	switch m.screen {
	case screenSetup:
		content = m.setup.view(theme)

	case screenRunning:
		target = m.running.target
		content = m.running.render(contentWidth, contentHeight, theme)

	case screenResults:
		if m.report != nil {
			target = m.report.URL
		}
		content = m.results.render(m.report, theme)

	case screenCanceled:
		content = renderStatus(
			"Audit canceled",
			"No partial report was saved.",
			theme.colors.warning,
			contentWidth,
			contentHeight,
		)

	case screenFailed:
		content = renderStatus(
			"Audit failed",
			m.failureMessage,
			theme.colors.error,
			contentWidth,
			contentHeight,
		)
	}

	if m.width < minimumWidth || m.height < minimumHeight {
		content = renderResizeWarning(contentWidth, contentHeight, m.width, m.height, theme)
	}

	view := tea.NewView(renderShell(
		m.width,
		m.height,
		m.screen,
		fallback(target, "Website audit"),
		m.footer.FullHelpView(m.activeKeyMap().FullHelp()),
		content,
		theme,
	))
	view.AltScreen = true
	view.WindowTitle = "Scoutly"
	if m.width >= minimumWidth && m.height >= minimumHeight &&
		(m.screen == screenSetup || m.screen == screenRunning || m.screen == screenResults) {
		view.MouseMode = tea.MouseModeCellMotion
	}
	return view
}

func (m *model) resize(width, height int, theme theme) {
	m.width = max(1, width)
	m.height = max(1, height)
	m.setup.resize(m.contentWidth(), m.contentHeight())
	if m.screen == screenRunning {
		m.running.resize(m.contentWidth(), m.contentHeight())
	}
	if m.screen == screenResults {
		m.results.resize(m.report, m.contentWidth(), m.contentHeight(), theme)
	}
}

func (m model) activeKeyMap() help.KeyMap {
	if m.width < minimumWidth || m.height < minimumHeight {
		return m.resizeKeymap
	}

	switch m.screen {
	case screenSetup:
		return m.setup.keymap
	case screenRunning:
		return m.running.keymap
	case screenResults:
		return m.results.keymap
	case screenCanceled, screenFailed:
		return m.statusKeymap
	default:
		return m.resizeKeymap
	}
}

func (m model) contentWidth() int {
	width, _ := shell.contentSize(m.width, m.height)
	return width
}

func (m model) contentHeight() int {
	_, height := shell.contentSize(m.width, m.height)
	return height
}

func contentMouse(message tea.MouseMsg) tea.Mouse {
	mouse := message.Mouse()
	mouse.X -= shell.bodyPaddingColumns / 2
	mouse.Y -= shell.headerRows
	return mouse
}

func (m *model) startAudit(target string, options audit.Options) {
	auditContext, cancel := context.WithCancel(m.context)
	events := make(chan tea.Msg)
	m.screen = screenRunning
	m.report = nil
	m.failureMessage = ""
	m.running = newRunningState(
		auditContext,
		cancel,
		events,
		target,
		m.contentWidth(),
		m.contentHeight(),
	)
}

func (m *model) finishAudit(message auditFinishedMsg) {
	if m.running.cancel != nil {
		m.running.cancel()
	}
	m.running.cancel = nil
	m.running.events = nil

	switch {
	case message.err == nil && message.report != nil:
		m.screen = screenResults
		m.report = message.report
		m.results = newResultsState(
			message.report,
			m.contentWidth(),
			m.contentHeight(),
			newTheme(m.darkBackground),
		)
	case errors.Is(message.err, context.Canceled):
		m.screen = screenCanceled
		m.report = nil
	default:
		m.screen = screenFailed
		m.report = nil
		if message.err == nil {
			m.failureMessage = "The audit completed without a report."
		} else {
			m.failureMessage = message.err.Error()
		}
	}
}

func (m *model) newAudit() tea.Cmd {
	m.screen = screenSetup
	m.report = nil
	m.failureMessage = ""
	m.running = runningState{}
	m.results = resultsState{}
	return m.setup.reset(m.contentWidth(), m.contentHeight())
}
