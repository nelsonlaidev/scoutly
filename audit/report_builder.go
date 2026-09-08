package audit

import (
	"context"
	"fmt"
	"net/url"
	"strings"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/checker"
	"github.com/nelsonlaidev/scoutly/internal/crawler"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/ptrutil"
	"github.com/nelsonlaidev/scoutly/internal/resource"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

func buildReport(
	ctx context.Context,
	auditedURL *url.URL,
	crawledPages []crawler.CrawledPage,
	checkedLinks []checker.LinkResult,
	checkedImages []checker.ImageResult,
	imagesChecked bool,
	rules Rules,
	ignoreRedirects bool,
	auditedAt time.Time,
) (*Report, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	report := &Report{
		URL:       auditedURL.String(),
		AuditedAt: auditedAt.UTC(),
		Issues:    make([]Issue, 0),
		Pages:     make([]Page, 0, len(crawledPages)),
		Links:     make([]Link, 0, len(checkedLinks)),
		Images:    make([]Image, 0, len(checkedImages)),
	}

	linkOccurrences, err := collectLinkOccurrences(ctx, crawledPages)
	if err != nil {
		return nil, err
	}
	imageOccurrences := make(map[string][]ImageOccurrence)
	if imagesChecked {
		imageOccurrences, err = collectImageOccurrences(ctx, crawledPages)
		if err != nil {
			return nil, err
		}
	}

	linkResults := make(map[string]checker.LinkResult, len(checkedLinks))
	for _, result := range checkedLinks {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		linkResults[result.URL] = result
		report.Links = append(report.Links, publicLink(result, linkOccurrences[result.URL]))
	}
	for requestURL := range linkOccurrences {
		if _, exists := linkResults[requestURL]; !exists {
			return nil, fmt.Errorf("missing check result for link: %s", requestURL)
		}
	}

	imageResults := make(map[string]checker.ImageResult, len(checkedImages))
	if imagesChecked {
		for _, result := range checkedImages {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			imageResults[result.Key] = result
			report.Images = append(report.Images, publicImage(result, imageOccurrences[result.Key]))
		}
		for key := range imageOccurrences {
			if _, exists := imageResults[key]; !exists {
				return nil, fmt.Errorf("missing check result for image: %s", key)
			}
		}
	}

	for _, crawled := range crawledPages {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if page.IsHTMLContentType(crawled.Page.ContentType) {
			public, err := publicPage(ctx, crawled)
			if err != nil {
				return nil, err
			}
			report.Pages = append(report.Pages, public)
		}

		switch {
		case crawled.StatusCode == nil:
			report.Issues = append(report.Issues, Issue{
				Code:     IssuePageCrawlFailed,
				Severity: SeverityError,
				Message:  "Page crawl failed",
				Target:   IssueTarget{Type: TargetPage, URL: crawled.URL.String()},
			})
		case *crawled.StatusCode >= 400:
			report.Issues = append(report.Issues, Issue{
				Code:     IssuePageHTTPError,
				Severity: SeverityError,
				Message:  fmt.Sprintf("Page returned HTTP %d", *crawled.StatusCode),
				Target:   IssueTarget{Type: TargetPage, URL: crawled.URL.String()},
			})
		case page.IsHTMLContentType(crawled.Page.ContentType):
			report.Issues = append(report.Issues, analyzePage(
				crawled.Page,
				IssueTarget{Type: TargetPage, URL: crawled.URL.String()},
			)...)
		}
	}

	for _, link := range report.Links {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		report.Issues = append(report.Issues, analyzeLink(link)...)
	}
	if imagesChecked {
		for _, image := range report.Images {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			report.Issues = append(report.Issues, analyzeImage(image)...)
		}
	}
	report.Issues, err = applyRulePolicy(ctx, report.Issues, rules, ignoreRedirects)
	if err != nil {
		return nil, err
	}

	summary, err := summarize(ctx, report)
	if err != nil {
		return nil, err
	}
	report.Summary = summary
	return report, nil
}

func collectLinkOccurrences(
	ctx context.Context,
	pages []crawler.CrawledPage,
) (map[string][]LinkOccurrence, error) {
	result := make(map[string][]LinkOccurrence)
	for _, crawled := range pages {
		for _, link := range crawled.Page.Links {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			if link.URL == nil {
				continue
			}
			key := resource.Key(link.URL)
			result[key] = append(result[key], LinkOccurrence{
				PageURL:     crawled.URL.String(),
				OriginalURL: link.OriginalURL,
				Element:     string(link.Element),
				Text:        link.Text,
			})
		}
	}
	return result, nil
}

func collectImageOccurrences(
	ctx context.Context,
	pages []crawler.CrawledPage,
) (map[string][]ImageOccurrence, error) {
	result := make(map[string][]ImageOccurrence)
	for _, crawled := range pages {
		for _, reference := range crawled.Page.ImageReferences {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			key := checker.ImageReferenceKey(reference)
			result[key] = append(result[key], ImageOccurrence{
				PageURL:     crawled.URL.String(),
				OriginalURL: reference.OriginalURL,
				Element:     string(reference.Element),
				Attribute:   string(reference.Attribute),
				Descriptor:  ptrutil.Clone(reference.Descriptor),
				Alt:         ptrutil.Clone(reference.Alt),
			})
		}
	}
	return result, nil
}

func publicPage(ctx context.Context, crawled crawler.CrawledPage) (Page, error) {
	images := make([]PageImage, 0, len(crawled.Page.Images))
	for _, image := range crawled.Page.Images {
		if err := ctx.Err(); err != nil {
			return Page{}, err
		}
		images = append(images, PageImage{
			URL: urlutil.String(image.Src),
			Alt: ptrutil.Clone(image.Alt),
		})
	}

	return Page{
		URL:         crawled.URL.String(),
		Depth:       crawled.Depth,
		StatusCode:  ptrutil.Clone(crawled.StatusCode),
		ContentType: ptrutil.Clone(crawled.ContentType),
		Title:       ptrutil.Clone(crawled.Page.Title),
		Description: ptrutil.Clone(crawled.Page.Description),
		Headings:    Headings{H1: append([]string{}, crawled.Page.Headings.H1...)},
		Images:      images,
		OpenGraph: OpenGraph{
			Title:       ptrutil.Clone(crawled.Page.OpenGraph.Title),
			Description: ptrutil.Clone(crawled.Page.OpenGraph.Description),
			Image:       urlPointer(crawled.Page.OpenGraph.Image),
			URL:         urlPointer(crawled.Page.OpenGraph.URL),
			Type:        ptrutil.Clone(crawled.Page.OpenGraph.Type),
			SiteName:    ptrutil.Clone(crawled.Page.OpenGraph.SiteName),
			Locale:      ptrutil.Clone(crawled.Page.OpenGraph.Locale),
		},
	}, nil
}

func publicLink(result checker.LinkResult, occurrences []LinkOccurrence) Link {
	publicResult := LinkResult{Kind: publicResultKind(result.Kind)}
	switch result.Kind {
	case checker.ResultReachable, checker.ResultBlocked:
		statusCode := result.StatusCode
		finalURL := result.FinalURL
		if finalURL == "" {
			finalURL = result.RequestURL
		}
		publicResult.StatusCode = &statusCode
		publicResult.FinalURL = &finalURL
		if result.Kind == checker.ResultBlocked {
			publicResult.Reason = FailureReason(result.Failure)
		}
	case checker.ResultFailed:
		publicResult.Reason = FailureReason(result.Failure)
	case checker.ResultSkipped:
		publicResult.Reason = FailureReason(result.Reason)
	}

	return Link{
		URL:     result.URL,
		Result:  publicResult,
		FoundOn: append([]LinkOccurrence{}, occurrences...),
	}
}

func publicImage(result checker.ImageResult, occurrences []ImageOccurrence) Image {
	publicResult := ImageResult{Kind: publicResultKind(result.Kind)}
	switch result.Kind {
	case checker.ResultReachable, checker.ResultBlocked:
		statusCode := result.StatusCode
		finalURL := result.FinalURL
		if finalURL == "" {
			finalURL = result.URL
		}
		publicResult.StatusCode = &statusCode
		publicResult.FinalURL = &finalURL
		publicResult.ContentType = ptrutil.Clone(result.ContentType)
		if result.Kind == checker.ResultBlocked {
			publicResult.Reason = FailureReason(result.Failure)
		}
	case checker.ResultFailed:
		publicResult.Reason = FailureReason(result.Failure)
	case checker.ResultSkipped, checker.ResultInvalid:
		publicResult.Reason = FailureReason(result.Reason)
	}

	return Image{
		URL:     result.URL,
		Result:  publicResult,
		FoundOn: append([]ImageOccurrence{}, occurrences...),
	}
}

func publicResultKind(kind checker.ResultKind) ResultKind {
	if kind == checker.ResultReachable {
		return ResultResponse
	}
	return ResultKind(kind)
}

func analyzeLink(link Link) []Issue {
	result := link.Result
	target := IssueTarget{Type: TargetLink, URL: link.URL}
	switch result.Kind {
	case ResultFailed:
		return []Issue{{
			Code:     IssueBrokenLink,
			Severity: SeverityError,
			Message: fmt.Sprintf(
				"Link check failed: %s (%s)",
				link.URL,
				strings.ReplaceAll(string(result.Reason), "-", " "),
			),
			Target: target,
		}}
	case ResultSkipped:
		return []Issue{}
	}

	issues := make([]Issue, 0, 2)
	if result.Kind == ResultBlocked {
		issues = append(issues, Issue{
			Code:     IssueLinkCheckBlocked,
			Severity: SeverityWarning,
			Message: fmt.Sprintf(
				"Link check blocked by anti-bot challenge: %s (HTTP %d)",
				link.URL,
				*result.StatusCode,
			),
			Target: target,
		})
	}
	if link.IsRedirected() {
		issues = append(issues, Issue{
			Code:     IssueRedirect,
			Severity: SeverityInfo,
			Message:  fmt.Sprintf("Link redirected: %s -> %s", link.URL, *result.FinalURL),
			Target:   target,
		})
	}
	if link.IsBroken() {
		issues = append(issues, Issue{
			Code:     IssueBrokenLink,
			Severity: SeverityError,
			Message:  fmt.Sprintf("Broken link: %s (HTTP %d)", link.URL, *result.StatusCode),
			Target:   target,
		})
	}
	return issues
}

func analyzeImage(image Image) []Issue {
	result := image.Result
	target := IssueTarget{Type: TargetImage, URL: image.URL}
	switch result.Kind {
	case ResultInvalid:
		return []Issue{{
			Code:     IssueInvalidImageURL,
			Severity: SeverityError,
			Message:  "Invalid image URL: " + image.URL,
			Target:   target,
		}}
	case ResultFailed:
		return []Issue{{
			Code:     IssueBrokenImage,
			Severity: SeverityError,
			Message: fmt.Sprintf(
				"Image check failed: %s (%s)",
				image.URL,
				strings.ReplaceAll(string(result.Reason), "-", " "),
			),
			Target: target,
		}}
	case ResultSkipped:
		return []Issue{}
	}

	issues := make([]Issue, 0, 2)
	if result.Kind == ResultBlocked {
		issues = append(issues, Issue{
			Code:     IssueImageCheckBlocked,
			Severity: SeverityWarning,
			Message: fmt.Sprintf(
				"Image check blocked by anti-bot challenge: %s (HTTP %d)",
				image.URL,
				*result.StatusCode,
			),
			Target: target,
		})
	} else if image.IsBroken() {
		issues = append(issues, Issue{
			Code:     IssueBrokenImage,
			Severity: SeverityError,
			Message:  fmt.Sprintf("Broken image: %s (HTTP %d)", image.URL, *result.StatusCode),
			Target:   target,
		})
	} else if image.IsInvalid() {
		contentType := "missing"
		if result.ContentType != nil {
			contentType = *result.ContentType
		}
		issues = append(issues, Issue{
			Code:     IssueInvalidImageContentType,
			Severity: SeverityError,
			Message:  fmt.Sprintf("Image has invalid Content-Type: %s (%s)", image.URL, contentType),
			Target:   target,
		})
	}
	if image.IsRedirected() {
		issues = append(issues, Issue{
			Code:     IssueImageRedirect,
			Severity: SeverityInfo,
			Message:  fmt.Sprintf("Image redirected: %s -> %s", image.URL, *result.FinalURL),
			Target:   target,
		})
	}
	return issues
}

func summarize(ctx context.Context, report *Report) (Summary, error) {
	summary := Summary{
		Pages:  len(report.Pages),
		Links:  LinkSummary{Total: len(report.Links)},
		Images: ImageSummary{Total: len(report.Images)},
		Issues: IssueSummary{Total: len(report.Issues)},
	}

	for _, link := range report.Links {
		if err := ctx.Err(); err != nil {
			return Summary{}, err
		}
		if link.Result.Kind != ResultSkipped {
			summary.Links.Checked++
		}
		if link.Result.Kind == ResultBlocked {
			summary.Links.Blocked++
		}
		if link.IsBroken() {
			summary.Links.Broken++
		}
		if link.IsRedirected() {
			summary.Links.Redirected++
		}
	}
	for _, image := range report.Images {
		if err := ctx.Err(); err != nil {
			return Summary{}, err
		}
		if image.Result.Kind == ResultResponse ||
			image.Result.Kind == ResultBlocked ||
			image.Result.Kind == ResultFailed {
			summary.Images.Checked++
		}
		if image.Result.Kind == ResultBlocked {
			summary.Images.Blocked++
		}
		if image.IsBroken() {
			summary.Images.Broken++
		}
		if image.IsInvalid() {
			summary.Images.Invalid++
		}
		if image.IsRedirected() {
			summary.Images.Redirected++
		}
	}

	for _, issue := range report.Issues {
		if err := ctx.Err(); err != nil {
			return Summary{}, err
		}
		switch issue.Severity {
		case SeverityError:
			summary.Issues.Error++
		case SeverityWarning:
			summary.Issues.Warning++
		case SeverityInfo:
			summary.Issues.Info++
		}
	}
	return summary, nil
}

func isImageContentType(contentType *string) bool {
	return contentType != nil &&
		strings.HasPrefix(strings.ToLower(strings.TrimSpace(*contentType)), "image/")
}

func urlPointer(input *url.URL) *string {
	if input == nil {
		return nil
	}
	return new(input.String())
}
