package resource

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync/atomic"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestCheckCachesFragmentFreeRequests(t *testing.T) {
	var requests atomic.Int32
	server := httptest.NewServer(&resourceSite{requests: &requests})
	defer server.Close()
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	cache := NewCache()

	first, _ := url.Parse(server.URL + "/image.png#one")
	second, _ := url.Parse(server.URL + "/image.png#two")
	result, err := Check(context.Background(), first, httpFetcher, cache)
	if err != nil {
		t.Fatalf("Check() error = %v", err)
	}
	if _, err := Check(context.Background(), second, httpFetcher, cache); err != nil {
		t.Fatalf("cached Check() error = %v", err)
	}

	if got := requests.Load(); got != 1 {
		t.Fatalf("requests = %d, want 1", got)
	}
	if result.FinalURL.String() != server.URL+"/image.png" {
		t.Fatalf("FinalURL = %q", result.FinalURL)
	}
	if result.ContentType == nil || *result.ContentType != "image/png" {
		t.Fatalf("ContentType = %v", result.ContentType)
	}
}

func TestCacheNilReceiverAndResourceKey(t *testing.T) {
	t.Parallel()

	var cache *Cache
	cache.Set("ignored", Result{Kind: KindReachable})
	if result, ok := cache.Get("ignored"); ok || result != (Result{}) {
		t.Fatalf("nil Cache.Get() = %#v, %t", result, ok)
	}
	input := testutil.ParseURL(t, "HTTPS://EXAMPLE.COM:443/image.png#fragment")
	if got := Key(input); got != "https://example.com/image.png" {
		t.Fatalf("Key() = %q", got)
	}
}

func TestCheckPropagatesCallerCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	input, _ := url.Parse("https://example.com")
	_, err := Check(ctx, input, testutil.NewFetcher(t, fetcher.Options{}), nil)

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Check() error = %v, want context.Canceled", err)
	}
}

func TestCheckClassifiesFailures(t *testing.T) {
	server := httptest.NewServer(blockingResourceSite{})
	defer server.Close()

	input, _ := url.Parse(server.URL)
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{Timeout: 25 * time.Millisecond})
	result, err := Check(context.Background(), input, httpFetcher, nil)
	if err != nil {
		t.Fatalf("Check() error = %v", err)
	}
	if result.Kind != KindFailed || result.Failure != FailureRequestTimedOut {
		t.Fatalf("result = %#v", result)
	}
}

func TestClassifyError(t *testing.T) {
	t.Parallel()

	for _, test := range []struct {
		name string
		err  error
		want Failure
	}{
		{name: "deadline", err: context.DeadlineExceeded, want: FailureRequestTimedOut},
		{name: "network timeout", err: &net.DNSError{IsTimeout: true}, want: FailureRequestTimedOut},
		{name: "network failure", err: &net.DNSError{Err: "refused"}, want: FailureConnectionFailed},
		{name: "URL failure", err: &url.Error{Op: "Get", URL: "https://example.com", Err: errors.New("failed")}, want: FailureConnectionFailed},
		{name: "generic failure", err: errors.New("failed"), want: FailureRequestFailed},
	} {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := classifyError(test.err); got != test.want {
				t.Errorf("classifyError(%T) = %q, want %q", test.err, got, test.want)
			}
		})
	}
}

func TestCheckClassifiesAntiBotChallenges(t *testing.T) {
	tests := []struct {
		name        string
		mitigation  string
		wantKind    Kind
		wantFailure Failure
	}{
		{
			name:        "challenge",
			mitigation:  "ChAlLeNgE",
			wantKind:    KindBlocked,
			wantFailure: FailureAntiBotChallenge,
		},
		{
			name:     "ordinary forbidden response",
			wantKind: KindReachable,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
				if test.mitigation != "" {
					response.Header().Set("Cf-Mitigated", test.mitigation)
				}
				response.Header().Set("Content-Type", "text/html")
				response.WriteHeader(http.StatusForbidden)
			}))
			defer server.Close()

			result, err := Check(
				context.Background(),
				testutil.ParseURL(t, server.URL),
				testutil.NewFetcher(t, fetcher.Options{}),
				nil,
			)
			if err != nil {
				t.Fatalf("Check() error = %v", err)
			}
			if result.Kind != test.wantKind || result.Failure != test.wantFailure {
				t.Fatalf("result = %#v", result)
			}
			if result.StatusCode != http.StatusForbidden || result.FinalURL.String() != server.URL+"/" {
				t.Fatalf("response metadata = %#v", result)
			}
			if result.ContentType == nil || *result.ContentType != "text/html" {
				t.Fatalf("ContentType = %v", result.ContentType)
			}
		})
	}
}

type resourceSite struct {
	requests *atomic.Int32
}

func (site *resourceSite) ServeHTTP(response http.ResponseWriter, _ *http.Request) {
	site.requests.Add(1)
	response.Header().Set("Content-Type", "image/png")
	response.WriteHeader(http.StatusNoContent)
}

type blockingResourceSite struct{}

func (blockingResourceSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	<-request.Context().Done()
}
