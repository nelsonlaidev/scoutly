package audit

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/crawler"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestAuditRunsEveryPhaseAndReusesResourceChecks(t *testing.T) {
	site := &countingAuditSite{requestCounts: make(map[string]int)}
	server := httptest.NewServer(site)
	defer server.Close()

	options := DefaultOptions()
	options.MaxDepth = 0
	options.MaxPages = 1
	options.Sitemaps = false
	var phases []Phase

	report, err := Audit(context.Background(), server.URL, options, func(progress Progress) error {
		if len(phases) == 0 || phases[len(phases)-1] != progress.Phase {
			phases = append(phases, progress.Phase)
		}
		return nil
	})
	if err != nil {
		t.Fatalf("Audit() error = %v", err)
	}

	wantPhases := []Phase{
		PhaseRobots,
		PhaseCrawl,
		PhaseSitemaps,
		PhaseLinks,
		PhaseImages,
		PhaseReport,
	}
	if strings.Join(phasesToStrings(phases), ",") != strings.Join(phasesToStrings(wantPhases), ",") {
		t.Fatalf("phases = %v, want %v", phases, wantPhases)
	}
	if len(report.Pages) != 1 || len(report.Links) != 1 || len(report.Images) != 1 {
		t.Fatalf("report resources = %d/%d/%d", len(report.Pages), len(report.Links), len(report.Images))
	}
	if got := report.Links[0].FoundOn[0].OriginalURL; got != "/shared.png#link" {
		t.Fatalf("original link URL = %q, want %q", got, "/shared.png#link")
	}
	site.mutex.Lock()
	sharedRequests := site.requestCounts["/shared.png"]
	site.mutex.Unlock()
	if sharedRequests != 1 {
		t.Fatalf("shared resource requests = %d, want 1", sharedRequests)
	}
}

func TestAuditReportsCrossOriginRedirect(t *testing.T) {
	destination := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/final" {
			http.NotFound(response, request)
			return
		}
		response.WriteHeader(http.StatusNoContent)
	}))
	defer destination.Close()

	source := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/":
			response.Header().Set("Content-Type", "text/html")
			_, _ = response.Write([]byte(`<html><body><a href="/redirect">redirect</a></body></html>`))
		case "/redirect":
			http.Redirect(response, request, destination.URL+"/final", http.StatusFound)
		default:
			http.NotFound(response, request)
		}
	}))
	defer source.Close()

	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.MaxDepth = 0
	options.MaxPages = 1

	report, err := Audit(context.Background(), source.URL, options, nil)
	if err != nil {
		t.Fatalf("Audit() error = %v", err)
	}

	redirectURL := source.URL + "/redirect"
	finalURL := destination.URL + "/final"
	if len(report.Links) != 1 {
		t.Fatalf("links = %d, want 1", len(report.Links))
	}
	if got := report.Links[0]; got.URL != redirectURL || got.Result.FinalURL == nil || *got.Result.FinalURL != finalURL {
		t.Fatalf("redirected link = %#v, want %s -> %s", got, redirectURL, finalURL)
	}
	if report.Summary.Links.Redirected != 1 {
		t.Fatalf("redirected summary = %d, want 1", report.Summary.Links.Redirected)
	}

	redirectIssues := 0
	for _, issue := range report.Issues {
		if issue.Code != IssueRedirect {
			continue
		}
		redirectIssues++
		if issue.Target.Type != TargetLink || issue.Target.URL != redirectURL {
			t.Errorf("redirect issue target = %#v, want %s", issue.Target, redirectURL)
		}
		wantMessage := "Link redirected: " + redirectURL + " -> " + finalURL
		if issue.Message != wantMessage {
			t.Errorf("redirect issue message = %q, want %q", issue.Message, wantMessage)
		}
	}
	if redirectIssues != 1 {
		t.Fatalf("redirect issues = %d, want 1", redirectIssues)
	}
}

func TestAuditLoadsCanonicalOriginRobotsAndCrawlsFinalScope(t *testing.T) {
	var childRequests atomic.Int32
	var blockedRequests atomic.Int32
	var destinationURL string
	destination := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/robots.txt":
			_, _ = io.WriteString(
				response,
				"User-agent: *\nDisallow: /blocked\nSitemap: "+destinationURL+"/custom-sitemap.xml\n",
			)
		case "/":
			response.Header().Set("Content-Type", "text/html")
			_, _ = io.WriteString(response, `<a href="/child">child</a><a href="/blocked">blocked</a>`)
		case "/child":
			childRequests.Add(1)
			response.Header().Set("Content-Type", "text/html")
			_, _ = io.WriteString(response, `<title>Child</title>`)
		case "/blocked":
			blockedRequests.Add(1)
			response.Header().Set("Content-Type", "text/html")
		case "/custom-sitemap.xml":
			_, _ = io.WriteString(response, testutil.URLSet(destinationURL+"/sitemap-only"))
		case "/sitemap-only":
			response.Header().Set("Content-Type", "text/html")
			_, _ = io.WriteString(response, `<title>Sitemap only</title>`)
		default:
			http.NotFound(response, request)
		}
	}))
	defer destination.Close()
	destinationURL = destination.URL

	source := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/robots.txt" {
			http.NotFound(response, request)
			return
		}
		http.Redirect(response, request, destination.URL, http.StatusFound)
	}))
	defer source.Close()

	options := DefaultOptions()
	options.MaxDepth = 1
	options.MaxPages = 3
	options.Images = false
	report, err := Audit(context.Background(), source.URL, options, nil)
	if err != nil {
		t.Fatal(err)
	}
	if report.Summary.Pages != 3 {
		t.Fatalf("pages = %d, want 3", report.Summary.Pages)
	}
	// The child is fetched once by the crawler and once by the link checker.
	if got := childRequests.Load(); got != 2 {
		t.Fatalf("child requests = %d, want 2", got)
	}
	// The blocked URL is checked as a link but is not crawled as a page.
	if got := blockedRequests.Load(); got != 1 {
		t.Fatalf("blocked requests = %d, want 1", got)
	}
}

func TestAuditReportsAntiBotChallengesAsBlocked(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/":
			response.Header().Set("Content-Type", "text/html")
			_, _ = response.Write([]byte(`<html><body><a href="/blocked">blocked</a></body></html>`))
		case "/blocked":
			response.Header().Set("Cf-Mitigated", "challenge")
			response.Header().Set("Content-Type", "text/html")
			response.WriteHeader(http.StatusForbidden)
		default:
			http.NotFound(response, request)
		}
	}))
	defer server.Close()

	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.MaxDepth = 0
	options.MaxPages = 1

	report, err := Audit(context.Background(), server.URL, options, nil)
	if err != nil {
		t.Fatalf("Audit() error = %v", err)
	}
	if len(report.Links) != 1 {
		t.Fatalf("links = %d, want 1", len(report.Links))
	}
	result := report.Links[0].Result
	if result.Kind != ResultBlocked ||
		result.StatusCode == nil || *result.StatusCode != http.StatusForbidden ||
		result.Reason != FailureAntiBotChallenge {
		t.Fatalf("blocked result = %#v", result)
	}
	if got := report.Summary.Links; got.Checked != 1 || got.Blocked != 1 || got.Broken != 0 {
		t.Fatalf("link summary = %#v", got)
	}
	if !hasIssue(report.Issues, IssueLinkCheckBlocked) || hasIssue(report.Issues, IssueBrokenLink) {
		t.Fatalf("issues = %#v", report.Issues)
	}
}

func TestAuditCancellationReturnsNoPartialReport(t *testing.T) {
	site := &blockingAuditSite{started: make(chan struct{})}
	server := httptest.NewServer(site)
	defer server.Close()

	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		<-site.started
		cancel()
	}()

	report, err := Audit(ctx, server.URL, options, nil)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Audit() error = %v, want context.Canceled", err)
	}
	if report != nil {
		t.Fatalf("report = %#v, want nil", report)
	}
}

func TestAuditPropagatesProgressErrorWithoutCheckingResources(t *testing.T) {
	server := httptest.NewServer(auditLinkSite{})
	defer server.Close()

	want := errors.New("observer failed")
	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.MaxDepth = 0
	options.MaxPages = 1

	report, err := Audit(context.Background(), server.URL, options, func(progress Progress) error {
		if progress.Phase == PhaseLinks {
			return want
		}
		return nil
	})
	if !errors.Is(err, want) {
		t.Fatalf("Audit() error = %v, want observer error", err)
	}
	if report != nil {
		t.Fatalf("report = %#v, want nil", report)
	}
}

func TestAuditCancellationDuringReportPhaseReturnsNoReport(t *testing.T) {
	server := httptest.NewServer(auditSite{})
	defer server.Close()

	ctx, cancel := context.WithCancel(context.Background())
	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.MaxDepth = 0
	options.MaxPages = 1

	report, err := Audit(ctx, server.URL, options, func(progress Progress) error {
		if progress.Phase == PhaseReport {
			cancel()
		}
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Audit() error = %v, want context.Canceled", err)
	}
	if report != nil {
		t.Fatalf("report = %#v, want nil", report)
	}
}

func TestAuditRejectsInvalidInputBeforeRequests(t *testing.T) {
	//nolint:staticcheck // Nil context intentionally verifies the public boundary guard.
	if report, err := Audit(nil, "https://example.com", DefaultOptions(), nil); err == nil || report != nil {
		t.Fatalf("Audit(nil) = %#v, %v", report, err)
	}
	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if report, err := Audit(canceled, "https://example.com", DefaultOptions(), nil); !errors.Is(err, context.Canceled) || report != nil {
		t.Fatalf("Audit(canceled) = %#v, %v", report, err)
	}

	options := DefaultOptions()
	options.MaxPages = 0
	if report, err := Audit(context.Background(), "https://example.com", options, nil); err == nil || report != nil {
		t.Fatalf("Audit() = %#v, %v", report, err)
	}

	options = DefaultOptions()
	if report, err := Audit(context.Background(), "ftp://example.com", options, nil); err == nil || report != nil {
		t.Fatalf("Audit() = %#v, %v", report, err)
	}
}

func TestAuditPropagatesErrorsFromEveryProgressPhase(t *testing.T) {
	server := httptest.NewServer(auditSite{})
	defer server.Close()

	for _, test := range []struct {
		name   string
		phase  Phase
		images bool
	}{
		{name: "robots", phase: PhaseRobots},
		{name: "crawl", phase: PhaseCrawl},
		{name: "sitemaps", phase: PhaseSitemaps},
		{name: "links", phase: PhaseLinks},
		{name: "images enabled", phase: PhaseImages, images: true},
		{name: "images disabled", phase: PhaseImages, images: false},
		{name: "report", phase: PhaseReport},
	} {
		t.Run(test.name, func(t *testing.T) {
			want := errors.New("progress failed at " + string(test.phase))
			options := DefaultOptions()
			options.RespectRobots = false
			options.Sitemaps = false
			options.Images = test.images
			options.MaxDepth = 0
			options.MaxPages = 1
			report, err := Audit(context.Background(), server.URL, options, func(progress Progress) error {
				if progress.Phase == test.phase {
					return want
				}
				return nil
			})
			if !errors.Is(err, want) || report != nil {
				t.Fatalf("Audit() = %#v, %v, want progress error", report, err)
			}
		})
	}
}

func TestAuditCollectionHelpersHandleNilDuplicatesAndCancellation(t *testing.T) {
	t.Parallel()

	link := testutil.ParseURL(t, "https://example.com/link#one")
	duplicate := testutil.ParseURL(t, "https://example.com/link#two")
	pages := []crawler.CrawledPage{{Page: page.Page{
		Links: []page.Link{{URL: nil}, {URL: link}, {URL: duplicate}},
		ImageReferences: []page.ImageReference{{
			URL: link, OriginalURL: link.String(),
		}},
	}}}
	links, err := collectLinkURLs(context.Background(), pages)
	if err != nil || len(links) != 1 || links[0].Fragment != "" {
		t.Fatalf("collectLinkURLs() = %#v, %v", links, err)
	}
	references, err := collectImageReferences(context.Background(), pages)
	if err != nil || len(references) != 1 {
		t.Fatalf("collectImageReferences() = %#v, %v", references, err)
	}
	if count, err := countUniqueImages(context.Background(), append(references, references[0])); err != nil || count != 1 {
		t.Fatalf("countUniqueImages() = %d, %v", count, err)
	}

	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := collectLinkURLs(canceled, pages); !errors.Is(err, context.Canceled) {
		t.Errorf("collectLinkURLs(canceled) error = %v", err)
	}
	if _, err := collectImageReferences(canceled, pages); !errors.Is(err, context.Canceled) {
		t.Errorf("collectImageReferences(canceled) error = %v", err)
	}
	if _, err := countUniqueImages(canceled, references); !errors.Is(err, context.Canceled) {
		t.Errorf("countUniqueImages(canceled) error = %v", err)
	}
}

func phasesToStrings(phases []Phase) []string {
	result := make([]string, len(phases))
	for index, phase := range phases {
		result[index] = string(phase)
	}
	return result
}

func TestAuditHonorsRequestTimeout(t *testing.T) {
	server := httptest.NewServer(&blockingAuditSite{started: make(chan struct{})})
	defer server.Close()

	options := DefaultOptions()
	options.RespectRobots = false
	options.Sitemaps = false
	options.Images = false
	options.MaxPages = 1
	options.Timeout = 25 * time.Millisecond

	report, err := Audit(context.Background(), server.URL, options, nil)
	if err != nil {
		t.Fatalf("Audit() error = %v", err)
	}
	if report.Summary.Pages != 1 || !hasIssue(report.Issues, IssuePageCrawlFailed) {
		t.Fatalf("report = %#v", report)
	}
}

func TestAuditReturnsRobotsRequestTimeoutWithoutReport(t *testing.T) {
	server := httptest.NewServer(&blockingAuditSite{started: make(chan struct{})})
	defer server.Close()

	options := DefaultOptions()
	options.Sitemaps = false
	options.Images = false
	options.Timeout = 25 * time.Millisecond

	report, err := Audit(context.Background(), server.URL, options, nil)
	if err == nil || !strings.Contains(err.Error(), "fetch robots.txt") {
		t.Fatalf("Audit() error = %v, want robots.txt fetch error", err)
	}
	if report != nil {
		t.Fatalf("Audit() report = %#v, want nil", report)
	}
}

type auditSite struct{}

func (auditSite) ServeHTTP(response http.ResponseWriter, _ *http.Request) {
	response.Header().Set("Content-Type", "text/html")
	_, _ = response.Write([]byte("<html><head><title>Audit</title></head><body></body></html>"))
}

type auditLinkSite struct{}

func (auditLinkSite) ServeHTTP(response http.ResponseWriter, _ *http.Request) {
	response.Header().Set("Content-Type", "text/html")
	_, _ = response.Write([]byte(`<html><body><a href="/next">next</a></body></html>`))
}

type countingAuditSite struct {
	mutex         sync.Mutex
	requestCounts map[string]int
}

func (site *countingAuditSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	site.mutex.Lock()
	site.requestCounts[request.URL.Path]++
	site.mutex.Unlock()

	switch request.URL.Path {
	case "/robots.txt":
		http.NotFound(response, request)
	case "/":
		response.Header().Set("Content-Type", "text/html")
		_, _ = response.Write([]byte(
			`<html><head><title>Short</title></head><body>` +
				`<a href="/shared.png#link">shared</a>` +
				`<img src="/shared.png#image" alt="shared"></body></html>`,
		))
	case "/shared.png":
		response.Header().Set("Content-Type", "image/png")
		_, _ = response.Write([]byte("png"))
	default:
		http.NotFound(response, request)
	}
}

type blockingAuditSite struct {
	started chan struct{}
	once    sync.Once
}

func (site *blockingAuditSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	site.once.Do(func() { close(site.started) })
	<-request.Context().Done()
}
