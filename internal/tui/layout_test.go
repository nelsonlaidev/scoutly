package tui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

func TestRenderShellUsesSymmetricHorizontalPadding(t *testing.T) {
	const width = 20
	view := ansi.Strip(renderShell(
		width,
		8,
		screenResults,
		"https://example.com",
		"footer",
		"body",
		newTheme(true),
	))
	lines := strings.Split(view, "\n")

	for _, row := range []int{shell.headerRows, len(lines) - 1} {
		line := lines[row]
		if got := lipgloss.Width(line); got != width {
			t.Fatalf("row %d width = %d, want %d: %q", row, got, width, line)
		}
		if !strings.HasPrefix(line, " ") || !strings.HasSuffix(line, " ") {
			t.Fatalf("row %d does not have symmetric horizontal padding: %q", row, line)
		}
	}
}
