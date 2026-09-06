package tui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

func TestRenderScrollbarPositionsProportionalThumb(t *testing.T) {
	theme := newTheme(true)
	tests := []struct {
		name   string
		offset int
		want   []string
	}{
		{
			name:   "top",
			offset: 0,
			want:   []string{"┃", "┃", "│", "│", "│", "│", "│", "│", "│", "│"},
		},
		{
			name:   "middle",
			offset: 40,
			want:   []string{"│", "│", "│", "│", "┃", "┃", "│", "│", "│", "│"},
		},
		{
			name:   "bottom",
			offset: 80,
			want:   []string{"│", "│", "│", "│", "│", "│", "│", "│", "┃", "┃"},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			bar := ansi.Strip(renderScrollbar(10, 100, 20, test.offset, true, theme))
			lines := strings.Split(bar, "\n")
			if strings.Join(lines, "") != strings.Join(test.want, "") {
				t.Fatalf("scrollbar = %q, want %q", lines, test.want)
			}
			for index, line := range lines {
				if got := ansi.StringWidth(line); got != 1 {
					t.Errorf("line %d width = %d, want 1", index, got)
				}
			}
		})
	}
}

func TestRenderScrollbarHidesWhenContentFits(t *testing.T) {
	bar := renderScrollbar(4, 4, 4, 0, false, newTheme(true))
	if got := ansi.Strip(bar); got != " \n \n \n " {
		t.Fatalf("scrollbar = %q, want blank column", got)
	}
}

func TestRenderScrollbarOnRightBorderPreservesFrame(t *testing.T) {
	view := "╭──╮\n│  │\n│  │\n│  │\n╰──╯"
	bar := renderScrollbar(3, 6, 3, 3, true, newTheme(true))
	got := ansi.Strip(renderScrollbarOnRightBorder(view, bar))
	want := "╭──╮\n│  │\n│  │\n│  ┃\n╰──╯"
	if got != want {
		t.Fatalf("bordered scrollbar:\n%s\nwant:\n%s", got, want)
	}
}

func TestRenderPaneUsesRightPaddingForScrollbar(t *testing.T) {
	const (
		width  = 30
		height = 8
	)
	content := lipgloss.NewStyle().
		Width(paneContentWidth(width)).
		Height(paneContentHeight(height)).
		Render("content")
	bar := renderScrollbar(
		paneContentHeight(height),
		100,
		paneContentHeight(height),
		0,
		false,
		newTheme(true),
	)
	view := renderPane("Title", content, width, height, false, bar, newTheme(true))
	if got := lipgloss.Width(view); got != width {
		t.Fatalf("pane width = %d, want %d:\n%s", got, width, view)
	}
	if got := lipgloss.Height(view); got != height {
		t.Fatalf("pane height = %d, want %d:\n%s", got, height, view)
	}
}
