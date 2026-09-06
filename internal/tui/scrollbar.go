package tui

import (
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

const (
	scrollbarTrack = "│"
	scrollbarThumb = "┃"
)

func renderScrollbar(
	height, total, visible, offset int,
	focused bool,
	theme theme,
) string {
	if height <= 0 {
		return ""
	}

	lines := make([]string, height)
	if total <= 0 || visible <= 0 || total <= visible {
		for index := range lines {
			lines[index] = " "
		}
		return strings.Join(lines, "\n")
	}

	visible = min(visible, total)
	maxOffset := total - visible
	offset = min(max(0, offset), maxOffset)
	thumbHeight := min(height, max(1, height*visible/total))
	thumbTop := 0
	if maxOffset > 0 {
		travel := height - thumbHeight
		thumbTop = (offset*travel + maxOffset/2) / maxOffset
	}

	thumbColor := theme.colors.muted
	if focused {
		thumbColor = theme.colors.accent
	}
	trackStyle := lipgloss.NewStyle().Foreground(theme.colors.border)
	thumbStyle := lipgloss.NewStyle().Foreground(thumbColor)
	for index := range lines {
		if index >= thumbTop && index < thumbTop+thumbHeight {
			lines[index] = thumbStyle.Render(scrollbarThumb)
		} else {
			lines[index] = trackStyle.Render(scrollbarTrack)
		}
	}
	return strings.Join(lines, "\n")
}

func renderScrollbarOnRightBorder(view, scrollbar string) string {
	if view == "" || scrollbar == "" {
		return view
	}

	viewLines := strings.Split(view, "\n")
	scrollbarLines := strings.Split(scrollbar, "\n")
	for index, bar := range scrollbarLines {
		viewIndex := index + 1
		if viewIndex >= len(viewLines)-1 {
			break
		}
		width := ansi.StringWidth(viewLines[viewIndex])
		if width < 1 {
			continue
		}
		viewLines[viewIndex] = ansi.Cut(viewLines[viewIndex], 0, width-1) + bar
	}
	return strings.Join(viewLines, "\n")
}
