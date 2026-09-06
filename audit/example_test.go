package audit_test

import (
	"context"
	"fmt"

	"github.com/nelsonlaidev/scoutly/audit"
)

func ExampleAudit() {
	options := audit.DefaultOptions()
	options.MaxDepth = 2
	options.MaxPages = 100

	report, err := audit.Audit(
		context.Background(),
		"https://example.com",
		options,
		func(progress audit.Progress) error {
			fmt.Println(progress.Phase, progress.CurrentURL)
			return nil
		},
	)
	if err != nil {
		return
	}

	fmt.Println(report.Summary.Issues.Total)
}
