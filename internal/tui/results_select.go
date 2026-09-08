package tui

import (
	"strings"

	"github.com/nelsonlaidev/scoutly/audit"
)

func selectResultItems(report *audit.Report, tab resultTab, query, filter string) []resultItem {
	if report == nil {
		return nil
	}
	query = strings.ToLower(strings.TrimSpace(query))
	includes := func(values ...string) bool {
		if query == "" {
			return true
		}
		for _, value := range values {
			if strings.Contains(strings.ToLower(value), query) {
				return true
			}
		}
		return false
	}

	items := make([]resultItem, 0)
	switch tab {
	case tabIssues:
		for _, issue := range report.Issues {
			if filter != "all" && string(issue.Severity) != filter {
				continue
			}
			if !includes(issue.Message, string(issue.Code), issue.Target.URL) {
				continue
			}
			items = append(items, resultItem{
				kind:        resultIssue,
				label:       issue.Message,
				description: strings.ToUpper(string(issue.Severity)) + "  " + issue.Target.URL,
				issue:       issue,
			})
		}

	case tabPages:
		for _, page := range report.Pages {
			if !matchesPageFilter(page, filter) {
				continue
			}
			if !includes(page.URL, stringValue(page.Title, ""), intValue(page.StatusCode, "")) {
				continue
			}
			items = append(items, resultItem{
				kind:        resultPage,
				label:       stringValue(page.Title, page.URL),
				description: intValue(page.StatusCode, "FAILED") + "  " + page.URL,
				page:        page,
			})
		}

	case tabLinks:
		for _, link := range report.Links {
			if !matchesLinkFilter(link, filter) {
				continue
			}
			description := describeLink(link)
			if !includes(link.URL, description) {
				continue
			}
			items = append(items, resultItem{
				kind:        resultLink,
				label:       link.URL,
				description: description,
				link:        link,
			})
		}

	case tabImages:
		for _, image := range report.Images {
			if !matchesImageFilter(image, filter) {
				continue
			}
			description := describeImage(image)
			if !includes(image.URL, description) {
				continue
			}
			items = append(items, resultItem{
				kind:        resultImage,
				label:       image.URL,
				description: description,
				image:       image,
			})
		}
	}
	return items
}

func matchesPageFilter(page audit.Page, filter string) bool {
	switch filter {
	case "failed":
		return page.StatusCode == nil
	case "healthy":
		return page.StatusCode != nil && *page.StatusCode >= 200 && *page.StatusCode < 300
	case "non-2xx":
		return page.StatusCode != nil && (*page.StatusCode < 200 || *page.StatusCode >= 300)
	default:
		return true
	}
}

func matchesLinkFilter(link audit.Link, filter string) bool {
	isBroken := link.IsBroken()
	blocked := link.Result.Kind == audit.ResultBlocked
	redirected := link.IsRedirected()
	skipped := link.Result.Kind == audit.ResultSkipped
	switch filter {
	case "healthy":
		return !isBroken && !blocked && !redirected && !skipped
	case "broken":
		return isBroken
	case "blocked":
		return blocked
	case "redirected":
		return redirected
	case "skipped":
		return skipped
	default:
		return true
	}
}

func matchesImageFilter(
	image audit.Image,
	filter string,
) bool {
	isBroken := image.IsBroken()
	isInvalid := image.IsInvalid()
	blocked := image.Result.Kind == audit.ResultBlocked
	redirected := image.IsRedirected()
	skipped := image.Result.Kind == audit.ResultSkipped
	switch filter {
	case "healthy":
		return !isBroken && !blocked && !isInvalid && !redirected && !skipped
	case "broken":
		return isBroken
	case "blocked":
		return blocked
	case "invalid":
		return isInvalid
	case "redirected":
		return redirected
	case "skipped":
		return skipped
	default:
		return true
	}
}
