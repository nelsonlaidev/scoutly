package crawler

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestCrawlPreservesBreadthFirstOrderAndDepth(t *testing.T) {
	t.Parallel()

	site := &crawlerSite{responses: map[string]crawlerResponse{
		"/": {
			contentType: "text/html",
			body: `<a href="/one">one</a><a href="/two">two</a>` +
				`<a href="https://outside.example/a">outside</a>`,
		},
		"/one": {contentType: "text/html", body: `<a href="/deep">deep</a>`},
		"/two": {contentType: "text/html", body: `<title>Two</title>`},
	}}
	server := httptest.NewServer(site)
	defer server.Close()

	var discovered []string
	pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
		MaxDepth:            1,
		MaxPages:            10,
		MaxSitemapDocuments: 1,
		Concurrency:         2,
		OnPageDiscovered: func(currentURL *url.URL) error {
			discovered = append(discovered, currentURL.String())
			return nil
		},
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}

	want := []string{server.URL + "/", server.URL + "/one", server.URL + "/two"}
	if got := crawledURLStrings(pages); !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %v, want %v", got, want)
	}
	if !reflect.DeepEqual(discovered, want) {
		t.Fatalf("discovered = %v, want %v", discovered, want)
	}
}

func TestCrawlUsesInitialRedirectDestinationAsScope(t *testing.T) {
	t.Parallel()

	destination := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Type", "text/html")
		if request.URL.Path == "/" {
			_, _ = io.WriteString(response, `<a href="/child">child</a>`)
			return
		}
		_, _ = io.WriteString(response, `<title>Child</title>`)
	}))
	defer destination.Close()

	source := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		http.Redirect(response, request, destination.URL, http.StatusFound)
	}))
	defer source.Close()

	pages, err := Crawl(
		context.Background(),
		testutil.ParseURL(t, source.URL),
		testutil.NewFetcher(t, fetcher.Options{MaxRedirects: 1}),
		Options{MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1},
	)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := crawledURLStrings(pages), []string{source.URL + "/", destination.URL + "/child"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %v, want %v", got, want)
	}
	if pages[0].FinalURL == nil || pages[0].FinalURL.String() != destination.URL+"/" {
		t.Fatalf("initial final URL = %v", pages[0].FinalURL)
	}
}

func TestCrawlAppliesRedirectPolicyBeforeFetchingDestination(t *testing.T) {
	t.Parallel()

	var privateRequests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/":
			response.Header().Set("Content-Type", "text/html")
			_, _ = io.WriteString(response, `<a href="/go">go</a>`)
		case "/go":
			http.Redirect(response, request, "/private", http.StatusFound)
		case "/private":
			privateRequests.Add(1)
			response.Header().Set("Content-Type", "text/html")
		}
	}))
	defer server.Close()

	pages, err := Crawl(
		context.Background(),
		testutil.ParseURL(t, server.URL),
		testutil.NewFetcher(t, fetcher.Options{MaxRedirects: 1}),
		Options{
			MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1,
			RedirectPolicy: func(_ context.Context, destination *url.URL) error {
				if destination.Path == "/private" {
					return errors.New("robots.txt disallows redirect")
				}
				return nil
			},
		},
	)
	if err != nil {
		t.Fatal(err)
	}
	if len(pages) != 2 || pages[1].StatusCode != nil {
		t.Fatalf("pages = %#v", pages)
	}
	if got := privateRequests.Load(); got != 0 {
		t.Fatalf("private requests = %d, want 0", got)
	}
}

func TestCrawlRecordsFailuresAndSkipsNonHTMLParsing(t *testing.T) {
	t.Parallel()

	site := &crawlerSite{
		responses: map[string]crawlerResponse{
			"/":      {body: `<a href="/failed">failed</a><a href="/image">image</a>`},
			"/image": {contentType: "image/png", body: `<a href="/not-discovered">not parsed</a>`},
		},
		failPaths: map[string]bool{"/failed": true},
	}
	server := httptest.NewServer(site)
	defer server.Close()

	pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
		MaxDepth:            2,
		MaxPages:            10,
		MaxSitemapDocuments: 1,
		Concurrency:         2,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	if len(pages) != 3 {
		t.Fatalf("pages = %d, want 3", len(pages))
	}
	if pages[1].StatusCode != nil {
		t.Fatalf("failed status = %v, want nil", pages[1].StatusCode)
	}
	if pages[2].ContentType == nil || *pages[2].ContentType != "image/png" {
		t.Fatalf("image content type = %v", pages[2].ContentType)
	}
	if len(pages[2].Page.Links) != 0 {
		t.Fatalf("non-HTML links = %v", pages[2].Page.Links)
	}
}

func TestCrawlRespectsFragmentsAndRobotsPolicy(t *testing.T) {
	t.Parallel()

	site := &crawlerSite{responses: map[string]crawlerResponse{
		"/": {
			body: `<a href="/page#one">one</a><a href="/page#two">two</a>` +
				`<a href="/blocked">blocked</a>`,
		},
		"/page": {},
	}}
	server := httptest.NewServer(site)
	defer server.Close()

	pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
		Allowed: func(candidate *url.URL) bool {
			return candidate.Path != "/blocked"
		},
		KeepFragments:       true,
		MaxDepth:            1,
		MaxPages:            10,
		MaxSitemapDocuments: 1,
		Concurrency:         2,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	if len(pages) != 3 {
		t.Fatalf("pages = %d, want 3", len(pages))
	}
}

func TestCrawlLimitsDiscoveredQueueToPageBudget(t *testing.T) {
	t.Parallel()

	site := &crawlerSite{responses: map[string]crawlerResponse{
		"/": {
			body: `<a href="/one">one</a><a href="/two">two</a><a href="/three">three</a>`,
		},
		"/one": {},
	}}
	server := httptest.NewServer(site)
	defer server.Close()
	discovered := make([]string, 0)

	pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
		MaxDepth:            1,
		MaxPages:            2,
		MaxSitemapDocuments: 1,
		Concurrency:         1,
		OnPageDiscovered: func(currentURL *url.URL) error {
			discovered = append(discovered, currentURL.String())
			return nil
		},
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	want := []string{server.URL + "/", server.URL + "/one"}
	if got := crawledURLStrings(pages); !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %v, want %v", got, want)
	}
	if !reflect.DeepEqual(discovered, want) {
		t.Fatalf("discovered = %v, want %v", discovered, want)
	}
}

func TestCrawlAddsSitemapLeavesWithinRemainingBudget(t *testing.T) {
	t.Parallel()

	site := &crawlerSite{responses: map[string]crawlerResponse{
		"/":             {body: `<a href="/child">child</a>`},
		"/child":        {},
		"/sitemap-only": {},
	}}
	server := httptest.NewServer(site)
	defer server.Close()
	site.mutex.Lock()
	site.responses["/sitemap.xml"] = crawlerResponse{
		contentType: "application/xml",
		body: testutil.URLSet(
			server.URL,
			server.URL+"/sitemap-only",
			server.URL+"/overflow",
		),
	}
	site.mutex.Unlock()
	sitemapURL := testutil.ParseURL(t, server.URL+"/sitemap.xml")
	fetchedSitemaps := make([]string, 0, 1)

	pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
		SitemapURLs:         []*url.URL{sitemapURL},
		ScanSitemaps:        true,
		MaxDepth:            1,
		MaxPages:            3,
		MaxSitemapDocuments: 1,
		Concurrency:         2,
		OnSitemapFetched: func(currentURL *url.URL) error {
			fetchedSitemaps = append(fetchedSitemaps, currentURL.String())
			return nil
		},
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}

	want := []string{server.URL + "/", server.URL + "/child", server.URL + "/sitemap-only"}
	if got := crawledURLStrings(pages); !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %v, want %v", got, want)
	}
	if pages[2].Depth != -1 {
		t.Fatalf("sitemap page depth = %d, want -1", pages[2].Depth)
	}
	if wantFetched := []string{sitemapURL.String()}; !reflect.DeepEqual(fetchedSitemaps, wantFetched) {
		t.Fatalf("fetched sitemaps = %v", fetchedSitemaps)
	}
}

func TestCrawlReturnsProgressCallbackErrorUnchanged(t *testing.T) {
	t.Parallel()

	want := &progressCallbackError{}
	_, err := Crawl(context.Background(), testutil.ParseURL(t, "https://example.com"), testutil.NewFetcher(t, fetcher.Options{}), Options{
		MaxDepth:            0,
		MaxPages:            1,
		MaxSitemapDocuments: 1,
		Concurrency:         1,
		OnPageDiscovered: func(*url.URL) error {
			return want
		},
	})
	var got *progressCallbackError
	if !errors.As(err, &got) || got != want || reflect.TypeOf(err) != reflect.TypeOf(want) {
		t.Fatalf("Crawl() error = %v, want exact callback error", err)
	}
}

func TestCrawlHandlesPolicyAndLifecycleCallbacks(t *testing.T) {
	t.Parallel()

	startURL := testutil.ParseURL(t, "https://example.com")
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	baseOptions := Options{MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1}

	t.Run("disallowed start still starts sitemap phase", func(t *testing.T) {
		t.Parallel()
		started := false
		options := baseOptions
		options.Allowed = func(*url.URL) bool { return false }
		options.OnSitemapPhaseStarted = func() error {
			started = true
			return nil
		}
		pages, err := Crawl(context.Background(), startURL, httpFetcher, options)
		if err != nil || len(pages) != 0 || !started {
			t.Fatalf("Crawl() = %#v, %v, sitemap started=%t", pages, err, started)
		}
	})

	t.Run("disallowed start propagates sitemap phase error", func(t *testing.T) {
		t.Parallel()
		want := errors.New("start sitemap phase")
		options := baseOptions
		options.Allowed = func(*url.URL) bool { return false }
		options.OnSitemapPhaseStarted = func() error { return want }
		if _, err := Crawl(context.Background(), startURL, httpFetcher, options); !errors.Is(err, want) {
			t.Fatalf("Crawl() error = %v, want %v", err, want)
		}
	})

	t.Run("canceled context", func(t *testing.T) {
		t.Parallel()
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		if _, err := Crawl(ctx, startURL, httpFetcher, baseOptions); !errors.Is(err, context.Canceled) {
			t.Fatalf("Crawl() error = %v, want context.Canceled", err)
		}
	})

	t.Run("post-crawl sitemap phase error", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(&crawlerSite{responses: map[string]crawlerResponse{"/": {}}})
		defer server.Close()
		want := errors.New("finish crawl phase")
		options := baseOptions
		options.OnSitemapPhaseStarted = func() error { return want }
		if _, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), httpFetcher, options); !errors.Is(err, want) {
			t.Fatalf("Crawl() error = %v, want %v", err, want)
		}
	})

	t.Run("crawled callback error", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(&crawlerSite{responses: map[string]crawlerResponse{"/": {}}})
		defer server.Close()
		want := errors.New("page crawled")
		options := baseOptions
		options.OnPageCrawled = func(*url.URL) error { return want }
		if _, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), httpFetcher, options); !errors.Is(err, want) {
			t.Fatalf("Crawl() error = %v, want %v", err, want)
		}
	})

	t.Run("discovered child callback error", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(&crawlerSite{responses: map[string]crawlerResponse{
			"/": {body: `<a href="/child">child</a>`},
		}})
		defer server.Close()
		want := errors.New("child discovered")
		calls := 0
		options := baseOptions
		options.OnPageDiscovered = func(*url.URL) error {
			calls++
			if calls == 2 {
				return want
			}
			return nil
		}
		if _, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), httpFetcher, options); !errors.Is(err, want) {
			t.Fatalf("Crawl() error = %v, want %v", err, want)
		}
	})
}

func TestCrawlUsesDefaultSitemapAndPropagatesSitemapCallbacks(t *testing.T) {
	t.Parallel()

	t.Run("default sitemap", func(t *testing.T) {
		t.Parallel()
		site := &crawlerSite{responses: map[string]crawlerResponse{"/": {}, "/leaf": {}}}
		server := httptest.NewServer(site)
		defer server.Close()
		site.mutex.Lock()
		site.responses["/sitemap.xml"] = crawlerResponse{body: testutil.URLSet(server.URL + "/leaf")}
		site.mutex.Unlock()

		pages, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
			ScanSitemaps: true, MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1,
		})
		if err != nil {
			t.Fatal(err)
		}
		if got, want := crawledURLStrings(pages), []string{server.URL + "/", server.URL + "/leaf"}; !reflect.DeepEqual(got, want) {
			t.Fatalf("pages = %#v, want %#v", got, want)
		}
	})

	t.Run("sitemap fetched callback error", func(t *testing.T) {
		t.Parallel()
		site := &crawlerSite{responses: map[string]crawlerResponse{"/": {}, "/sitemap.xml": {body: testutil.URLSet("https://example.com/leaf")}}}
		server := httptest.NewServer(site)
		defer server.Close()
		want := errors.New("sitemap fetched")
		_, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
			ScanSitemaps: true, MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1,
			OnSitemapFetched: func(*url.URL) error { return want },
		})
		if !errors.Is(err, want) {
			t.Fatalf("Crawl() error = %v, want %v", err, want)
		}
	})

	t.Run("canceled sitemap traversal", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(&crawlerSite{responses: map[string]crawlerResponse{"/": {}}})
		defer server.Close()
		ctx, cancel := context.WithCancel(context.Background())
		calls := 0
		_, err := Crawl(ctx, testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
			ScanSitemaps: true, MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1,
			Allowed: func(*url.URL) bool {
				calls++
				if calls == 2 {
					cancel()
				}
				return true
			},
		})
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("Crawl() error = %v, want context.Canceled", err)
		}
	})

	for _, test := range []struct {
		name     string
		callback func(*url.URL) error
	}{
		{
			name: "sitemap page discovered error",
			callback: func(current *url.URL) error {
				if current.Path == "/leaf" {
					return errors.New("sitemap page discovered")
				}
				return nil
			},
		},
		{
			name: "sitemap page crawled error",
			callback: func(current *url.URL) error {
				if current.Path == "/leaf" {
					return errors.New("sitemap page crawled")
				}
				return nil
			},
		},
	} {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			site := &crawlerSite{responses: map[string]crawlerResponse{"/": {}, "/leaf": {}}}
			server := httptest.NewServer(site)
			defer server.Close()
			site.mutex.Lock()
			site.responses["/sitemap.xml"] = crawlerResponse{body: testutil.URLSet(server.URL + "/leaf")}
			site.mutex.Unlock()
			options := Options{ScanSitemaps: true, MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1}
			if strings.Contains(test.name, "discovered") {
				options.OnPageDiscovered = test.callback
			} else {
				options.OnPageCrawled = test.callback
			}
			if _, err := Crawl(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), options); err == nil {
				t.Fatal("Crawl() error = nil")
			}
		})
	}

	t.Run("canceled before sitemap page batch", func(t *testing.T) {
		t.Parallel()
		site := &crawlerSite{responses: map[string]crawlerResponse{"/": {}, "/leaf": {}}}
		server := httptest.NewServer(site)
		defer server.Close()
		site.mutex.Lock()
		site.responses["/sitemap.xml"] = crawlerResponse{body: testutil.URLSet(server.URL + "/leaf")}
		site.mutex.Unlock()
		ctx, cancel := context.WithCancel(context.Background())
		_, err := Crawl(ctx, testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{
			ScanSitemaps: true, MaxDepth: 1, MaxPages: 2, MaxSitemapDocuments: 1, Concurrency: 1,
			OnPageDiscovered: func(current *url.URL) error {
				if current.Path == "/leaf" {
					cancel()
				}
				return nil
			},
		})
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("Crawl() error = %v, want context.Canceled", err)
		}
	})
}

func TestCrawlPageRejectsOversizedHTMLAndCancellation(t *testing.T) {
	t.Parallel()

	t.Run("oversized HTML", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
			response.Header().Set("Content-Type", "text/html")
			_, _ = response.Write([]byte(strings.Repeat("x", maxHTMLBytes+1)))
		}))
		defer server.Close()
		queued := createQueuedPage(testutil.ParseURL(t, server.URL), 0, false)
		result, err := crawlPage(
			context.Background(),
			queued,
			testutil.NewFetcher(t, fetcher.Options{}),
			Options{},
		)
		if err != nil {
			t.Fatal(err)
		}
		if result.StatusCode != nil || result.ContentType != nil || len(result.Page.Links) != 0 {
			t.Fatalf("oversized result = %#v", result)
		}
	})

	t.Run("canceled", func(t *testing.T) {
		t.Parallel()
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		queued := createQueuedPage(testutil.ParseURL(t, "https://example.com"), 0, false)
		if _, err := crawlPage(ctx, queued, testutil.NewFetcher(t, fetcher.Options{}), Options{}); !errors.Is(err, context.Canceled) {
			t.Fatalf("crawlPage() error = %v", err)
		}
	})
}

func TestCrawlBatchAndLinkEligibilityEdges(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	batch := []queuedPage{createQueuedPage(testutil.ParseURL(t, "https://example.com"), 0, false)}
	if _, err := crawlBatch(ctx, batch, testutil.NewFetcher(t, fetcher.Options{}), Options{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("crawlBatch() error = %v", err)
	}

	start := testutil.ParseURL(t, "https://example.com")
	for _, link := range []page.Link{
		{},
		{Element: page.LinkElementVideo, URL: start},
		{Element: page.LinkElementAnchor, URL: testutil.ParseURL(t, "mailto:test@example.com")},
		{Element: page.LinkElementIframe, URL: testutil.ParseURL(t, "https://other.example")},
	} {
		if isCrawlable(link, start) {
			t.Fatalf("isCrawlable(%#v) = true", link)
		}
	}
	if !isCrawlable(page.Link{Element: page.LinkElementIframe, URL: testutil.ParseURL(t, "https://example.com/frame")}, start) {
		t.Fatal("same-host iframe is not crawlable")
	}
}

func TestCrawlBatchPropagatesCancellationDuringFetch(t *testing.T) {
	t.Parallel()

	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
		close(started)
		<-request.Context().Done()
	}))
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		_, err := crawlBatch(
			ctx,
			[]queuedPage{createQueuedPage(testutil.ParseURL(t, server.URL), 0, false)},
			testutil.NewFetcher(t, fetcher.Options{}),
			Options{},
		)
		result <- err
	}()
	<-started
	cancel()
	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatalf("crawlBatch() error = %v", err)
	}
}

func TestCrawlValidationErrors(t *testing.T) {
	t.Parallel()

	startURL := testutil.ParseURL(t, "https://example.com")
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	tests := []struct {
		name    string
		ctx     context.Context
		start   *url.URL
		fetcher *fetcher.Fetcher
		options Options
		want    error
	}{
		{name: "nil context", ctx: nil, start: startURL, fetcher: httpFetcher, options: Options{MaxPages: 1, Concurrency: 1}, want: ErrNilContext},
		{name: "nil start URL", ctx: context.Background(), start: nil, fetcher: httpFetcher, options: Options{MaxPages: 1, Concurrency: 1}, want: ErrNilStartURL},
		{name: "nil fetcher", ctx: context.Background(), start: startURL, fetcher: nil, options: Options{MaxPages: 1, Concurrency: 1}, want: ErrNilFetcher},
		{name: "zero max pages", ctx: context.Background(), start: startURL, fetcher: httpFetcher, options: Options{Concurrency: 1}, want: ErrMaxPagesNotPositive},
		{name: "negative max depth", ctx: context.Background(), start: startURL, fetcher: httpFetcher, options: Options{MaxPages: 1, MaxDepth: -1, Concurrency: 1}, want: ErrNegativeMaxDepth},
		{name: "zero concurrency", ctx: context.Background(), start: startURL, fetcher: httpFetcher, options: Options{MaxPages: 1}, want: ErrInvalidConcurrency},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			_, err := Crawl(test.ctx, test.start, test.fetcher, test.options)
			if !errors.Is(err, test.want) {
				t.Fatalf("Crawl() error = %v, want %v", err, test.want)
			}
		})
	}
}

type crawlerResponse struct {
	status      int
	contentType string
	body        string
}

type crawlerSite struct {
	mutex     sync.Mutex
	responses map[string]crawlerResponse
	failPaths map[string]bool
	requests  []string
}

func (site *crawlerSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	site.mutex.Lock()
	site.requests = append(site.requests, request.URL.Path)
	entry, exists := site.responses[request.URL.Path]
	fail := site.failPaths[request.URL.Path]
	site.mutex.Unlock()

	if fail {
		hijacker := response.(http.Hijacker)
		connection, _, err := hijacker.Hijack()
		if err == nil {
			_ = connection.Close()
		}
		return
	}
	if !exists {
		http.NotFound(response, request)
		return
	}
	if entry.contentType != "" {
		response.Header().Set("Content-Type", entry.contentType)
	}
	status := entry.status
	if status == 0 {
		status = http.StatusOK
	}
	response.WriteHeader(status)
	_, _ = response.Write([]byte(entry.body))
}

type progressCallbackError struct{}

func (*progressCallbackError) Error() string {
	return "progress callback failed"
}

func crawledURLStrings(pages []CrawledPage) []string {
	result := make([]string, 0, len(pages))
	for _, crawled := range pages {
		result = append(result, crawled.URL.String())
	}
	return result
}
