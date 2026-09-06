package robots

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestGetUsesSelectedUserAgentAndOnlyRestrictsOrigin(t *testing.T) {
	t.Parallel()

	site := &robotsSite{
		body: strings.Join([]string{
			"User-agent: *",
			"Disallow: /public",
			"",
			"User-agent: scoutly",
			"Disallow: /private",
			"Allow: /private/open",
		}, "\n"),
	}
	server := httptest.NewServer(site)
	defer server.Close()

	result, err := Get(
		context.Background(),
		testutil.ParseURL(t, server.URL+"/start"),
		testutil.NewFetcher(t, fetcher.Options{}),
		Options{
			Respect:   true,
			UserAgent: "scoutly/1.0",
		},
	)
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}

	tests := []struct {
		target  string
		allowed bool
	}{
		{target: server.URL + "/private", allowed: false},
		{target: server.URL + "/private/open", allowed: true},
		{target: server.URL + "/public", allowed: true},
		{target: "https://other.example/private", allowed: true},
	}
	for _, test := range tests {
		if got := result.IsAllowed(testutil.ParseURL(t, test.target)); got != test.allowed {
			t.Errorf("IsAllowed(%q) = %t, want %t", test.target, got, test.allowed)
		}
	}
	if result.IsAllowed(nil) {
		t.Error("IsAllowed(nil) = true")
	}

	site.mutex.Lock()
	gotPath := site.requestPath
	site.mutex.Unlock()
	if gotPath != "/robots.txt" {
		t.Errorf("requested path = %q, want /robots.txt", gotPath)
	}
}

func TestCacheCoalescesOriginLookupsAndClonesResults(t *testing.T) {
	t.Parallel()

	var requests atomic.Int32
	started := make(chan struct{})
	release := make(chan struct{})
	var baseURL string
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		if requests.Add(1) == 1 {
			close(started)
		}
		<-release
		_, _ = fmt.Fprintf(
			response,
			"User-agent: *\nDisallow: /private\nSitemap: %s/sitemap.xml\n",
			baseURL,
		)
	}))
	defer server.Close()
	baseURL = server.URL

	cache := NewCache()
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	firstOriginURL := testutil.ParseURL(t, server.URL+"/one")
	secondOriginURL := testutil.ParseURL(t, server.URL+"/two")
	type outcome struct {
		result Result
		err    error
	}
	outcomes := make(chan outcome, 2)
	load := func(originURL *url.URL) {
		result, err := cache.GetForOrigin(
			context.Background(),
			originURL,
			httpFetcher,
			Options{Respect: true},
		)
		outcomes <- outcome{result: result, err: err}
	}
	go load(firstOriginURL)
	<-started
	go load(secondOriginURL)
	close(release)

	first, second := <-outcomes, <-outcomes
	for _, result := range []outcome{first, second} {
		if result.err != nil || result.result.IsAllowed(testutil.ParseURL(t, server.URL+"/private")) {
			t.Fatalf("cached result = %#v, %v", result.result, result.err)
		}
	}
	if got := requests.Load(); got != 1 {
		t.Fatalf("robots requests = %d, want 1", got)
	}

	first.result.SitemapURLs[0].Path = "/mutated.xml"
	cached, ok := cache.Lookup(testutil.ParseURL(t, server.URL+"/page"))
	if !ok || cached.SitemapURLs[0].Path != "/sitemap.xml" {
		t.Fatalf("Lookup() = %#v, %t", cached, ok)
	}
}

func TestGetCollectsDistinctAbsoluteHTTPSitemaps(t *testing.T) {
	t.Parallel()

	site := &robotsSite{body: strings.Join([]string{
		"User-agent: *",
		"Allow: /",
		"Sitemap: https://example.com/sitemap.xml",
		"Sitemap: HTTPS://EXAMPLE.COM:443/sitemap.xml",
		"Sitemap: http://sitemaps.example/pages.xml",
		"Sitemap: https://example.com/sitemap.xml",
		"Sitemap: /relative.xml",
		"Sitemap: ftp://example.com/files.xml",
	}, "\n")}
	server := httptest.NewServer(site)
	defer server.Close()

	result, err := Get(
		context.Background(),
		testutil.ParseURL(t, server.URL),
		testutil.NewFetcher(t, fetcher.Options{}),
		Options{
			Respect: true,
		},
	)
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	want := []string{
		"https://example.com/sitemap.xml",
		"http://sitemaps.example/pages.xml",
	}
	if got := testutil.URLStrings(result.SitemapURLs); !reflect.DeepEqual(got, want) {
		t.Fatalf("SitemapURLs = %#v, want %#v", got, want)
	}
}

func TestGetStatusAndFailurePolicies(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		status      int
		block       bool
		wantAllowed bool
		wantError   bool
	}{
		{name: "client error allows all", status: http.StatusNotFound, wantAllowed: true},
		{name: "server error fails", status: http.StatusServiceUnavailable, wantError: true},
		{name: "redirect without location fails", status: http.StatusFound, wantError: true},
		{name: "request timeout fails", block: true, wantError: true},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			var handler http.Handler = &robotsSite{status: test.status}
			fetchOptions := fetcher.Options{MaxRedirects: 1}
			if test.block {
				handler = blockingRobotsSite{}
				fetchOptions.Timeout = 25 * time.Millisecond
			}
			server := httptest.NewServer(handler)
			defer server.Close()

			result, err := Get(
				context.Background(),
				testutil.ParseURL(t, server.URL),
				testutil.NewFetcher(t, fetchOptions),
				Options{
					Respect: true,
				},
			)
			if test.wantError {
				if err == nil || !strings.Contains(err.Error(), "robots.txt") {
					t.Fatalf("Get() error = %v, want robots.txt error", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("Get() error = %v", err)
			}
			if got := result.IsAllowed(testutil.ParseURL(t, server.URL+"/private")); got != test.wantAllowed {
				t.Errorf("same-origin allowed = %t, want %t", got, test.wantAllowed)
			}
			if !result.IsAllowed(testutil.ParseURL(t, "https://other.example/private")) {
				t.Error("other origin should remain allowed")
			}
		})
	}
}

func TestGetDoesNotRequireFetcherWhenRespectIsDisabled(t *testing.T) {
	t.Parallel()

	result, err := Get(
		context.Background(),
		testutil.ParseURL(t, "https://example.com"),
		nil,
		Options{Respect: false},
	)
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	if !result.IsAllowed(testutil.ParseURL(t, "https://example.com/private")) {
		t.Error("disabled robots policy should allow all URLs")
	}
}

func TestGetRejectsOversizedBody(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(&robotsSite{body: strings.Repeat("x", maxBodyBytes+1)})
	defer server.Close()
	baseURL := testutil.ParseURL(t, server.URL)
	_, err := Get(context.Background(), baseURL, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Respect: true,
	})
	if err == nil || !strings.Contains(err.Error(), "exceeds") {
		t.Fatalf("Get() error = %v, want document size error", err)
	}
}

func TestGetRejectsMalformedAndTruncatedDocuments(t *testing.T) {
	t.Parallel()

	t.Run("parse error", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(&robotsSite{body: "Disallow: /\nUser-agent: bot"})
		defer server.Close()
		_, err := Get(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{Respect: true})
		if err == nil || !strings.Contains(err.Error(), "parse robots.txt") {
			t.Fatalf("Get() error = %v, want parse error", err)
		}
	})

	t.Run("truncated body", func(t *testing.T) {
		t.Parallel()
		server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
			connection, _, err := writer.(http.Hijacker).Hijack()
			if err != nil {
				return
			}
			_, _ = fmt.Fprint(connection, "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort")
			_ = connection.Close()
		}))
		defer server.Close()
		_, err := Get(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}), Options{Respect: true})
		if err == nil || !strings.Contains(err.Error(), "read robots.txt") {
			t.Fatalf("Get() error = %v, want read error", err)
		}
	})
}

func TestRequestTargetDefaultsPathAndIncludesQuery(t *testing.T) {
	t.Parallel()

	if got := requestTarget(testutil.ParseURL(t, "https://example.com?one=two")); got != "/?one=two" {
		t.Fatalf("requestTarget() = %q", got)
	}
}

func TestGetReturnsContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err := Get(ctx, testutil.ParseURL(t, "https://example.com"), testutil.NewFetcher(t, fetcher.Options{}), Options{
		Respect: true,
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Get() error = %v, want context.Canceled", err)
	}
}

func TestGetReturnsCancellationFromInFlightFetch(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		cancel()
		connection, _, err := response.(http.Hijacker).Hijack()
		if err == nil {
			_ = connection.Close()
		}
	}))
	defer server.Close()

	_, err := Get(
		ctx,
		testutil.ParseURL(t, server.URL),
		testutil.NewFetcher(t, fetcher.Options{}),
		Options{Respect: true},
	)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Get() error = %v, want context.Canceled", err)
	}
}

func TestGetReturnsCancellationWhileReadingBody(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	headersSent := make(chan struct{})
	releaseBody := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		response.Header().Set("Content-Length", "1024")
		response.WriteHeader(http.StatusOK)
		response.(http.Flusher).Flush()
		close(headersSent)
		<-releaseBody
		_, _ = response.Write([]byte("User-agent: *\n"))
	}))
	defer server.Close()

	baseURL := testutil.ParseURL(t, server.URL)
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	errorsChannel := make(chan error, 1)
	go func() {
		_, err := Get(
			ctx,
			baseURL,
			httpFetcher,
			Options{Respect: true},
		)
		errorsChannel <- err
	}()
	<-headersSent
	time.Sleep(10 * time.Millisecond)
	cancel()
	close(releaseBody)
	if err := <-errorsChannel; !errors.Is(err, context.Canceled) {
		t.Fatalf("Get() error = %v, want context.Canceled", err)
	}
}

func TestNilArguments(t *testing.T) {
	t.Parallel()

	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	baseURL := testutil.ParseURL(t, "https://example.com")

	t.Run("nil context", func(t *testing.T) {
		t.Parallel()

		//nolint:staticcheck // The nil input intentionally verifies Get's boundary guard.
		if _, err := Get(nil, baseURL, httpFetcher, Options{Respect: true}); !errors.Is(err, ErrNilContext) {
			t.Fatalf("Get() error = %v, want ErrNilContext", err)
		}
	})

	t.Run("nil base URL", func(t *testing.T) {
		t.Parallel()

		if _, err := Get(context.Background(), nil, httpFetcher, Options{Respect: true}); !errors.Is(err, ErrNilBaseURL) {
			t.Fatalf("Get() error = %v, want ErrNilBaseURL", err)
		}
	})

	t.Run("nil fetcher", func(t *testing.T) {
		t.Parallel()

		if _, err := Get(context.Background(), baseURL, nil, Options{Respect: true}); !errors.Is(err, ErrNilFetcher) {
			t.Fatalf("Get() error = %v, want ErrNilFetcher", err)
		}
	})
}

type robotsSite struct {
	mutex       sync.Mutex
	status      int
	body        string
	requestPath string
}

func (site *robotsSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	site.mutex.Lock()
	site.requestPath = request.URL.Path
	site.mutex.Unlock()
	status := site.status
	if status == 0 {
		status = http.StatusOK
	}
	response.WriteHeader(status)
	_, _ = response.Write([]byte(site.body))
}

type blockingRobotsSite struct{}

func (blockingRobotsSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	<-request.Context().Done()
}
