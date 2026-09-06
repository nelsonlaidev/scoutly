package cli

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/nelsonlaidev/scoutly/audit"
)

func formatReport(report *audit.Report, format string) (string, error) {
	if format == "json" {
		var buffer bytes.Buffer
		encoder := json.NewEncoder(&buffer)
		encoder.SetIndent("", "  ")
		encoder.SetEscapeHTML(false)
		if err := encoder.Encode(report); err != nil {
			return "", fmt.Errorf("encode JSON report: %w", err)
		}

		return strings.TrimSuffix(buffer.String(), "\n"), nil
	}

	return formatTextReport(report), nil
}

func formatTextReport(report *audit.Report) string {
	summary := report.Summary
	lines := []string{
		"Scoutly Audit Report",
		"URL: " + report.URL,
		"Audited: " + report.AuditedAt.Format("2006-01-02T15:04:05.000Z07:00"),
		"",
		"Summary",
		fmt.Sprintf("Pages: %d", summary.Pages),
		fmt.Sprintf(
			"Links: %d total, %d checked, %d broken, %d blocked, %d redirected",
			summary.Links.Total,
			summary.Links.Checked,
			summary.Links.Broken,
			summary.Links.Blocked,
			summary.Links.Redirected,
		),
		fmt.Sprintf(
			"Images: %d total, %d checked, %d broken, %d blocked, %d invalid, %d redirected",
			summary.Images.Total,
			summary.Images.Checked,
			summary.Images.Broken,
			summary.Images.Blocked,
			summary.Images.Invalid,
			summary.Images.Redirected,
		),
		fmt.Sprintf(
			"Issues: %d total, %s, %s, %d info",
			summary.Issues.Total,
			formatCount(summary.Issues.Error, "error", "errors"),
			formatCount(summary.Issues.Warning, "warning", "warnings"),
			summary.Issues.Info,
		),
	}

	if len(report.Issues) == 0 {
		lines = append(lines, "", "No issues found.")
		return strings.Join(lines, "\n")
	}

	linksByURL := make(map[string]audit.Link, len(report.Links))
	for _, link := range report.Links {
		linksByURL[link.URL] = link
	}
	imagesByURL := make(map[string]audit.Image, len(report.Images))
	for _, image := range report.Images {
		imagesByURL[image.URL] = image
	}

	sections := []struct {
		severity audit.Severity
		heading  string
	}{
		{severity: audit.SeverityError, heading: "Errors"},
		{severity: audit.SeverityWarning, heading: "Warnings"},
		{severity: audit.SeverityInfo, heading: "Info"},
	}
	for _, section := range sections {
		issues := make([]audit.Issue, 0)
		for _, issue := range report.Issues {
			if issue.Severity == section.severity {
				issues = append(issues, issue)
			}
		}
		if len(issues) == 0 {
			continue
		}

		lines = append(lines, "", fmt.Sprintf("%s (%d)", section.heading, len(issues)))
		for _, issue := range issues {
			lines = append(lines, formatIssue(issue, linksByURL, imagesByURL)...)
		}
	}

	return strings.Join(lines, "\n")
}

func formatCount(count int, singular, plural string) string {
	label := plural
	if count == 1 {
		label = singular
	}
	return fmt.Sprintf("%d %s", count, label)
}

func formatIssue(
	issue audit.Issue,
	linksByURL map[string]audit.Link,
	imagesByURL map[string]audit.Image,
) []string {
	targetLabels := map[audit.TargetType]string{
		audit.TargetPage:  "Page",
		audit.TargetLink:  "Link",
		audit.TargetImage: "Image",
	}
	lines := []string{
		fmt.Sprintf("- %s [%s]", issue.Message, issue.Code),
		fmt.Sprintf("  %s: %s", targetLabels[issue.Target.Type], issue.Target.URL),
	}

	seenPages := make(map[string]struct{})
	switch issue.Target.Type {
	case audit.TargetLink:
		for _, occurrence := range linksByURL[issue.Target.URL].FoundOn {
			if _, ok := seenPages[occurrence.PageURL]; ok {
				continue
			}
			seenPages[occurrence.PageURL] = struct{}{}
			lines = append(lines, "  Found on: "+occurrence.PageURL)
		}
	case audit.TargetImage:
		for _, occurrence := range imagesByURL[issue.Target.URL].FoundOn {
			if _, ok := seenPages[occurrence.PageURL]; ok {
				continue
			}
			seenPages[occurrence.PageURL] = struct{}{}
			lines = append(lines, "  Found on: "+occurrence.PageURL)
		}
	}

	return lines
}
