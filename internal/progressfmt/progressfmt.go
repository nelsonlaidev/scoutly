// Package progressfmt formats audit progress values for terminal frontends.
package progressfmt

import (
	"fmt"
	"time"

	"github.com/nelsonlaidev/scoutly/audit"
)

// Duration formats elapsed time as unbounded minutes and two-digit seconds.
func Duration(duration time.Duration) string {
	if duration < 0 {
		duration = 0
	}
	totalSeconds := int(duration.Seconds())
	return fmt.Sprintf("%d:%02d", totalSeconds/60, totalSeconds%60)
}

// Resource formats a checked/total resource count. An unknown total is shown
// as two dashes.
func Resource(progress audit.ResourceProgress) string {
	total := "--"
	if progress.Total != nil {
		total = fmt.Sprintf("%d", *progress.Total)
	}
	return fmt.Sprintf("%d/%s", progress.Checked, total)
}
