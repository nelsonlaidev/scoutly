package tui

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/nelsonlaidev/scoutly/audit"
)

func describeLink(link audit.Link) string {
	switch link.Result.Kind {
	case audit.ResultResponse:
		description := fmt.Sprintf("HTTP %s", intValue(link.Result.StatusCode, "unknown"))
		if link.IsRedirected() {
			description += " -> " + stringValue(link.Result.FinalURL, "")
		}
		return description
	case audit.ResultBlocked:
		description := fmt.Sprintf(
			"BLOCKED: %s (HTTP %s)",
			link.Result.Reason,
			intValue(link.Result.StatusCode, "unknown"),
		)
		if link.IsRedirected() {
			description += " -> " + stringValue(link.Result.FinalURL, "")
		}
		return description
	default:
		return fmt.Sprintf("%s: %s", link.Result.Kind, link.Result.Reason)
	}
}

func describeImage(image audit.Image) string {
	switch image.Result.Kind {
	case audit.ResultResponse:
		description := fmt.Sprintf(
			"HTTP %s %s",
			intValue(image.Result.StatusCode, "unknown"),
			stringValue(image.Result.ContentType, "(missing Content-Type)"),
		)
		if image.IsRedirected() {
			description += " -> " + stringValue(image.Result.FinalURL, "")
		}
		return description
	case audit.ResultBlocked:
		description := fmt.Sprintf(
			"BLOCKED: %s (HTTP %s)",
			image.Result.Reason,
			intValue(image.Result.StatusCode, "unknown"),
		)
		if image.IsRedirected() {
			description += " -> " + stringValue(image.Result.FinalURL, "")
		}
		return description
	default:
		return fmt.Sprintf("%s: %s", image.Result.Kind, image.Result.Reason)
	}
}

func createDetailLines(item resultItem, report *audit.Report) []string {
	switch item.kind {
	case resultIssue:
		issue := item.issue
		lines := []string{
			issue.Message,
			"",
			"Severity: " + string(issue.Severity),
			"Code: " + string(issue.Code),
			"Target: " + string(issue.Target.Type),
			"URL: " + issue.Target.URL,
		}
		switch issue.Target.Type {
		case audit.TargetLink:
			for _, link := range report.Links {
				if link.URL == issue.Target.URL {
					return append(lines, linkOccurrenceLines(link.FoundOn)...)
				}
			}
		case audit.TargetImage:
			for _, image := range report.Images {
				if image.URL == issue.Target.URL {
					return append(lines, imageOccurrenceLines(image.FoundOn)...)
				}
			}
		}
		return lines

	case resultPage:
		page := item.page
		return []string{
			stringValue(page.Title, "(untitled page)"),
			"",
			"URL: " + page.URL,
			fmt.Sprintf("Depth: %d", page.Depth),
			"Status: " + intValue(page.StatusCode, "failed"),
			"Content-Type: " + stringValue(page.ContentType, "unknown"),
			"Description: " + stringValue(page.Description, "(missing)"),
			"H1: " + fallback(strings.Join(page.Headings.H1, " | "), "(missing)"),
			fmt.Sprintf("Images: %d", len(page.Images)),
			"",
			"Open Graph",
			"Title: " + stringValue(page.OpenGraph.Title, "(missing)"),
			"Description: " + stringValue(page.OpenGraph.Description, "(missing)"),
			"Image: " + stringValue(page.OpenGraph.Image, "(missing)"),
			"URL: " + stringValue(page.OpenGraph.URL, "(missing)"),
			"Type: " + stringValue(page.OpenGraph.Type, "(missing)"),
			"Site name: " + stringValue(page.OpenGraph.SiteName, "(missing)"),
			"Locale: " + stringValue(page.OpenGraph.Locale, "(missing)"),
		}

	case resultLink:
		link := item.link
		lines := []string{link.URL, "", describeLink(link)}
		if link.Result.Kind == audit.ResultResponse || link.Result.Kind == audit.ResultBlocked {
			lines = append(
				lines,
				"Status: "+intValue(link.Result.StatusCode, "unknown"),
				"Final URL: "+stringValue(link.Result.FinalURL, "(missing)"),
			)
			if link.Result.Kind == audit.ResultBlocked {
				lines = append(lines, "Reason: "+string(link.Result.Reason))
			}
		} else {
			lines = append(lines, "Reason: "+string(link.Result.Reason))
		}
		return append(lines, linkOccurrenceLines(link.FoundOn)...)

	case resultImage:
		image := item.image
		lines := []string{image.URL, "", describeImage(image)}
		if image.Result.Kind == audit.ResultResponse || image.Result.Kind == audit.ResultBlocked {
			lines = append(
				lines,
				"Status: "+intValue(image.Result.StatusCode, "unknown"),
				"Final URL: "+stringValue(image.Result.FinalURL, "(missing)"),
				"Content-Type: "+stringValue(image.Result.ContentType, "(missing)"),
			)
			if image.Result.Kind == audit.ResultBlocked {
				lines = append(lines, "Reason: "+string(image.Result.Reason))
			}
		} else {
			lines = append(lines, "Reason: "+string(image.Result.Reason))
		}
		return append(lines, imageOccurrenceLines(image.FoundOn)...)
	}
	return []string{"No item selected."}
}

func linkOccurrenceLines(occurrences []audit.LinkOccurrence) []string {
	if len(occurrences) == 0 {
		return nil
	}
	lines := []string{"", "Found on:"}
	for _, occurrence := range occurrences {
		lines = append(
			lines,
			"- "+occurrence.PageURL,
			"  Original: "+occurrence.OriginalURL,
			fmt.Sprintf("  <%s> %s", occurrence.Element, fallback(occurrence.Text, "(no text)")),
		)
	}
	return lines
}

func imageOccurrenceLines(occurrences []audit.ImageOccurrence) []string {
	if len(occurrences) == 0 {
		return nil
	}
	lines := []string{"", "Found on:"}
	for _, occurrence := range occurrences {
		lines = append(
			lines,
			"- "+occurrence.PageURL,
			"  Original: "+occurrence.OriginalURL,
			fmt.Sprintf("  Source: <%s> %s", occurrence.Element, occurrence.Attribute),
			"  Descriptor: "+stringValue(occurrence.Descriptor, "(none)"),
			"  Alt: "+stringValue(occurrence.Alt, "(missing)"),
		)
	}
	return lines
}

func stringValue(value *string, fallback string) string {
	if value == nil {
		return fallback
	}
	return *value
}

func intValue(value *int, fallback string) string {
	if value == nil {
		return fallback
	}
	return strconv.Itoa(*value)
}
