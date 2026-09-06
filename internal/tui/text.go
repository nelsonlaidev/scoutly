package tui

import (
	"strings"

	"github.com/charmbracelet/x/ansi"
)

func truncate(value string, maximum int) string {
	if maximum <= 0 {
		return ""
	}
	return ansi.Truncate(value, maximum, "…")
}

func titleCase(value string) string {
	if value == "" {
		return ""
	}
	runes := []rune(value)
	first := strings.ToUpper(string(runes[0]))
	return first + string(runes[1:])
}

func fallback(value, alternate string) string {
	if value == "" {
		return alternate
	}
	return value
}
