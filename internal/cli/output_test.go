package cli

import (
	"strings"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestFormatReportJSONUsesSnakeCase(t *testing.T) {
	report := &audit.Report{
		URL:       "https://example.com/",
		AuditedAt: time.Date(2026, 7, 31, 12, 0, 0, 0, time.UTC),
		Issues:    []audit.Issue{},
		Pages:     []audit.Page{},
		Links:     []audit.Link{},
		Images:    []audit.Image{},
	}

	output, err := formatReport(report, "json")
	if err != nil {
		t.Fatalf("formatReport() error = %v", err)
	}
	if !strings.Contains(output, `"audited_at"`) || strings.Contains(output, `"auditedAt"`) {
		t.Fatalf("output = %s", output)
	}
}

func TestFormatTextReportGroupsIssuesAndOccurrences(t *testing.T) {
	report := &audit.Report{
		URL:       "https://example.com/",
		AuditedAt: time.Date(2026, 7, 31, 12, 0, 0, 0, time.UTC),
		Summary: audit.Summary{
			Links:  audit.LinkSummary{Total: 1, Checked: 1, Broken: 1},
			Issues: audit.IssueSummary{Total: 1, Error: 1},
		},
		Issues: []audit.Issue{{
			Code:     audit.IssueBrokenLink,
			Severity: audit.SeverityError,
			Message:  "Broken link",
			Target:   audit.IssueTarget{Type: audit.TargetLink, URL: "https://example.com/broken"},
		}},
		Pages: []audit.Page{},
		Links: []audit.Link{{
			URL: "https://example.com/broken",
			FoundOn: []audit.LinkOccurrence{
				{PageURL: "https://example.com/"},
				{PageURL: "https://example.com/"},
			},
		}},
		Images: []audit.Image{},
	}

	output := formatTextReport(report)
	if !strings.Contains(output, "Errors (1)") {
		t.Fatalf("output = %s", output)
	}
	if got := strings.Count(output, "Found on: https://example.com/"); got != 1 {
		t.Fatalf("Found on count = %d, output = %s", got, output)
	}
}

func TestFormatTextReportIncludesBlockedCounts(t *testing.T) {
	report := &audit.Report{
		URL:       "https://example.com/",
		AuditedAt: time.Date(2026, 8, 2, 12, 0, 0, 0, time.UTC),
		Summary: audit.Summary{
			Links:  audit.LinkSummary{Total: 2, Checked: 2, Blocked: 1},
			Images: audit.ImageSummary{Total: 1, Checked: 1, Blocked: 1},
		},
		Issues: []audit.Issue{},
		Pages:  []audit.Page{},
		Links:  []audit.Link{},
		Images: []audit.Image{},
	}

	output := formatTextReport(report)
	for _, expected := range []string{
		"Links: 2 total, 2 checked, 0 broken, 1 blocked, 0 redirected",
		"Images: 1 total, 1 checked, 0 broken, 1 blocked, 0 invalid, 0 redirected",
	} {
		if !strings.Contains(output, expected) {
			t.Errorf("output missing %q:\n%s", expected, output)
		}
	}
}

func TestFormatTextReportIncludesImageOccurrencesAndAllSeverities(t *testing.T) {
	t.Parallel()

	imageURL := "https://example.com/image.png"
	report := &audit.Report{
		URL:       "https://example.com/",
		AuditedAt: time.Date(2026, 8, 2, 12, 0, 0, 0, time.UTC),
		Summary:   audit.Summary{Issues: audit.IssueSummary{Total: 3, Error: 1, Warning: 1, Info: 1}},
		Issues: []audit.Issue{
			{Code: audit.IssueInvalidImageContentType, Severity: audit.SeverityError, Message: "Invalid image", Target: audit.IssueTarget{Type: audit.TargetImage, URL: imageURL}},
			{Code: audit.IssueTitleTooShort, Severity: audit.SeverityWarning, Message: "Short title", Target: audit.IssueTarget{Type: audit.TargetPage, URL: "https://example.com/"}},
			{Code: audit.IssueRedirect, Severity: audit.SeverityInfo, Message: "Redirect", Target: audit.IssueTarget{Type: audit.TargetLink, URL: "https://example.com/link"}},
		},
		Images: []audit.Image{{
			URL: imageURL,
			FoundOn: []audit.ImageOccurrence{
				{PageURL: "https://example.com/one"},
				{PageURL: "https://example.com/one"},
				{PageURL: "https://example.com/two"},
			},
		}},
	}
	output := formatTextReport(report)
	for _, expected := range []string{"Errors (1)", "Warnings (1)", "Info (1)", "Found on: https://example.com/one", "Found on: https://example.com/two"} {
		if !strings.Contains(output, expected) {
			t.Errorf("output missing %q:\n%s", expected, output)
		}
	}
	if got := strings.Count(output, "Found on: https://example.com/one"); got != 1 {
		t.Fatalf("duplicate image occurrence count = %d", got)
	}
}
