package tui

import (
	"fmt"
	"image/color"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

type shellMetrics struct {
	headerRows         int
	footerRows         int
	bodyPaddingColumns int
}

var shell = shellMetrics{
	headerRows:         3,
	footerRows:         1,
	bodyPaddingColumns: 2,
}

func (m shellMetrics) contentSize(width, height int) (int, int) {
	return max(1, width-m.bodyPaddingColumns),
		max(1, height-m.headerRows-m.footerRows)
}

const (
	paneBorderRows     = 2
	paneBorderColumns  = 2
	panePaddingColumns = 2
	paneTitleRows      = 1
)

func paneContentWidth(width int) int {
	return max(1, width-paneBorderColumns-panePaddingColumns)
}

func paneContentHeight(height int) int {
	return max(1, height-paneBorderRows-paneTitleRows)
}

var statusLabels = map[screen]string{
	screenSetup:    "Setup",
	screenRunning:  "Running",
	screenResults:  "Results",
	screenCanceled: "Canceled",
	screenFailed:   "Failed",
}

func renderShell(width, height int, state screen, target, footer string, content string, theme theme) string {
	if width < 1 {
		width = 1
	}
	if height < 1 {
		height = 1
	}

	brand := theme.brand.Render("Scoutly")
	status := theme.status(state).Render(statusLabels[state])
	targetWidth := max(1, width-lipgloss.Width(brand)-lipgloss.Width(status)-4)
	target = truncate(target, targetWidth)
	headerLine := lipgloss.JoinHorizontal(
		lipgloss.Top,
		brand,
		theme.headerText.Width(targetWidth).Align(lipgloss.Center).Render(target),
		status,
	)
	header := lipgloss.NewStyle().
		Width(width).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(theme.colors.border).
		Padding(0, 1).
		Render(headerLine)

	contentWidth, contentHeight := shell.contentSize(width, height)
	body := lipgloss.NewStyle().
		Width(width).
		Height(contentHeight).
		Padding(0, 1).
		Render(content)
	footerLine := theme.footer.
		Width(width).
		Padding(0, 1).
		Render(truncate(footer, contentWidth))

	return lipgloss.JoinVertical(lipgloss.Left, header, body, footerLine)
}

func renderPane(
	title, content string,
	width, height int,
	focused bool,
	scrollbar string,
	theme theme,
) string {
	pane := theme.pane(focused)
	body := pane.title.Render(title) + "\n" + content
	style := lipgloss.NewStyle().
		Width(width).
		Height(height).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(pane.border).
		Padding(0, 1)
	if scrollbar != "" {
		body = lipgloss.JoinHorizontal(lipgloss.Top, body, " \n"+scrollbar)
		style = style.PaddingRight(0)
	}
	return style.Render(body)
}

func renderStatus(title, message string, statusColor color.Color, width, height int) string {
	messageLines := strings.Split(ansi.Hardwrap(message, max(1, width), false), "\n")
	maxMessageHeight := max(1, height-2)
	if len(messageLines) > maxMessageHeight {
		messageLines = messageLines[:maxMessageHeight]
		last := len(messageLines) - 1
		messageLines[last] = truncate(messageLines[last]+"…", max(1, width))
	}

	content := lipgloss.JoinVertical(
		lipgloss.Center,
		lipgloss.NewStyle().Foreground(statusColor).Bold(true).Render(title),
		"",
		strings.Join(messageLines, "\n"),
	)
	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, content)
}

func renderResizeWarning(width, height, currentWidth, currentHeight int, theme theme) string {
	content := lipgloss.JoinVertical(
		lipgloss.Center,
		theme.warning.Render("Terminal is too small"),
		"",
		fmt.Sprintf("Minimum size: %d x %d", minimumWidth, minimumHeight),
		fmt.Sprintf("Current size: %d x %d", currentWidth, currentHeight),
	)
	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, content)
}
