package tui

import (
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

func TestTruncateUsesTerminalCellWidth(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		value   string
		maximum int
		want    string
	}{
		{name: "zero width", value: "Scoutly", maximum: 0, want: ""},
		{name: "unchanged", value: "Scoutly", maximum: 7, want: "Scoutly"},
		{name: "ASCII", value: "Scoutly", maximum: 5, want: "Scou…"},
		{name: "wide characters", value: "網站 audit", maximum: 4, want: "網…"},
		{name: "grapheme", value: "👩‍💻 audit", maximum: 4, want: "👩‍💻 …"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := truncate(test.value, test.maximum); got != test.want {
				t.Fatalf("truncate(%q, %d) = %q, want %q", test.value, test.maximum, got, test.want)
			}
		})
	}
}

func TestTruncatePreservesANSISequences(t *testing.T) {
	t.Parallel()

	styled := lipgloss.NewStyle().Bold(true).Render("Scoutly")
	got := truncate(styled, 5)
	if width := ansi.StringWidth(got); width != 5 {
		t.Fatalf("truncate() width = %d, want 5", width)
	}
	if plain := ansi.Strip(got); plain != "Scou…" {
		t.Fatalf("truncate() text = %q, want %q", plain, "Scou…")
	}
}
