package audit

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/checker"
	"github.com/nelsonlaidev/scoutly/internal/crawler"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/resource"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestBuildReportAggregatesPagesResourcesAndIssues(t *testing.T) {
	start := testutil.ParseURL(t, "https://example.com/")
	linkURL := testutil.ParseURL(t, "https://example.com/broken")
	title := "Short"
	description := "Short"
	contentType := "text/html"
	status := 200
	originalImage := "%"
	crawled := []crawler.CrawledPage{{
		URL:         start,
		Depth:       0,
		StatusCode:  &status,
		ContentType: &contentType,
		Page: page.Page{
			ContentType: contentType,
			Title:       &title,
			Description: &description,
			Headings:    page.Headings{H1: []string{}},
			Links: []page.Link{{
				Element: page.LinkElementAnchor,
				URL:     linkURL,
				Text:    "Broken",
			}},
			Images:        []page.Image{},
			ImageAltTexts: []*string{},
			ImageReferences: []page.ImageReference{{
				URL:         nil,
				OriginalURL: originalImage,
				Element:     page.ImageReferenceElementImage,
				Attribute:   page.ImageReferenceAttributeSrc,
			}},
		},
	}}
	links := []checker.LinkResult{{
		URL:        linkURL.String(),
		RequestURL: linkURL.String(),
		Kind:       checker.ResultReachable,
		StatusCode: 404,
		FinalURL:   linkURL.String(),
	}}
	images := []checker.ImageResult{{
		Key:    "invalid:" + originalImage,
		URL:    originalImage,
		Kind:   checker.ResultInvalid,
		Reason: checker.ReasonInvalidURL,
	}}

	report, err := buildReport(
		context.Background(),
		start,
		crawled,
		links,
		images,
		true,
		false,
		time.Date(2026, 7, 31, 12, 0, 0, 0, time.UTC),
	)
	if err != nil {
		t.Fatalf("buildReport() error = %v", err)
	}

	if len(report.Pages) != 1 || len(report.Links) != 1 || len(report.Images) != 1 {
		t.Fatalf("report resources = %d/%d/%d", len(report.Pages), len(report.Links), len(report.Images))
	}
	if len(report.Links[0].FoundOn) != 1 || len(report.Images[0].FoundOn) != 1 {
		t.Fatalf("occurrences = %#v / %#v", report.Links[0].FoundOn, report.Images[0].FoundOn)
	}
	if report.Summary.Links.Broken != 1 || report.Summary.Images.Invalid != 1 {
		t.Fatalf("summary = %#v", report.Summary)
	}
	if report.Summary.Links.Checked != 1 ||
		report.Links[0].Result.Kind != ResultResponse {
		t.Fatalf("link result/summary = %#v / %#v", report.Links[0].Result, report.Summary.Links)
	}
	if !hasIssue(report.Issues, IssueBrokenLink) || !hasIssue(report.Issues, IssueInvalidImageURL) {
		t.Fatalf("issues = %#v", report.Issues)
	}
}

func TestBuildReportClassifiesPageFailuresAndNonHTML(t *testing.T) {
	start := testutil.ParseURL(t, "https://example.com/")
	jsonURL := testutil.ParseURL(t, "https://example.com/data")
	jsonType := "application/json"
	httpError := 500
	crawled := []crawler.CrawledPage{
		{
			URL:   start,
			Depth: 0,
			Page: page.Page{
				Headings:        page.Headings{H1: []string{}},
				Links:           []page.Link{},
				Images:          []page.Image{},
				ImageAltTexts:   []*string{},
				ImageReferences: []page.ImageReference{},
			},
		},
		{
			URL:         jsonURL,
			Depth:       1,
			StatusCode:  &httpError,
			ContentType: &jsonType,
			Page: page.Page{
				ContentType:     jsonType,
				Headings:        page.Headings{H1: []string{}},
				Links:           []page.Link{},
				Images:          []page.Image{},
				ImageAltTexts:   []*string{},
				ImageReferences: []page.ImageReference{},
			},
		},
	}

	report, err := buildReport(
		context.Background(),
		start,
		crawled,
		nil,
		nil,
		false,
		false,
		time.Now(),
	)
	if err != nil {
		t.Fatalf("buildReport() error = %v", err)
	}
	if len(report.Pages) != 1 {
		t.Fatalf("pages = %d, want failed missing-content-type page only", len(report.Pages))
	}
	if !hasIssue(report.Issues, IssuePageCrawlFailed) || !hasIssue(report.Issues, IssuePageHTTPError) {
		t.Fatalf("issues = %#v", report.Issues)
	}
}

func TestAnalyzeRedirectsCanBeIgnored(t *testing.T) {
	result := checker.LinkResult{
		URL:        "https://example.com/a",
		RequestURL: "https://example.com/a",
		Kind:       checker.ResultReachable,
		StatusCode: 200,
		FinalURL:   "https://example.com/b",
	}
	if issues := analyzeLink(result, true); len(issues) != 0 {
		t.Fatalf("issues = %#v", issues)
	}
	if issues := analyzeLink(result, false); len(issues) != 1 || issues[0].Code != IssueRedirect {
		t.Fatalf("issues = %#v", issues)
	}
}

func TestBuildReportClassifiesBlockedResources(t *testing.T) {
	start := testutil.ParseURL(t, "https://example.com/")
	linkURL := "https://example.com/blocked-link"
	linkFinalURL := "https://other.example/blocked-link"
	imageURL := "https://example.com/blocked-image.png"
	contentType := "text/html"
	links := []checker.LinkResult{{
		URL:        linkURL,
		RequestURL: linkURL,
		Kind:       checker.ResultBlocked,
		StatusCode: 403,
		FinalURL:   linkFinalURL,
		Failure:    resource.FailureAntiBotChallenge,
	}}
	images := []checker.ImageResult{{
		Key:         imageURL,
		URL:         imageURL,
		Kind:        checker.ResultBlocked,
		StatusCode:  403,
		FinalURL:    imageURL,
		ContentType: &contentType,
		Failure:     resource.FailureAntiBotChallenge,
	}}

	report, err := buildReport(
		context.Background(),
		start,
		nil,
		links,
		images,
		true,
		false,
		time.Now(),
	)
	if err != nil {
		t.Fatalf("buildReport() error = %v", err)
	}

	if got := report.Links[0].Result; got.Kind != ResultBlocked ||
		got.StatusCode == nil || *got.StatusCode != 403 ||
		got.FinalURL == nil || *got.FinalURL != linkFinalURL ||
		got.Reason != FailureAntiBotChallenge {
		t.Fatalf("blocked link result = %#v", got)
	}
	if got := report.Images[0].Result; got.Kind != ResultBlocked ||
		got.StatusCode == nil || *got.StatusCode != 403 ||
		got.ContentType == nil || *got.ContentType != contentType ||
		got.Reason != FailureAntiBotChallenge {
		t.Fatalf("blocked image result = %#v", got)
	}
	if got := report.Summary.Links; got.Checked != 1 || got.Broken != 0 || got.Blocked != 1 || got.Redirected != 1 {
		t.Fatalf("link summary = %#v", got)
	}
	if got := report.Summary.Images; got.Checked != 1 || got.Broken != 0 || got.Blocked != 1 || got.Invalid != 0 {
		t.Fatalf("image summary = %#v", got)
	}
	if !hasIssue(report.Issues, IssueLinkCheckBlocked) ||
		!hasIssue(report.Issues, IssueImageCheckBlocked) ||
		!hasIssue(report.Issues, IssueRedirect) ||
		hasIssue(report.Issues, IssueBrokenLink) ||
		hasIssue(report.Issues, IssueBrokenImage) ||
		hasIssue(report.Issues, IssueInvalidImageContentType) {
		t.Fatalf("issues = %#v", report.Issues)
	}
	if got := analyzeLink(links[0], true); len(got) != 1 || got[0].Code != IssueLinkCheckBlocked {
		t.Fatalf("issues with redirects ignored = %#v", got)
	}
}

func TestReachableResultsUsePublicResponseKind(t *testing.T) {
	link := publicLink(checker.LinkResult{
		URL:        "https://example.com/link",
		RequestURL: "https://example.com/link",
		Kind:       checker.ResultReachable,
		StatusCode: 200,
	}, nil)
	if link.Result.Kind != ResultResponse {
		t.Fatalf("link kind = %q, want %q", link.Result.Kind, ResultResponse)
	}

	image := publicImage(checker.ImageResult{
		Key:        "https://example.com/image.png",
		URL:        "https://example.com/image.png",
		Kind:       checker.ResultReachable,
		StatusCode: 200,
	}, nil)
	if image.Result.Kind != ResultResponse {
		t.Fatalf("image kind = %q, want %q", image.Result.Kind, ResultResponse)
	}
}

func TestPublicReportCollectionsMarshalAsArrays(t *testing.T) {
	start := testutil.ParseURL(t, "https://example.com/")
	contentType := "text/html"
	status := 200
	crawled := []crawler.CrawledPage{{
		URL:         start,
		StatusCode:  &status,
		ContentType: &contentType,
		Page: page.Page{
			ContentType:     contentType,
			Headings:        page.Headings{},
			Links:           []page.Link{},
			Images:          []page.Image{},
			ImageAltTexts:   []*string{},
			ImageReferences: []page.ImageReference{},
		},
	}}
	links := []checker.LinkResult{{
		URL:        "https://example.com/standalone-link",
		RequestURL: "https://example.com/standalone-link",
		Kind:       checker.ResultSkipped,
		Reason:     checker.ReasonUnsupportedProtocol,
	}}
	images := []checker.ImageResult{{
		Key:    "invalid:standalone-image",
		URL:    "standalone-image",
		Kind:   checker.ResultInvalid,
		Reason: checker.ReasonInvalidURL,
	}}

	report, err := buildReport(
		context.Background(),
		start,
		crawled,
		links,
		images,
		false,
		false,
		time.Now(),
	)
	if err != nil {
		t.Fatalf("buildReport() error = %v", err)
	}

	data, err := json.Marshal(report)
	if err != nil {
		t.Fatalf("json.Marshal() error = %v", err)
	}
	for _, unexpected := range []string{`"h1":null`, `"found_on":null`} {
		if strings.Contains(string(data), unexpected) {
			t.Fatalf("JSON contains %s: %s", unexpected, data)
		}
	}
}

func TestReportBuilderResourceAndAnalyzerEdges(t *testing.T) {
	t.Parallel()

	linkURL := "https://example.com/link"
	linkCases := []struct {
		result checker.LinkResult
		code   IssueCode
		count  int
	}{
		{result: checker.LinkResult{URL: linkURL, RequestURL: linkURL, Kind: checker.ResultFailed, Failure: resource.FailureConnectionFailed}, code: IssueBrokenLink, count: 1},
		{result: checker.LinkResult{URL: "mailto:test@example.com", Kind: checker.ResultSkipped, Reason: checker.ReasonUnsupportedProtocol}, count: 0},
		{result: checker.LinkResult{URL: linkURL, RequestURL: linkURL, Kind: checker.ResultReachable, StatusCode: 404}, code: IssueBrokenLink, count: 1},
	}
	for _, test := range linkCases {
		issues := analyzeLink(test.result, false)
		if len(issues) != test.count || test.count > 0 && issues[0].Code != test.code {
			t.Errorf("analyzeLink(%q) = %#v", test.result.Kind, issues)
		}
	}

	contentType := "text/plain"
	imageURL := "https://example.com/image.png"
	imageCases := []struct {
		result checker.ImageResult
		code   IssueCode
		count  int
	}{
		{result: checker.ImageResult{URL: "%", Kind: checker.ResultInvalid, Reason: checker.ReasonInvalidURL}, code: IssueInvalidImageURL, count: 1},
		{result: checker.ImageResult{URL: imageURL, Kind: checker.ResultFailed, Failure: resource.FailureConnectionFailed}, code: IssueBrokenImage, count: 1},
		{result: checker.ImageResult{URL: "data:image/png,x", Kind: checker.ResultSkipped}, count: 0},
		{result: checker.ImageResult{URL: imageURL, Kind: checker.ResultReachable, StatusCode: 404}, code: IssueBrokenImage, count: 1},
		{result: checker.ImageResult{URL: imageURL, Kind: checker.ResultReachable, StatusCode: 200}, code: IssueInvalidImageContentType, count: 1},
		{result: checker.ImageResult{URL: imageURL, Kind: checker.ResultReachable, StatusCode: 200, ContentType: &contentType}, code: IssueInvalidImageContentType, count: 1},
		{result: checker.ImageResult{URL: imageURL, Kind: checker.ResultReachable, StatusCode: 200, ContentType: new("image/png"), FinalURL: imageURL + "?v=2"}, code: IssueImageRedirect, count: 1},
	}
	for _, test := range imageCases {
		issues := analyzeImage(test.result, false)
		if len(issues) != test.count || test.count > 0 && issues[0].Code != test.code {
			t.Errorf("analyzeImage(%q, %v) = %#v", test.result.Kind, test.result.ContentType, issues)
		}
	}

	failedLink := publicLink(checker.LinkResult{URL: linkURL, Kind: checker.ResultFailed, Failure: resource.FailureRequestFailed}, nil)
	if failedLink.Result.Reason != FailureRequestFailed {
		t.Fatalf("failed public link = %#v", failedLink)
	}
	skippedLink := publicLink(checker.LinkResult{URL: linkURL, Kind: checker.ResultSkipped, Reason: checker.ReasonUnsupportedProtocol}, nil)
	if skippedLink.Result.Reason != FailureUnsupportedProtocol {
		t.Fatalf("skipped public link = %#v", skippedLink)
	}
	reachableLink := publicLink(checker.LinkResult{URL: linkURL, RequestURL: linkURL, Kind: checker.ResultReachable, StatusCode: 200}, nil)
	if reachableLink.Result.FinalURL == nil || *reachableLink.Result.FinalURL != linkURL {
		t.Fatalf("reachable public link = %#v", reachableLink)
	}

	failedImage := publicImage(checker.ImageResult{URL: imageURL, Kind: checker.ResultFailed, Failure: resource.FailureRequestFailed}, nil)
	if failedImage.Result.Reason != FailureRequestFailed {
		t.Fatalf("failed public image = %#v", failedImage)
	}
	skippedImage := publicImage(checker.ImageResult{URL: imageURL, Kind: checker.ResultSkipped, Reason: checker.ReasonUnsupportedProtocol}, nil)
	if skippedImage.Result.Reason != FailureUnsupportedProtocol {
		t.Fatalf("skipped public image = %#v", skippedImage)
	}
	reachableImage := publicImage(checker.ImageResult{URL: imageURL, Kind: checker.ResultReachable, StatusCode: 200}, nil)
	if reachableImage.Result.FinalURL == nil || *reachableImage.Result.FinalURL != imageURL {
		t.Fatalf("reachable public image = %#v", reachableImage)
	}

	parsedURL := testutil.ParseURL(t, imageURL)
	if pointer := urlPointer(parsedURL); pointer == nil || *pointer != imageURL {
		t.Fatalf("urlPointer() = %v", pointer)
	}
}

func TestBuildReportRejectsMissingResourceResults(t *testing.T) {
	t.Parallel()

	start := testutil.ParseURL(t, "https://example.com/")
	status := 200
	contentType := "text/html"
	linkURL := testutil.ParseURL(t, "https://example.com/missing-link")
	crawled := []crawler.CrawledPage{{
		URL: start, StatusCode: &status, ContentType: &contentType,
		Page: page.Page{ContentType: contentType, Links: []page.Link{{Element: page.LinkElementAnchor, URL: linkURL}}},
	}}
	if _, err := buildReport(context.Background(), start, crawled, nil, nil, false, false, time.Now()); err == nil || !strings.Contains(err.Error(), "missing check result for link") {
		t.Fatalf("missing link error = %v", err)
	}

	crawled[0].Page.Links = nil
	crawled[0].Page.ImageReferences = []page.ImageReference{{OriginalURL: "%"}}
	if _, err := buildReport(context.Background(), start, crawled, nil, nil, true, false, time.Now()); err == nil || !strings.Contains(err.Error(), "missing check result for image") {
		t.Fatalf("missing image error = %v", err)
	}
}

func TestCollectLinkOccurrencesSkipsNilURLs(t *testing.T) {
	t.Parallel()
	occurrences, err := collectLinkOccurrences(context.Background(), []crawler.CrawledPage{{
		URL:  testutil.ParseURL(t, "https://example.com"),
		Page: page.Page{Links: []page.Link{{URL: nil}}},
	}})
	if err != nil || len(occurrences) != 0 {
		t.Fatalf("collectLinkOccurrences() = %#v, %v", occurrences, err)
	}
}

func TestReportBuildingHonorsCancellationAtEveryStage(t *testing.T) {
	t.Parallel()

	start := testutil.ParseURL(t, "https://example.com/")
	linkURL := testutil.ParseURL(t, "https://example.com/link")
	imageURL := testutil.ParseURL(t, "https://example.com/image.png")
	status := 200
	contentType := "text/html"
	crawled := []crawler.CrawledPage{{
		URL: start, StatusCode: &status, ContentType: &contentType,
		Page: page.Page{
			ContentType: contentType,
			Links:       []page.Link{{Element: page.LinkElementAnchor, URL: linkURL}},
			Images:      []page.Image{{Src: imageURL}},
			ImageReferences: []page.ImageReference{{
				URL: imageURL, OriginalURL: imageURL.String(), Element: page.ImageReferenceElementImage, Attribute: page.ImageReferenceAttributeSrc,
			}},
		},
	}}
	links := []checker.LinkResult{{URL: linkURL.String(), RequestURL: linkURL.String(), FinalURL: linkURL.String(), Kind: checker.ResultReachable, StatusCode: 200}}
	images := []checker.ImageResult{{Key: imageURL.String(), URL: imageURL.String(), FinalURL: imageURL.String(), Kind: checker.ResultReachable, StatusCode: 200, ContentType: new("image/png")}}

	for cancelAt := 1; cancelAt <= 20; cancelAt++ {
		ctx := &cancelAfterChecksContext{cancelAt: cancelAt}
		_, err := buildReport(ctx, start, crawled, links, images, true, false, time.Now())
		if ctx.checks >= cancelAt && !errors.Is(err, context.Canceled) {
			t.Fatalf("cancelAt=%d checks=%d error=%v", cancelAt, ctx.checks, err)
		}
	}
}

func TestSummarizeCountsEveryResultAndIssueKind(t *testing.T) {
	t.Parallel()

	redirected := "https://example.com/final"
	report := &Report{
		Links: []Link{
			{URL: "https://example.com/one", Result: LinkResult{Kind: ResultResponse, FinalURL: &redirected}},
			{URL: "https://example.com/two", Result: LinkResult{Kind: ResultBlocked, FinalURL: &redirected}},
			{URL: "mailto:test@example.com", Result: LinkResult{Kind: ResultSkipped}},
		},
		Images: []Image{
			{URL: "https://example.com/one.png", Result: ImageResult{Kind: ResultResponse, FinalURL: &redirected}},
			{URL: "https://example.com/two.png", Result: ImageResult{Kind: ResultBlocked, FinalURL: &redirected}},
			{URL: "https://example.com/three.png", Result: ImageResult{Kind: ResultFailed}},
		},
		Issues: []Issue{
			{Code: IssueBrokenLink, Severity: SeverityError, Target: IssueTarget{URL: "link"}},
			{Code: IssueBrokenImage, Severity: SeverityWarning, Target: IssueTarget{URL: "image"}},
			{Code: IssueInvalidImageURL, Severity: SeverityInfo, Target: IssueTarget{URL: "invalid-one"}},
			{Code: IssueInvalidImageContentType, Severity: SeverityError, Target: IssueTarget{URL: "invalid-two"}},
		},
	}
	summary, err := summarize(context.Background(), report)
	if err != nil {
		t.Fatal(err)
	}
	if summary.Links.Checked != 2 || summary.Links.Blocked != 1 || summary.Links.Redirected != 2 || summary.Links.Broken != 1 ||
		summary.Images.Checked != 3 || summary.Images.Blocked != 1 || summary.Images.Redirected != 2 || summary.Images.Broken != 1 || summary.Images.Invalid != 2 ||
		summary.Issues.Error != 2 || summary.Issues.Warning != 1 || summary.Issues.Info != 1 {
		t.Fatalf("summary = %#v", summary)
	}
}

type cancelAfterChecksContext struct {
	context.Context
	cancelAt int
	checks   int
}

func (ctx *cancelAfterChecksContext) Err() error {
	ctx.checks++
	if ctx.checks >= ctx.cancelAt {
		return context.Canceled
	}
	return nil
}

func hasIssue(issues []Issue, code IssueCode) bool {
	for _, issue := range issues {
		if issue.Code == code {
			return true
		}
	}
	return false
}
