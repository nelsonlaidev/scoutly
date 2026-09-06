package tui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"charm.land/bubbles/v2/key"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/nelsonlaidev/scoutly/audit"
	"github.com/nelsonlaidev/scoutly/internal/progressfmt"
)

const (
	maxActivityEntries = 200
	runningSummaryRows = 4
	currentPaneRows    = 4
)

type auditProgressMsg struct {
	progress audit.Progress
}

type auditFinishedMsg struct {
	report *audit.Report
	err    error
}

type elapsedTickMsg struct {
	now    time.Time
	events <-chan tea.Msg
}

type runningKeyMap struct {
	Cancel key.Binding
}

func (k runningKeyMap) ShortHelp() []key.Binding {
	return []key.Binding{k.Cancel}
}

func (k runningKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{{k.Cancel}}
}

type runningState struct {
	target            string
	startedAt         time.Time
	now               time.Time
	progress          audit.Progress
	activity          []string
	activityViewport  viewport.Model
	activityFollowing bool
	context           context.Context
	cancel            context.CancelFunc
	events            chan tea.Msg
	canceling         bool
	keymap            runningKeyMap
	width             int
	height            int
}

func newRunningState(
	ctx context.Context,
	cancel context.CancelFunc,
	events chan tea.Msg,
	target string,
	width, height int,
) runningState {
	activity := viewport.New()
	activity.SoftWrap = false
	activity.MouseWheelEnabled = true
	now := time.Now()
	state := runningState{
		target:            target,
		startedAt:         now,
		now:               now,
		progress:          audit.Progress{Phase: audit.PhaseRobots},
		activity:          make([]string, 0),
		activityViewport:  activity,
		activityFollowing: true,
		context:           ctx,
		cancel:            cancel,
		events:            events,
		keymap: runningKeyMap{
			Cancel: key.NewBinding(
				key.WithKeys("ctrl+c"),
				key.WithHelp("Ctrl+C", "cancel audit"),
			),
		},
	}
	state.resize(width, height)
	return state
}

func (state *runningState) resize(width, height int) {
	state.width = max(1, width)
	state.height = max(1, height)
	recentPaneHeight := max(4, height-runningSummaryRows-currentPaneRows)
	state.activityViewport.SetWidth(paneContentWidth(width))
	state.activityViewport.SetHeight(max(1, paneContentHeight(recentPaneHeight)))
	state.syncActivity(0)
}

func (state *runningState) receiveProgress(progress audit.Progress) {
	state.progress = progress
	trimmed := 0
	if progress.CurrentURL != "" {
		entry := string(progress.Phase) + ": " + progress.CurrentURL
		if len(state.activity) == 0 || state.activity[len(state.activity)-1] != entry {
			state.activity = append(state.activity, entry)
			if len(state.activity) > maxActivityEntries {
				trimmed = len(state.activity) - maxActivityEntries
				state.activity = append([]string{}, state.activity[trimmed:]...)
			}
		}
	}
	state.syncActivity(trimmed)
}

func (state *runningState) syncActivity(trimmed int) {
	offset := state.activityViewport.YOffset()
	state.activityViewport.SetContent(strings.Join(state.activity, "\n"))
	if state.activityFollowing {
		state.activityViewport.GotoBottom()
		return
	}
	state.activityViewport.SetYOffset(max(0, offset-trimmed))
	if state.activityViewport.AtBottom() {
		state.activityFollowing = true
	}
}

func (state *runningState) updateMouseWheel(mouse tea.Mouse) tea.Cmd {
	recentTop := runningSummaryRows + currentPaneRows
	recentHeight := max(4, state.height-runningSummaryRows-currentPaneRows)
	if mouse.X < 0 || mouse.X >= state.width ||
		mouse.Y < recentTop || mouse.Y >= recentTop+recentHeight {
		return nil
	}
	if mouse.Button != tea.MouseWheelUp && mouse.Button != tea.MouseWheelDown {
		return nil
	}

	updated, cmd := state.activityViewport.Update(tea.MouseWheelMsg(mouse))
	state.activityViewport = updated
	state.activityFollowing = state.activityViewport.AtBottom()
	return cmd
}

func (state *runningState) cancelAudit() {
	if state.cancel == nil || state.canceling {
		return
	}
	state.canceling = true
	state.cancel()
}

func (state runningState) render(width, height int, theme theme) string {
	phases := []audit.Phase{
		audit.PhaseRobots,
		audit.PhaseCrawl,
		audit.PhaseSitemaps,
		audit.PhaseLinks,
		audit.PhaseImages,
		audit.PhaseReport,
	}
	current := 0
	for index, phase := range phases {
		if phase == state.progress.Phase {
			current = index
			break
		}
	}
	phaseViews := make([]string, 0, len(phases))
	for index, phase := range phases {
		marker := "[ ]"
		markerStyle := theme.muted
		switch {
		case index < current:
			marker, markerStyle = "[x]", theme.success
		case index == current:
			marker, markerStyle = "[>]", theme.accent
		}
		phaseViews = append(
			phaseViews,
			markerStyle.Render(marker+" "+titleCase(string(phase))),
		)
	}
	phaseBar := strings.Join(phaseViews, "   ")
	if lipgloss.Width(phaseBar) > width {
		phaseBar = strings.Join(phaseViews, " ")
	}

	metrics := fmt.Sprintf(
		"%s · %d/%d pages · %d sitemaps · %s links · %s images",
		progressfmt.Duration(state.now.Sub(state.startedAt)),
		state.progress.Pages.Crawled,
		state.progress.Pages.Discovered,
		state.progress.Sitemaps.Fetched,
		progressfmt.Resource(state.progress.Links),
		progressfmt.Resource(state.progress.Images),
	)
	currentURL := fallback(state.progress.CurrentURL, "Preparing audit...")
	activity := state.activityViewport.View()
	if len(state.activity) == 0 {
		activity = theme.muted.Render("Waiting for the first request...")
	}
	recentPaneHeight := max(4, height-runningSummaryRows-currentPaneRows)
	activityScrollbar := renderScrollbar(
		state.activityViewport.Height(),
		state.activityViewport.TotalLineCount(),
		state.activityViewport.VisibleLineCount(),
		state.activityViewport.YOffset(),
		false,
		theme,
	)

	return strings.Join([]string{
		phaseBar,
		"",
		truncate(metrics, width),
		"",
		renderPane("Current activity", truncate(currentURL, max(1, width-6)), width, currentPaneRows, false, "", theme),
		renderPane("Recent activity", activity, width, recentPaneHeight, false, activityScrollbar, theme),
	}, "\n")
}

func startAudit(
	ctx context.Context,
	target string,
	options audit.Options,
	events chan tea.Msg,
) tea.Cmd {
	return func() tea.Msg {
		go func() {
			report, err := audit.Audit(ctx, target, options, func(progress audit.Progress) error {
				if sendAuditEvent(ctx, events, auditProgressMsg{progress: progress}) {
					return nil
				}
				return context.Cause(ctx)
			})
			sendAuditEvent(ctx, events, auditFinishedMsg{report: report, err: err})
		}()
		return waitAuditEvent(ctx, events)()
	}
}

func waitAuditEvent(ctx context.Context, events <-chan tea.Msg) tea.Cmd {
	return func() tea.Msg {
		select {
		case message := <-events:
			return message
		case <-ctx.Done():
			return auditFinishedMsg{err: context.Cause(ctx)}
		}
	}
}

func sendAuditEvent(ctx context.Context, events chan<- tea.Msg, message tea.Msg) bool {
	select {
	case events <- message:
		return true
	case <-ctx.Done():
		return false
	}
}

func elapsedTick(events <-chan tea.Msg) tea.Cmd {
	return tea.Tick(250*time.Millisecond, func(now time.Time) tea.Msg {
		return elapsedTickMsg{now: now, events: events}
	})
}
