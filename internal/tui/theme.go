package tui

import (
	"image/color"

	"charm.land/bubbles/v2/table"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
)

type palette struct {
	accent             color.Color
	muted              color.Color
	border             color.Color
	selectedForeground color.Color
	selectedBackground color.Color
	error              color.Color
	warning            color.Color
	info               color.Color
	success            color.Color
}

func newPalette(dark bool) palette {
	choose := lipgloss.LightDark(dark)
	foreground := choose(lipgloss.Color("#000000"), lipgloss.Color("#FFFFFF"))
	background := choose(lipgloss.Color("#FFFFFF"), lipgloss.Color("#000000"))
	return palette{
		accent:             foreground,
		muted:              choose(lipgloss.Color("#666666"), lipgloss.Color("#A1A1A1")),
		border:             choose(lipgloss.Color("#EAEAEA"), lipgloss.Color("#333333")),
		selectedForeground: background,
		selectedBackground: foreground,
		error:              foreground,
		warning:            foreground,
		info:               foreground,
		success:            foreground,
	}
}

func newHuhTheme(dark bool) *huh.Styles {
	colors := newPalette(dark)
	styles := huh.ThemeBase(dark)
	styles.FieldSeparator = lipgloss.NewStyle().SetString("\n")

	styles.Focused.Base = styles.Focused.Base.BorderForeground(colors.accent)
	styles.Focused.Card = styles.Focused.Base
	styles.Focused.Title = styles.Focused.Title.
		Foreground(colors.accent).
		Bold(true).
		PaddingRight(1)
	styles.Focused.Description = styles.Focused.Description.Foreground(colors.muted)
	styles.Focused.ErrorIndicator = styles.Focused.ErrorIndicator.Foreground(colors.error)
	styles.Focused.ErrorMessage = styles.Focused.ErrorMessage.Foreground(colors.error)
	styles.Focused.TextInput.Cursor = styles.Focused.TextInput.Cursor.Foreground(colors.accent)
	styles.Focused.TextInput.Placeholder = styles.Focused.TextInput.Placeholder.Foreground(colors.muted)
	styles.Focused.TextInput.Prompt = styles.Focused.TextInput.Prompt.Foreground(colors.accent)
	styles.Focused.FocusedButton = styles.Focused.FocusedButton.
		Foreground(colors.selectedForeground).
		Background(colors.selectedBackground)
	styles.Focused.BlurredButton = styles.Focused.BlurredButton.
		Foreground(colors.selectedBackground).
		Background(colors.selectedForeground)

	styles.Blurred = styles.Focused
	styles.Blurred.Base = styles.Focused.Base.BorderStyle(lipgloss.HiddenBorder())
	styles.Blurred.Card = styles.Blurred.Base
	styles.Blurred.Title = styles.Blurred.Title.Foreground(colors.muted).Bold(false)
	styles.Blurred.NextIndicator = lipgloss.NewStyle()
	styles.Blurred.PrevIndicator = lipgloss.NewStyle()

	styles.Group.Title = styles.Focused.Title
	styles.Group.Description = styles.Focused.Description
	return styles
}

type setupTheme struct {
	dark bool
}

func (theme *setupTheme) Theme(_ bool) *huh.Styles {
	// Huh tracks the terminal background per field, but only the active group
	// receives the background response. Keep every setup field on one palette.
	return newHuhTheme(theme.dark)
}

type setupRunTheme struct {
	setup *setupTheme
	width int
}

func (theme *setupRunTheme) Theme(dark bool) *huh.Styles {
	styles := theme.setup.Theme(dark)
	styles.Focused.Base = lipgloss.NewStyle()
	styles.Focused.Card = styles.Focused.Base
	styles.Focused.FocusedButton = styles.Focused.FocusedButton.
		MarginRight(0).
		Width(max(1, theme.width)).
		Align(lipgloss.Center)
	styles.Blurred.Base = lipgloss.NewStyle()
	styles.Blurred.Card = styles.Blurred.Base
	styles.Blurred.FocusedButton = styles.Blurred.FocusedButton.
		MarginRight(0).
		Width(max(1, theme.width)).
		Align(lipgloss.Center)
	return styles
}

type paneTheme struct {
	border color.Color
	title  lipgloss.Style
}

type theme struct {
	colors palette

	brand       lipgloss.Style
	headerText  lipgloss.Style
	footer      lipgloss.Style
	title       lipgloss.Style
	muted       lipgloss.Style
	accent      lipgloss.Style
	errorText   lipgloss.Style
	success     lipgloss.Style
	warning     lipgloss.Style
	metricBox   lipgloss.Style
	metricLabel lipgloss.Style
	metricValue lipgloss.Style
	table       table.Styles
}

func newTheme(dark bool) theme {
	colors := newPalette(dark)

	return theme{
		colors: colors,
		brand:  lipgloss.NewStyle().Foreground(colors.accent).Bold(true),
		headerText: lipgloss.NewStyle().
			Foreground(colors.muted),
		footer:    lipgloss.NewStyle().Foreground(colors.muted),
		title:     lipgloss.NewStyle().Foreground(colors.accent).Bold(true),
		muted:     lipgloss.NewStyle().Foreground(colors.muted),
		accent:    lipgloss.NewStyle().Foreground(colors.accent),
		errorText: lipgloss.NewStyle().Foreground(colors.error),
		success:   lipgloss.NewStyle().Foreground(colors.success),
		warning:   lipgloss.NewStyle().Foreground(colors.warning).Bold(true),
		metricBox: lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(colors.border).
			Padding(0, 1),
		metricLabel: lipgloss.NewStyle().Foreground(colors.muted),
		metricValue: lipgloss.NewStyle().Foreground(colors.accent).Bold(true),
		table: table.Styles{
			Header: lipgloss.NewStyle().
				Foreground(colors.muted).
				PaddingRight(1),
			Cell: lipgloss.NewStyle().PaddingRight(1),
			Selected: lipgloss.NewStyle().
				Bold(true).
				Foreground(colors.selectedForeground).
				Background(colors.selectedBackground).
				PaddingRight(1),
		},
	}
}

func (theme theme) focused(focused bool) lipgloss.Style {
	if focused {
		return lipgloss.NewStyle().Foreground(theme.colors.accent).Bold(true)
	}
	return lipgloss.NewStyle().Foreground(theme.colors.muted)
}

func (theme theme) pane(focused bool) paneTheme {
	border := theme.colors.border
	titleColor := theme.colors.muted
	if focused {
		border = theme.colors.accent
		titleColor = theme.colors.accent
	}
	return paneTheme{
		border: border,
		title:  lipgloss.NewStyle().Foreground(titleColor),
	}
}

func (theme theme) status(state screen) lipgloss.Style {
	statusColor := theme.colors.muted
	switch state {
	case screenRunning:
		statusColor = theme.colors.info
	case screenResults:
		statusColor = theme.colors.success
	case screenCanceled:
		statusColor = theme.colors.warning
	case screenFailed:
		statusColor = theme.colors.error
	}
	return lipgloss.NewStyle().Foreground(statusColor)
}
