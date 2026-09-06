package cli

import (
	"context"
	"fmt"
	"io"

	"github.com/nelsonlaidev/scoutly/audit"
	"github.com/nelsonlaidev/scoutly/internal/config"
)

func Run(
	ctx context.Context,
	target string,
	cfg config.Config,
	stdout io.Writer,
	stderr io.Writer,
) (err error) {
	if stdout == nil {
		return fmt.Errorf("CLI stdout writer is nil")
	}
	if stderr == nil {
		return fmt.Errorf("CLI stderr writer is nil")
	}

	progress := newProgressDisplay(cfg.Progress, stderr)
	defer func() {
		if closeErr := progress.Close(); closeErr != nil && err == nil {
			err = fmt.Errorf("close progress display: %w", closeErr)
		}
	}()

	report, err := audit.Audit(ctx, target, cfg.AuditOptions(), progress.Update)
	if err != nil {
		return err
	}
	if err := progress.Close(); err != nil {
		return fmt.Errorf("close progress display: %w", err)
	}

	output, err := formatReport(report, cfg.Format)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintln(stdout, output); err != nil {
		return fmt.Errorf("write audit report: %w", err)
	}

	return nil
}
