package fetcher

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"golang.org/x/time/rate"
)

func TestTooManyRedirectsError(t *testing.T) {
	t.Parallel()

	err := (&TooManyRedirectsError{Limit: 2, URL: "https://example.com/final"}).Error()
	want := "fetcher: too many redirects (limit 2) at https://example.com/final"
	if err != want {
		t.Fatalf("TooManyRedirectsError.Error() = %q, want %q", err, want)
	}
}

func TestNewValidatesOptions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		options Options
	}{
		{
			name:    "negative timeout",
			options: Options{Timeout: -time.Second},
		},
		{
			name:    "negative redirects",
			options: Options{MaxRedirects: -1},
		},
		{
			name:    "negative rate",
			options: Options{RateLimit: -1},
		},
		{
			name:    "invalid user agent",
			options: Options{UserAgent: "scoutly\r\ninjected: value"},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			if _, err := New(test.options); err == nil {
				t.Fatal("New() error = nil, want validation error")
			}
		})
	}
}

func TestNilArguments(t *testing.T) {
	t.Parallel()

	client, err := New(Options{})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	t.Run("Fetch with nil context", func(t *testing.T) {
		t.Parallel()

		//nolint:staticcheck // Nil context intentionally verifies the public boundary guard.
		response, err := client.Fetch(nil, mustURL(t, "https://example.com"))
		if response != nil {
			_ = response.Body.Close()
			t.Fatal("Fetch() returned a response")
		}
		if !errors.Is(err, ErrNilContext) {
			t.Fatalf("Fetch() error = %v, want ErrNilContext", err)
		}
	})

	t.Run("Fetch with nil URL", func(t *testing.T) {
		t.Parallel()

		response, err := client.Fetch(context.Background(), nil)
		if response != nil {
			_ = response.Body.Close()
			t.Fatal("Fetch() returned a response")
		}
		if !errors.Is(err, ErrNilURL) {
			t.Fatalf("Fetch() error = %v, want ErrNilURL", err)
		}
	})

	t.Run("Do with nil context", func(t *testing.T) {
		t.Parallel()

		//nolint:staticcheck // Nil context intentionally verifies the public boundary guard.
		response, err := client.Do(nil, mustRequest(t, "https://example.com"))
		if response != nil {
			_ = response.Body.Close()
			t.Fatal("Do() returned a response")
		}
		if !errors.Is(err, ErrNilContext) {
			t.Fatalf("Do() error = %v, want ErrNilContext", err)
		}
	})

	t.Run("Do with nil request", func(t *testing.T) {
		t.Parallel()

		response, err := client.Do(context.Background(), nil)
		if response != nil {
			_ = response.Body.Close()
			t.Fatal("Do() returned a response")
		}
		if !errors.Is(err, ErrNilRequest) {
			t.Fatalf("Do() error = %v, want ErrNilRequest", err)
		}
	})
}

func TestFetchRejectsMalformedTarget(t *testing.T) {
	t.Parallel()

	client, err := New(Options{})
	if err != nil {
		t.Fatal(err)
	}
	target := &url.URL{Scheme: "http", Host: "[::1"}
	response, err := client.Fetch(context.Background(), target)
	if response != nil {
		_ = response.Body.Close()
		t.Fatal("Fetch() returned a response")
	}
	if err == nil || !strings.Contains(err.Error(), "create request") {
		t.Fatalf("Fetch() error = %v", err)
	}
}

func TestFetchAppliesDefaultHeaders(t *testing.T) {
	t.Parallel()

	headers := make(chan http.Header, 1)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		headers <- request.Header.Clone()
		_, _ = io.WriteString(writer, "response body")
	}))
	defer server.Close()

	client, err := New(Options{
		MaxRedirects: 1,
		UserAgent:    "scoutly-test/1.0",
	})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
	if err != nil {
		t.Fatalf("Fetch() error = %v", err)
	}
	defer func() {
		if closeErr := response.Body.Close(); closeErr != nil {
			t.Errorf("Body.Close() error = %v", closeErr)
		}
	}()

	body, err := io.ReadAll(response.Body)
	if err != nil {
		t.Fatalf("ReadAll() error = %v", err)
	}
	if got, want := string(body), "response body"; got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}

	gotHeaders := <-headers
	if got, want := gotHeaders.Get("Accept"), "*/*"; got != want {
		t.Errorf("Accept = %q, want %q", got, want)
	}
	if got, want := gotHeaders.Get("Accept-Language"), "en-US,en;q=0.9"; got != want {
		t.Errorf("Accept-Language = %q, want %q", got, want)
	}
	if got, want := gotHeaders.Get("User-Agent"), "scoutly-test/1.0"; got != want {
		t.Errorf("User-Agent = %q, want %q", got, want)
	}
}

func TestDoPreservesExplicitHeaders(t *testing.T) {
	t.Parallel()

	headers := make(chan http.Header, 1)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		headers <- request.Header.Clone()
		writer.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	client, err := New(Options{UserAgent: "configured-agent"})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	request, err := http.NewRequest(http.MethodGet, server.URL, nil)
	if err != nil {
		t.Fatalf("NewRequest() error = %v", err)
	}
	request.Header.Set("Accept", "text/html")
	request.Header.Set("Accept-Language", "zh-TW")
	request.Header.Set("User-Agent", "request-agent")

	response, err := client.Do(context.Background(), request)
	if err != nil {
		t.Fatalf("Do() error = %v", err)
	}
	if err := response.Body.Close(); err != nil {
		t.Fatalf("Body.Close() error = %v", err)
	}

	gotHeaders := <-headers
	if got, want := gotHeaders.Get("Accept"), "text/html"; got != want {
		t.Errorf("Accept = %q, want %q", got, want)
	}
	if got, want := gotHeaders.Get("Accept-Language"), "zh-TW"; got != want {
		t.Errorf("Accept-Language = %q, want %q", got, want)
	}
	if got, want := gotHeaders.Get("User-Agent"), "request-agent"; got != want {
		t.Errorf("User-Agent = %q, want %q", got, want)
	}
}

func TestFetchFollowsSupportedRelativeRedirects(t *testing.T) {
	t.Parallel()

	statusCodes := []int{
		http.StatusMovedPermanently,
		http.StatusFound,
		http.StatusSeeOther,
		http.StatusTemporaryRedirect,
		http.StatusPermanentRedirect,
	}

	for _, statusCode := range statusCodes {
		t.Run(http.StatusText(statusCode), func(t *testing.T) {
			t.Parallel()

			var finalRequests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				switch request.URL.Path {
				case "/start":
					writer.Header().Set("Location", "final")
					writer.WriteHeader(statusCode)
				case "/final":
					finalRequests.Add(1)
					_, _ = io.WriteString(writer, "done")
				default:
					http.NotFound(writer, request)
				}
			}))
			defer server.Close()

			client, err := New(Options{MaxRedirects: 1})
			if err != nil {
				t.Fatalf("New() error = %v", err)
			}

			response, err := client.Fetch(context.Background(), mustURL(t, server.URL+"/start"))
			if err != nil {
				t.Fatalf("Fetch() error = %v", err)
			}
			defer func() {
				if closeErr := response.Body.Close(); closeErr != nil {
					t.Errorf("Body.Close() error = %v", closeErr)
				}
			}()

			if got, want := response.Request.URL.Path, "/final"; got != want {
				t.Errorf("final URL path = %q, want %q", got, want)
			}
			if got, want := finalRequests.Load(), int32(1); got != want {
				t.Errorf("final requests = %d, want %d", got, want)
			}
		})
	}
}

func TestFetchReturnsRedirectWithoutLocation(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusFound)
	}))
	defer server.Close()
	client, err := New(Options{MaxRedirects: 1})
	if err != nil {
		t.Fatal(err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
	if err != nil {
		t.Fatalf("Fetch() error = %v", err)
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode != http.StatusFound {
		t.Fatalf("status = %d, want %d", response.StatusCode, http.StatusFound)
	}
}

func TestDoClosesRedirectResponseWhenClientRejectsRedirect(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Location", "/final")
		writer.WriteHeader(http.StatusFound)
	}))
	defer server.Close()
	client, err := New(Options{MaxRedirects: 1})
	if err != nil {
		t.Fatal(err)
	}
	client.client.CheckRedirect = func(_ *http.Request, _ []*http.Request) error {
		return errors.New("redirect rejected")
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
	if response != nil {
		_ = response.Body.Close()
		t.Fatal("Fetch() returned a response")
	}
	if err == nil || !strings.Contains(err.Error(), "redirect rejected") {
		t.Fatalf("Fetch() error = %v", err)
	}
}

func TestDoReportsRedirectBodyReplayError(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Location", "/final")
		writer.WriteHeader(http.StatusTemporaryRedirect)
	}))
	defer server.Close()
	client, err := New(Options{MaxRedirects: 1})
	if err != nil {
		t.Fatal(err)
	}
	request, err := http.NewRequest(http.MethodPost, server.URL, io.NopCloser(strings.NewReader("body")))
	if err != nil {
		t.Fatal(err)
	}

	response, err := client.Do(context.Background(), request)
	if response != nil {
		_ = response.Body.Close()
		t.Fatal("Do() returned a response")
	}
	if err == nil || !strings.Contains(err.Error(), "cannot replay") {
		t.Fatalf("Do() error = %v", err)
	}
}

func TestFetchRejectsInvalidRedirectLocation(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Location", "%")
		writer.WriteHeader(http.StatusFound)
	}))
	defer server.Close()
	client, err := New(Options{MaxRedirects: 1})
	if err != nil {
		t.Fatal(err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
	if response != nil {
		_ = response.Body.Close()
		t.Fatal("Fetch() returned a response")
	}
	if err == nil || !strings.Contains(err.Error(), "failed to parse Location") {
		t.Fatalf("Fetch() error = %v", err)
	}
}

func TestFetchReturnsTypedErrorAtRedirectLimit(t *testing.T) {
	t.Parallel()

	var finalRequests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/start" {
			writer.Header().Set("Location", "/final")
			writer.WriteHeader(http.StatusFound)
			return
		}
		finalRequests.Add(1)
		writer.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	client, err := New(Options{MaxRedirects: 0})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL+"/start"))
	if response != nil {
		if closeErr := response.Body.Close(); closeErr != nil {
			t.Errorf("Body.Close() error = %v", closeErr)
		}
		t.Fatal("Fetch() response is non-nil, want nil")
	}

	var redirectError *TooManyRedirectsError
	if !errors.As(err, &redirectError) {
		t.Fatalf("Fetch() error = %v, want *TooManyRedirectsError", err)
	}
	if got, want := redirectError.Limit, 0; got != want {
		t.Errorf("redirect limit = %d, want %d", got, want)
	}
	if got := finalRequests.Load(); got != 0 {
		t.Errorf("final requests = %d, want 0", got)
	}
}

func TestFetchAppliesRedirectPoliciesBeforeRequest(t *testing.T) {
	t.Parallel()

	want := errors.New("redirect is not authorized")
	var finalRequests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/start" {
			writer.Header().Set("Location", "/private")
			writer.WriteHeader(http.StatusFound)
			return
		}
		finalRequests.Add(1)
		writer.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	client, err := New(Options{MaxRedirects: 1})
	if err != nil {
		t.Fatal(err)
	}
	policyCalls := 0
	response, err := client.Fetch(
		context.Background(),
		mustURL(t, server.URL+"/start"),
		func(_ context.Context, destination *url.URL) error {
			policyCalls++
			if destination.Path != "/private" {
				t.Fatalf("redirect destination = %s", destination)
			}
			return want
		},
	)
	if response != nil {
		_ = response.Body.Close()
		t.Fatal("Fetch() response is non-nil, want nil")
	}
	if !errors.Is(err, want) {
		t.Fatalf("Fetch() error = %v, want policy error", err)
	}
	if policyCalls != 1 {
		t.Fatalf("policy calls = %d, want 1", policyCalls)
	}
	if got := finalRequests.Load(); got != 0 {
		t.Fatalf("final requests = %d, want 0", got)
	}
}

func TestRedirectHeaderPolicy(t *testing.T) {
	t.Parallel()

	t.Run("same origin retains sensitive headers", func(t *testing.T) {
		t.Parallel()

		headers := make(chan http.Header, 1)
		server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			if request.URL.Path == "/start" {
				writer.Header().Set("Location", "/final")
				writer.WriteHeader(http.StatusFound)
				return
			}
			headers <- request.Header.Clone()
			writer.WriteHeader(http.StatusNoContent)
		}))
		defer server.Close()

		client, err := New(Options{MaxRedirects: 1})
		if err != nil {
			t.Fatalf("New() error = %v", err)
		}

		request := mustRequest(t, server.URL+"/start")
		setSensitiveHeaders(request.Header)

		response, err := client.Do(context.Background(), request)
		if err != nil {
			t.Fatalf("Do() error = %v", err)
		}
		if closeErr := response.Body.Close(); closeErr != nil {
			t.Fatalf("Body.Close() error = %v", closeErr)
		}

		gotHeaders := <-headers
		assertSensitiveHeaders(t, gotHeaders, true)
	})

	t.Run("cross origin removes sensitive headers", func(t *testing.T) {
		t.Parallel()

		headers := make(chan http.Header, 1)
		destination := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			headers <- request.Header.Clone()
			writer.WriteHeader(http.StatusNoContent)
		}))
		defer destination.Close()

		source := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
			writer.Header().Set("Location", destination.URL)
			writer.WriteHeader(http.StatusFound)
		}))
		defer source.Close()

		client, err := New(Options{MaxRedirects: 1})
		if err != nil {
			t.Fatalf("New() error = %v", err)
		}

		request := mustRequest(t, source.URL)
		setSensitiveHeaders(request.Header)
		request.Header.Set("X-Trace", "keep-me")

		response, err := client.Do(context.Background(), request)
		if err != nil {
			t.Fatalf("Do() error = %v", err)
		}
		if closeErr := response.Body.Close(); closeErr != nil {
			t.Fatalf("Body.Close() error = %v", closeErr)
		}

		gotHeaders := <-headers
		assertSensitiveHeaders(t, gotHeaders, false)
		if got, want := gotHeaders.Get("X-Trace"), "keep-me"; got != want {
			t.Errorf("X-Trace = %q, want %q", got, want)
		}
	})
}

func TestTimeoutCoversRequestAndBodyRead(t *testing.T) {
	t.Parallel()

	t.Run("waiting for response", func(t *testing.T) {
		t.Parallel()

		server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
			<-request.Context().Done()
		}))
		defer server.Close()

		client, err := New(Options{Timeout: 25 * time.Millisecond})
		if err != nil {
			t.Fatalf("New() error = %v", err)
		}

		response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
		if response != nil {
			if closeErr := response.Body.Close(); closeErr != nil {
				t.Errorf("Body.Close() error = %v", closeErr)
			}
			t.Fatal("Fetch() response is non-nil, want nil")
		}
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("Fetch() error = %v, want context deadline exceeded", err)
		}
	})

	t.Run("reading response body", func(t *testing.T) {
		t.Parallel()

		server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			writer.WriteHeader(http.StatusOK)
			writer.(http.Flusher).Flush()
			<-request.Context().Done()
		}))
		defer server.Close()

		client, err := New(Options{Timeout: 25 * time.Millisecond})
		if err != nil {
			t.Fatalf("New() error = %v", err)
		}

		response, err := client.Fetch(context.Background(), mustURL(t, server.URL))
		if err != nil {
			t.Fatalf("Fetch() error = %v", err)
		}
		defer func() {
			if closeErr := response.Body.Close(); closeErr != nil {
				t.Errorf("Body.Close() error = %v", closeErr)
			}
		}()

		_, err = response.Body.Read(make([]byte, 1))
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("Body.Read() error = %v, want context deadline exceeded", err)
		}
	})
}

func TestCallerCancellationIsReturned(t *testing.T) {
	t.Parallel()

	started := make(chan struct{})
	var once atomic.Bool
	server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
		if once.CompareAndSwap(false, true) {
			close(started)
		}
		<-request.Context().Done()
	}))
	defer server.Close()

	client, err := New(Options{})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		response, fetchErr := client.Fetch(ctx, mustURL(t, server.URL))
		if response != nil {
			_ = response.Body.Close()
		}
		result <- fetchErr
	}()

	<-started
	cancel()

	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatalf("Fetch() error = %v, want context canceled", err)
	}
}

func TestSharedRateLimiterHonorsCancellation(t *testing.T) {
	t.Parallel()

	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		requests.Add(1)
		_, _ = io.WriteString(writer, "ok")
	}))
	defer server.Close()

	client, err := New(Options{
		RateLimit: 0.1,
	})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	first, err := client.Fetch(context.Background(), mustURL(t, server.URL+"/first"))
	if err != nil {
		t.Fatalf("first Fetch() error = %v", err)
	}
	if err := first.Body.Close(); err != nil {
		t.Fatalf("first Body.Close() error = %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		response, fetchErr := client.Fetch(ctx, mustURL(t, server.URL+"/second"))
		if response != nil {
			_ = response.Body.Close()
		}
		result <- fetchErr
	}()

	time.AfterFunc(25*time.Millisecond, cancel)
	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatalf("second Fetch() error = %v, want context canceled", err)
	}
	if got, want := requests.Load(), int32(1); got != want {
		t.Errorf("RoundTrip calls = %d, want %d", got, want)
	}
}

func TestRateLimiterAppliesToRedirects(t *testing.T) {
	t.Parallel()

	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		requests.Add(1)
		if request.URL.Path == "/start" {
			writer.Header().Set("Location", "/final")
			writer.WriteHeader(http.StatusFound)
			_, _ = io.WriteString(writer, "redirect")
			return
		}
		_, _ = io.WriteString(writer, "final")
	}))
	defer server.Close()

	client, err := New(Options{
		Timeout:      25 * time.Millisecond,
		MaxRedirects: 1,
		RateLimit:    0.1,
	})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL+"/start"))
	if response != nil {
		if closeErr := response.Body.Close(); closeErr != nil {
			t.Errorf("Body.Close() error = %v", closeErr)
		}
		t.Fatal("Fetch() response is non-nil, want nil")
	}
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("Fetch() error = %v, want context deadline exceeded", err)
	}
	if got, want := requests.Load(), int32(1); got != want {
		t.Errorf("RoundTrip calls = %d, want %d", got, want)
	}
}

func TestRedirectReturnsReadableFinalBody(t *testing.T) {
	t.Parallel()

	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		requests.Add(1)
		if request.URL.Path == "/start" {
			writer.Header().Set("Location", "/final")
			writer.WriteHeader(http.StatusFound)
			return
		}
		_, _ = io.WriteString(writer, "final")
	}))
	defer server.Close()

	client, err := New(Options{
		Timeout:      time.Minute,
		MaxRedirects: 1,
	})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	response, err := client.Fetch(context.Background(), mustURL(t, server.URL+"/start"))
	if err != nil {
		t.Fatalf("Fetch() error = %v", err)
	}

	body, err := io.ReadAll(response.Body)
	if err != nil {
		t.Fatalf("ReadAll() error = %v", err)
	}
	if got, want := string(body), "final"; got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}

	if err := response.Body.Close(); err != nil {
		t.Fatalf("Body.Close() error = %v", err)
	}
	if got, want := requests.Load(), int32(2); got != want {
		t.Fatalf("requests = %d, want %d", got, want)
	}
}

func TestRequestContextCancellationIsHonored(t *testing.T) {
	t.Parallel()

	started := make(chan struct{})
	var once atomic.Bool
	server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
		if once.CompareAndSwap(false, true) {
			close(started)
		}
		<-request.Context().Done()
	}))
	defer server.Close()

	client, err := New(Options{})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	requestContext, cancelRequest := context.WithCancel(context.Background())
	request, err := http.NewRequestWithContext(
		requestContext,
		http.MethodGet,
		server.URL,
		nil,
	)
	if err != nil {
		t.Fatalf("NewRequestWithContext() error = %v", err)
	}

	result := make(chan error, 1)
	go func() {
		response, fetchErr := client.Do(context.Background(), request)
		if response != nil {
			_ = response.Body.Close()
		}
		result <- fetchErr
	}()

	<-started
	cancelRequest()

	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatalf("Do() error = %v, want context canceled", err)
	}
}

func TestRedirectRequestBodyPoliciesAndErrors(t *testing.T) {
	t.Parallel()

	nextURL := mustURL(t, "https://example.com/final")

	post, err := http.NewRequest(http.MethodPost, "https://example.com/start", strings.NewReader("body"))
	if err != nil {
		t.Fatal(err)
	}
	post.Header.Set("Content-Type", "text/plain")
	redirected, err := redirectRequest(context.Background(), post, nextURL, http.StatusMovedPermanently)
	if err != nil {
		t.Fatal(err)
	}
	if redirected.Method != http.MethodGet || redirected.Body != nil || redirected.Header.Get("Content-Type") != "" {
		t.Fatalf("301 redirected request = %#v", redirected)
	}

	head, err := http.NewRequest(http.MethodHead, "https://example.com/start", nil)
	if err != nil {
		t.Fatal(err)
	}
	redirected, err = redirectRequest(context.Background(), head, nextURL, http.StatusSeeOther)
	if err != nil || redirected.Method != http.MethodHead {
		t.Fatalf("303 HEAD = %#v, %v", redirected, err)
	}

	unreplayable, err := http.NewRequest(http.MethodPost, "https://example.com/start", io.NopCloser(strings.NewReader("body")))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := redirectRequest(context.Background(), unreplayable, nextURL, http.StatusTemporaryRedirect); err == nil || !strings.Contains(err.Error(), "cannot replay") {
		t.Fatalf("unreplayable redirect error = %v", err)
	}

	replayFailure := errors.New("replay failed")
	unreplayable.GetBody = func() (io.ReadCloser, error) { return nil, replayFailure }
	if _, err := redirectRequest(context.Background(), unreplayable, nextURL, http.StatusPermanentRedirect); !errors.Is(err, replayFailure) {
		t.Fatalf("GetBody redirect error = %v", err)
	}
}

func TestRateLimitWaitReportsNonContextError(t *testing.T) {
	t.Parallel()

	client, err := New(Options{})
	if err != nil {
		t.Fatal(err)
	}
	client.limiter = rate.NewLimiter(rate.Limit(1), 0)
	if err := client.wait(context.Background()); err == nil || !strings.Contains(err.Error(), "wait for rate limit") {
		t.Fatalf("wait() error = %v", err)
	}
}

func TestCloseRedirectBodyAcceptsNil(t *testing.T) {
	t.Parallel()
	closeRedirectBody(nil)
}

func mustRequest(t *testing.T, target string) *http.Request {
	t.Helper()

	request, err := http.NewRequest(http.MethodGet, target, nil)
	if err != nil {
		t.Fatalf("NewRequest() error = %v", err)
	}
	return request
}

func mustURL(t *testing.T, target string) *url.URL {
	t.Helper()

	parsed, err := url.Parse(target)
	if err != nil {
		t.Fatalf("url.Parse(%q) error = %v", target, err)
	}
	return parsed
}

func setSensitiveHeaders(headers http.Header) {
	headers.Set("Authorization", "Bearer secret")
	headers.Set("Cookie", "session=secret")
	headers.Set("Proxy-Authorization", "Basic secret")
}

func assertSensitiveHeaders(t *testing.T, headers http.Header, present bool) {
	t.Helper()

	for _, name := range []string{"Authorization", "Cookie", "Proxy-Authorization"} {
		got := headers.Get(name)
		if present && got == "" {
			t.Errorf("%s is empty, want retained value", name)
		}
		if !present && got != "" {
			t.Errorf("%s = %q, want empty", name, got)
		}
	}
}
