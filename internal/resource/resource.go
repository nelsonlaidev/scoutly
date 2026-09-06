package resource

import (
	"context"
	"errors"
	"io"
	"net"
	"net/url"
	"strings"
	"sync"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

type Kind string

const (
	KindReachable Kind = "reachable"
	KindBlocked   Kind = "blocked"
	KindFailed    Kind = "failed"
)

type Failure string

const (
	FailureRequestTimedOut  Failure = "request-timed-out"
	FailureConnectionFailed Failure = "connection-failed"
	FailureRequestFailed    Failure = "request-failed"
	FailureAntiBotChallenge Failure = "anti-bot-challenge"
)

type Result struct {
	Kind        Kind
	StatusCode  int
	FinalURL    *url.URL
	ContentType *string
	Failure     Failure
}

type Cache struct {
	mu      sync.RWMutex
	results map[string]Result
}

func NewCache() *Cache {
	return &Cache{results: make(map[string]Result)}
}

func (cache *Cache) Get(key string) (Result, bool) {
	if cache == nil {
		return Result{}, false
	}

	cache.mu.RLock()
	defer cache.mu.RUnlock()

	result, ok := cache.results[key]
	return result, ok
}

func (cache *Cache) Set(key string, result Result) {
	if cache == nil {
		return
	}

	cache.mu.Lock()
	defer cache.mu.Unlock()

	cache.results[key] = result
}

func RequestURL(input *url.URL) *url.URL {
	return urlutil.Normalize(input, false)
}

func Key(input *url.URL) string {
	return RequestURL(input).String()
}

func Check(
	ctx context.Context,
	input *url.URL,
	httpFetcher *fetcher.Fetcher,
	cache *Cache,
) (Result, error) {
	requestURL := RequestURL(input)
	key := requestURL.String()
	if result, ok := cache.Get(key); ok {
		return result, nil
	}

	result, err := check(ctx, requestURL, httpFetcher)
	if err != nil {
		return Result{}, err
	}
	cache.Set(key, result)

	return result, nil
}

func check(
	ctx context.Context,
	input *url.URL,
	httpFetcher *fetcher.Fetcher,
) (Result, error) {
	response, err := httpFetcher.Fetch(ctx, input)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Result{}, ctxErr
		}

		return Result{
			Kind:    KindFailed,
			Failure: classifyError(err),
		}, nil
	}
	defer func() {
		_ = response.Body.Close()
	}()

	// Resource checks intentionally inspect headers only. Draining a small
	// response preserves connection reuse without allowing an unbounded read.
	_, _ = io.Copy(io.Discard, io.LimitReader(response.Body, 32*1024))

	finalURL := RequestURL(input)
	if response.Request != nil && response.Request.URL != nil {
		finalURL = RequestURL(response.Request.URL)
	}

	var contentType *string
	if value, ok := response.Header["Content-Type"]; ok {
		joined := strings.Join(value, ", ")
		contentType = &joined
	}

	kind := KindReachable
	failure := Failure("")
	if strings.EqualFold(strings.TrimSpace(response.Header.Get("Cf-Mitigated")), "challenge") {
		kind = KindBlocked
		failure = FailureAntiBotChallenge
	}

	return Result{
		Kind:        kind,
		StatusCode:  response.StatusCode,
		FinalURL:    finalURL,
		ContentType: contentType,
		Failure:     failure,
	}, nil
}

func classifyError(err error) Failure {
	if errors.Is(err, context.DeadlineExceeded) {
		return FailureRequestTimedOut
	}

	if networkError, ok := errors.AsType[net.Error](err); ok {
		if networkError.Timeout() {
			return FailureRequestTimedOut
		}
		return FailureConnectionFailed
	}

	if _, ok := errors.AsType[*url.Error](err); ok {
		return FailureConnectionFailed
	}

	return FailureRequestFailed
}
