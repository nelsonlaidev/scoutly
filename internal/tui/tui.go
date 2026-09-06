// Package tui provides Scoutly's interactive terminal user interface.
package tui

import (
	"context"
	"errors"
	"fmt"

	tea "charm.land/bubbletea/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

// Run starts the full-screen TUI with the supplied audit options.
func Run(ctx context.Context, options audit.Options) error {
	if ctx == nil {
		return fmt.Errorf("TUI context is nil")
	}
	if err := options.Validate(); err != nil {
		return fmt.Errorf("validate TUI options: %w", err)
	}

	runContext, cancel := context.WithCancel(ctx)
	defer cancel()

	program := tea.NewProgram(
		newModel(runContext, options),
		tea.WithContext(runContext),
	)
	_, err := program.Run()
	if err == nil {
		return nil
	}
	if errors.Is(err, tea.ErrProgramKilled) && ctx.Err() != nil {
		return context.Cause(ctx)
	}
	if errors.Is(err, tea.ErrInterrupted) {
		return context.Canceled
	}
	return fmt.Errorf("run TUI: %w", err)
}
