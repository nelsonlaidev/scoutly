// Package fetcher provides context-aware HTTP fetching with shared rate
// limiting and explicit redirect handling.
package fetcher

import (
	"context"
	"errors"
	"fmt"
	"io"
	"math"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/urlutil"

	"golang.org/x/time/rate"
)

const (
	defaultUserAgent      = "scoutly/dev"
	defaultAccept         = "*/*"
	defaultAcceptLanguage = "en-US,en;q=0.9"
	redirectDrainLimit    = 2 << 10
)

var (
	ErrNilContext = errors.New("fetcher: nil context")
	ErrNilRequest = errors.New("fetcher: nil request")
	ErrNilURL     = errors.New("fetcher: nil URL")
)

// RedirectPolicy validates a redirect destination before its request is sent.
// Returning an error rejects the redirect.
type RedirectPolicy func(context.Context, *url.URL) error

// Options configures a Fetcher.
//
// Timeout covers the complete operation, including rate-limit waits and all
// redirects. A zero Timeout disables the fetcher-level timeout.
//
// MaxRedirects is the maximum number of redirects that may be followed. Zero
// disables redirects.
//
// RateLimit is the maximum number of outgoing requests per second across all
// concurrent calls made through the Fetcher. Zero disables rate limiting.
type Options struct {
	Timeout      time.Duration
	MaxRedirects int
	RateLimit    float64
	UserAgent    string
}

// TooManyRedirectsError reports that following the next redirect would exceed
// the configured limit.
type TooManyRedirectsError struct {
	Limit int
	URL   string
}

func (e *TooManyRedirectsError) Error() string {
	return fmt.Sprintf("fetcher: too many redirects (limit %d) at %s", e.Limit, e.URL)
}

// Fetcher is safe for concurrent use.
type Fetcher struct {
	client       *http.Client
	limiter      *rate.Limiter
	timeout      time.Duration
	maxRedirects int
	userAgent    string
}

// New constructs a Fetcher.
func New(options Options) (*Fetcher, error) {
	if options.Timeout < 0 {
		return nil, fmt.Errorf("fetcher: timeout must not be negative")
	}
	if options.MaxRedirects < 0 {
		return nil, fmt.Errorf("fetcher: max redirects must not be negative")
	}
	if options.RateLimit < 0 || math.IsNaN(options.RateLimit) || math.IsInf(options.RateLimit, 0) {
		return nil, fmt.Errorf("fetcher: rate limit must be a finite non-negative number")
	}
	if strings.ContainsAny(options.UserAgent, "\r\n") {
		return nil, fmt.Errorf("fetcher: user agent must not contain a newline")
	}

	userAgent := options.UserAgent
	if userAgent == "" {
		userAgent = defaultUserAgent
	}

	fetcher := &Fetcher{
		client: &http.Client{
			CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
		timeout:      options.Timeout,
		maxRedirects: options.MaxRedirects,
		userAgent:    userAgent,
	}

	if options.RateLimit > 0 {
		fetcher.limiter = rate.NewLimiter(rate.Limit(options.RateLimit), 1)
	}

	return fetcher, nil
}

// Fetch performs a GET request for target. Optional redirect policies run in
// order before each redirect destination is requested. The caller owns the
// returned response body and must close it.
func (f *Fetcher) Fetch(
	ctx context.Context,
	target *url.URL,
	redirectPolicies ...RedirectPolicy,
) (*http.Response, error) {
	if ctx == nil {
		return nil, ErrNilContext
	}
	if target == nil {
		return nil, ErrNilURL
	}

	request, err := http.NewRequestWithContext(ctx, http.MethodGet, target.String(), nil)
	if err != nil {
		return nil, fmt.Errorf("fetcher: create request: %w", err)
	}

	return f.Do(ctx, request, redirectPolicies...)
}

// Do executes request while applying the Fetcher's defaults, rate limit,
// timeout, and redirect policies. Both ctx and request.Context() are honored.
// The caller owns the returned response body and must close it.
//
// Redirects of status 307 and 308 replay the request body via
// request.GetBody; Do returns an error if the request carries a body but has
// no GetBody function. The returned response body is not safe for concurrent
// reads or concurrent reads and Close calls.
func (f *Fetcher) Do(
	ctx context.Context,
	request *http.Request,
	redirectPolicies ...RedirectPolicy,
) (*http.Response, error) {
	if ctx == nil {
		return nil, ErrNilContext
	}
	if request == nil {
		return nil, ErrNilRequest
	}

	combinedContext, stopCombining := combineContexts(ctx, request.Context())

	operationContext := combinedContext
	cancelTimeout := func() {}
	if f.timeout > 0 {
		operationContext, cancelTimeout = context.WithTimeout(combinedContext, f.timeout)
	}

	var cleanupOnce sync.Once
	cleanup := func() {
		cleanupOnce.Do(func() {
			cancelTimeout()
			stopCombining()
		})
	}
	handedOffBody := false
	defer func() {
		if !handedOffBody {
			cleanup()
		}
	}()

	current := request.Clone(operationContext)
	current.RequestURI = ""
	applyDefaultHeaders(current.Header, f.userAgent)

	redirects := 0
	for {
		if err := f.wait(operationContext); err != nil {
			return nil, err
		}

		response, err := f.client.Do(current)
		if err != nil {
			if response != nil && response.Body != nil {
				_ = response.Body.Close()
			}
			if cause := context.Cause(operationContext); cause != nil {
				return nil, cause
			}
			return nil, fmt.Errorf("fetcher: request %s: %w", current.URL, err)
		}

		if !isRedirect(response.StatusCode) {
			response.Body = &cleanupReadCloser{
				ReadCloser: response.Body,
				cleanup:    cleanup,
			}
			handedOffBody = true
			return response, nil
		}

		location := response.Header.Get("Location")
		if location == "" {
			response.Body = &cleanupReadCloser{
				ReadCloser: response.Body,
				cleanup:    cleanup,
			}
			handedOffBody = true
			return response, nil
		}

		nextURL, err := current.URL.Parse(location)
		if err != nil {
			closeRedirectBody(response.Body)
			return nil, fmt.Errorf("fetcher: parse redirect location %q: %w", location, err)
		}

		if redirects >= f.maxRedirects {
			closeRedirectBody(response.Body)
			return nil, &TooManyRedirectsError{
				Limit: f.maxRedirects,
				URL:   nextURL.String(),
			}
		}
		for _, policy := range redirectPolicies {
			if policy == nil {
				continue
			}
			if err := policy(operationContext, nextURL); err != nil {
				closeRedirectBody(response.Body)
				if cause := context.Cause(operationContext); cause != nil {
					return nil, cause
				}
				return nil, fmt.Errorf("fetcher: redirect to %s rejected: %w", nextURL, err)
			}
		}

		next, err := redirectRequest(operationContext, current, nextURL, response.StatusCode)
		closeRedirectBody(response.Body)
		if err != nil {
			return nil, err
		}

		redirects++
		current = next
	}
}

func (f *Fetcher) wait(ctx context.Context) error {
	if f.limiter == nil {
		return nil
	}

	if err := f.limiter.Wait(ctx); err != nil {
		if cause := context.Cause(ctx); cause != nil {
			return cause
		}
		if _, hasDeadline := ctx.Deadline(); hasDeadline {
			return context.DeadlineExceeded
		}
		return fmt.Errorf("fetcher: wait for rate limit: %w", err)
	}

	return nil
}

func combineContexts(primary, secondary context.Context) (context.Context, func()) {
	combined, cancel := context.WithCancelCause(primary)
	stopSecondary := context.AfterFunc(secondary, func() {
		cancel(context.Cause(secondary))
	})

	return combined, func() {
		stopSecondary()
		cancel(nil)
	}
}

func applyDefaultHeaders(header http.Header, userAgent string) {
	if header.Get("Accept") == "" {
		header.Set("Accept", defaultAccept)
	}
	if header.Get("Accept-Language") == "" {
		header.Set("Accept-Language", defaultAcceptLanguage)
	}
	if header.Get("User-Agent") == "" {
		header.Set("User-Agent", userAgent)
	}
}

func isRedirect(statusCode int) bool {
	switch statusCode {
	case http.StatusMovedPermanently,
		http.StatusFound,
		http.StatusSeeOther,
		http.StatusTemporaryRedirect,
		http.StatusPermanentRedirect:
		return true
	default:
		return false
	}
}

func redirectRequest(
	ctx context.Context,
	previous *http.Request,
	nextURL *url.URL,
	statusCode int,
) (*http.Request, error) {
	method := previous.Method
	body := io.ReadCloser(nil)
	getBody := previous.GetBody
	contentLength := previous.ContentLength
	dropBody := false

	switch statusCode {
	case http.StatusMovedPermanently, http.StatusFound:
		if method != http.MethodGet && method != http.MethodHead {
			method = http.MethodGet
		}
		getBody = nil
		contentLength = 0
		dropBody = true
	case http.StatusSeeOther:
		if method != http.MethodHead {
			method = http.MethodGet
		}
		getBody = nil
		contentLength = 0
		dropBody = true
	case http.StatusTemporaryRedirect, http.StatusPermanentRedirect:
		if previous.Body != nil {
			if getBody == nil {
				return nil, fmt.Errorf(
					"fetcher: cannot replay %s request body for %d redirect",
					previous.Method,
					statusCode,
				)
			}

			var err error
			body, err = getBody()
			if err != nil {
				return nil, fmt.Errorf("fetcher: replay request body: %w", err)
			}
		}
	}

	next := previous.Clone(ctx)
	next.Method = method
	next.URL = nextURL
	next.RequestURI = ""
	next.Host = ""
	next.Body = body
	next.GetBody = getBody
	next.ContentLength = contentLength

	if dropBody {
		next.Body = nil
		next.GetBody = nil
		next.ContentLength = 0
		next.Header.Del("Content-Length")
		next.Header.Del("Content-Type")
		next.Header.Del("Transfer-Encoding")
	}

	if urlutil.Origin(previous.URL) != urlutil.Origin(nextURL) {
		next.Header.Del("Authorization")
		next.Header.Del("Cookie")
		next.Header.Del("Proxy-Authorization")
	}

	return next, nil
}

func closeRedirectBody(body io.ReadCloser) {
	if body == nil {
		return
	}

	_, _ = io.Copy(io.Discard, io.LimitReader(body, redirectDrainLimit))
	_ = body.Close()
}

type cleanupReadCloser struct {
	io.ReadCloser
	cleanup func()
}

func (body *cleanupReadCloser) Read(buffer []byte) (int, error) {
	read, err := body.ReadCloser.Read(buffer)
	if err != nil {
		body.cleanup()
	}
	return read, err
}

func (body *cleanupReadCloser) Close() error {
	err := body.ReadCloser.Close()
	body.cleanup()
	return err
}
