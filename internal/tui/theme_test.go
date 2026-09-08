package tui

import (
	"reflect"
	"testing"

	"charm.land/lipgloss/v2"
)

func TestPaletteUsesMonochromeVercelColors(t *testing.T) {
	tests := []struct {
		name               string
		dark               bool
		accent             string
		muted              string
		border             string
		selectedForeground string
		selectedBackground string
	}{
		{
			name:               "light",
			accent:             "#000000",
			muted:              "#666666",
			border:             "#EAEAEA",
			selectedForeground: "#FFFFFF",
			selectedBackground: "#000000",
		},
		{
			name:               "dark",
			dark:               true,
			accent:             "#FFFFFF",
			muted:              "#A1A1A1",
			border:             "#333333",
			selectedForeground: "#000000",
			selectedBackground: "#FFFFFF",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			colors := newPalette(test.dark)
			for name, comparison := range map[string]struct {
				got  any
				want any
			}{
				"accent":              {got: colors.accent, want: lipgloss.Color(test.accent)},
				"muted":               {got: colors.muted, want: lipgloss.Color(test.muted)},
				"border":              {got: colors.border, want: lipgloss.Color(test.border)},
				"selected foreground": {got: colors.selectedForeground, want: lipgloss.Color(test.selectedForeground)},
				"selected background": {got: colors.selectedBackground, want: lipgloss.Color(test.selectedBackground)},
				"error":               {got: colors.error, want: lipgloss.Color(test.accent)},
				"warning":             {got: colors.warning, want: lipgloss.Color(test.accent)},
				"info":                {got: colors.info, want: lipgloss.Color(test.accent)},
				"success":             {got: colors.success, want: lipgloss.Color(test.accent)},
			} {
				if !reflect.DeepEqual(comparison.got, comparison.want) {
					t.Errorf("%s = %v, want %v", name, comparison.got, comparison.want)
				}
			}
		})
	}
}

func TestHuhThemeUsesApplicationPalette(t *testing.T) {
	for _, dark := range []bool{false, true} {
		styles := newHuhTheme(dark)
		colors := newPalette(dark)

		if got := styles.Focused.Title.GetForeground(); !reflect.DeepEqual(got, colors.accent) {
			t.Errorf("dark=%t focused title color = %v, want %v", dark, got, colors.accent)
		}
		if !styles.Focused.Title.GetBold() {
			t.Errorf("dark=%t focused title is not bold", dark)
		}
		if got := styles.Blurred.Title.GetForeground(); !reflect.DeepEqual(got, colors.muted) {
			t.Errorf("dark=%t blurred title color = %v, want %v", dark, got, colors.muted)
		}
		if styles.Blurred.Title.GetBold() {
			t.Errorf("dark=%t blurred title is bold", dark)
		}
		if got := styles.Focused.ErrorMessage.GetForeground(); !reflect.DeepEqual(got, colors.error) {
			t.Errorf("dark=%t error color = %v, want %v", dark, got, colors.error)
		}
		for name, comparison := range map[string]struct {
			got            lipgloss.Style
			wantForeground any
			wantBackground any
		}{
			"focused selected confirm": {
				got:            styles.Focused.FocusedButton,
				wantForeground: colors.selectedForeground,
				wantBackground: colors.selectedBackground,
			},
			"focused unselected confirm": {
				got:            styles.Focused.BlurredButton,
				wantForeground: colors.selectedBackground,
				wantBackground: colors.selectedForeground,
			},
			"blurred selected confirm": {
				got:            styles.Blurred.FocusedButton,
				wantForeground: colors.selectedForeground,
				wantBackground: colors.selectedBackground,
			},
			"blurred unselected confirm": {
				got:            styles.Blurred.BlurredButton,
				wantForeground: colors.selectedBackground,
				wantBackground: colors.selectedForeground,
			},
		} {
			if got := comparison.got.GetForeground(); !reflect.DeepEqual(got, comparison.wantForeground) {
				t.Errorf("dark=%t %s foreground = %v, want %v", dark, name, got, comparison.wantForeground)
			}
			if got := comparison.got.GetBackground(); !reflect.DeepEqual(got, comparison.wantBackground) {
				t.Errorf("dark=%t %s background = %v, want %v", dark, name, got, comparison.wantBackground)
			}
		}
		if got := styles.Focused.TextInput.Prompt.GetForeground(); !reflect.DeepEqual(got, colors.accent) {
			t.Errorf("dark=%t input prompt color = %v, want %v", dark, got, colors.accent)
		}
		if got := styles.Blurred.Base.GetBorderStyle(); !reflect.DeepEqual(got, lipgloss.HiddenBorder()) {
			t.Errorf("dark=%t blurred border = %#v, want hidden border", dark, got)
		}
	}
}

func TestSetupThemeUsesOneBackgroundModeForEveryField(t *testing.T) {
	for _, dark := range []bool{false, true} {
		theme := &setupTheme{dark: dark}
		colors := newPalette(dark)

		for _, fieldDark := range []bool{false, true} {
			styles := theme.Theme(fieldDark)
			if got := styles.Focused.Title.GetForeground(); !reflect.DeepEqual(got, colors.accent) {
				t.Errorf(
					"dark=%t fieldDark=%t focused title color = %v, want %v",
					dark,
					fieldDark,
					got,
					colors.accent,
				)
			}
			if got := styles.Blurred.Title.GetForeground(); !reflect.DeepEqual(got, colors.muted) {
				t.Errorf(
					"dark=%t fieldDark=%t blurred title color = %v, want %v",
					dark,
					fieldDark,
					got,
					colors.muted,
				)
			}
		}
	}
}

func TestThemeUsesInverseTableSelection(t *testing.T) {
	for _, dark := range []bool{false, true} {
		theme := newTheme(dark)
		if theme.table.Header.GetBold() {
			t.Errorf("dark=%t table header is bold", dark)
		}
		if got := theme.table.Selected.GetForeground(); !reflect.DeepEqual(got, theme.colors.selectedForeground) {
			t.Errorf("dark=%t selected foreground = %v, want %v", dark, got, theme.colors.selectedForeground)
		}
		if got := theme.table.Selected.GetBackground(); !reflect.DeepEqual(got, theme.colors.selectedBackground) {
			t.Errorf("dark=%t selected background = %v, want %v", dark, got, theme.colors.selectedBackground)
		}
		if !theme.table.Selected.GetBold() {
			t.Errorf("dark=%t selected row is not bold", dark)
		}
	}
}
