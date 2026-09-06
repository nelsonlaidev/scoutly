// Package robots retrieves and evaluates a site's robots.txt policy.
package robots

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"sync"

	"github.com/temoto/robotstxt"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

const (
	defaultUserAgent = "scoutly"
	maxBodyBytes     = 512 * 1024
)

var (
	ErrNilContext = errors.New("robots: nil context")
	ErrNilBaseURL = errors.New("robots: nil base URL")
	ErrNilFetcher = errors.New("robots: nil fetcher")
)

// Options controls robots.txt discovery.
type Options struct {
	Respect   bool
	UserAgent string
}

// Result contains the applicable robots.txt policy and declared sitemaps.
type Result struct {
	IsAllowed   func(*url.URL) bool
	SitemapURLs []*url.URL
}

type cacheEntry struct {
	ready  chan struct{}
	result Result
	err    error
}

// Cache stores one robots.txt result per origin and coalesces concurrent
// lookups for the same origin.
type Cache struct {
	mu      sync.Mutex
	entries map[string]*cacheEntry
}

// NewCache creates an empty robots.txt policy cache.
func NewCache() *Cache {
	return &Cache{entries: make(map[string]*cacheEntry)}
}

// GetForOrigin returns the cached robots.txt policy for baseURL's origin or
// fetches it once when absent.
func (cache *Cache) GetForOrigin(
	ctx context.Context,
	baseURL *url.URL,
	httpFetcher *fetcher.Fetcher,
	options Options,
) (Result, error) {
	if ctx == nil {
		return Result{}, ErrNilContext
	}
	if err := ctx.Err(); err != nil {
		return Result{}, err
	}
	if baseURL == nil {
		return Result{}, ErrNilBaseURL
	}
	if cache == nil {
		return Get(ctx, baseURL, httpFetcher, options)
	}

	key := urlutil.Origin(baseURL)
	cache.mu.Lock()
	entry, exists := cache.entries[key]
	if exists {
		cache.mu.Unlock()
		select {
		case <-entry.ready:
			return cloneResult(entry.result), entry.err
		case <-ctx.Done():
			return Result{}, ctx.Err()
		}
	}

	entry = &cacheEntry{ready: make(chan struct{})}
	cache.entries[key] = entry
	cache.mu.Unlock()

	entry.result, entry.err = Get(ctx, baseURL, httpFetcher, options)
	cache.mu.Lock()
	if entry.err != nil {
		delete(cache.entries, key)
	}
	close(entry.ready)
	cache.mu.Unlock()

	return cloneResult(entry.result), entry.err
}

// Lookup returns a previously loaded policy for candidate's origin.
func (cache *Cache) Lookup(candidate *url.URL) (Result, bool) {
	if cache == nil || candidate == nil {
		return Result{}, false
	}

	cache.mu.Lock()
	entry, exists := cache.entries[urlutil.Origin(candidate)]
	if !exists {
		cache.mu.Unlock()
		return Result{}, false
	}
	ready := entry.ready
	cache.mu.Unlock()

	select {
	case <-ready:
		if entry.err != nil {
			return Result{}, false
		}
		return cloneResult(entry.result), true
	default:
		return Result{}, false
	}
}

// Get retrieves and parses the robots.txt document for baseURL.
//
// Failures that prevent determining the policy are returned to the caller so
// the audit can stop without silently producing an empty report. A 4xx
// response means that all URLs are allowed. Context cancellation is always
// returned to the caller.
func Get(
	ctx context.Context,
	baseURL *url.URL,
	httpFetcher *fetcher.Fetcher,
	options Options,
) (Result, error) {
	if ctx == nil {
		return Result{}, ErrNilContext
	}
	if err := ctx.Err(); err != nil {
		return Result{}, err
	}
	if baseURL == nil {
		return Result{}, ErrNilBaseURL
	}
	if !options.Respect {
		return allowAll(), nil
	}
	if httpFetcher == nil {
		return Result{}, ErrNilFetcher
	}

	robotsURL := &url.URL{
		Scheme: baseURL.Scheme,
		Host:   baseURL.Host,
		Path:   "/robots.txt",
	}
	resp, err := httpFetcher.Fetch(ctx, robotsURL)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Result{}, ctxErr
		}
		return Result{}, fmt.Errorf("fetch robots.txt %s: %w", robotsURL, err)
	}
	if ctxErr := ctx.Err(); ctxErr != nil {
		if resp != nil && resp.Body != nil {
			_ = resp.Body.Close()
		}
		return Result{}, ctxErr
	}
	if resp == nil {
		return Result{}, fmt.Errorf("fetch robots.txt %s: empty response", robotsURL)
	}
	if resp.Body == nil {
		return Result{}, fmt.Errorf("fetch robots.txt %s: response body is nil", robotsURL)
	}
	defer func() {
		_ = resp.Body.Close()
	}()

	switch {
	case resp.StatusCode >= http.StatusBadRequest && resp.StatusCode < http.StatusInternalServerError:
		return allowAll(), nil
	case resp.StatusCode < http.StatusOK || resp.StatusCode >= http.StatusMultipleChoices:
		return Result{}, fmt.Errorf(
			"fetch robots.txt %s: unexpected HTTP status %d",
			robotsURL,
			resp.StatusCode,
		)
	}

	body, err := io.ReadAll(io.LimitReader(resp.Body, maxBodyBytes+1))
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Result{}, ctxErr
		}
		return Result{}, fmt.Errorf("read robots.txt %s: %w", robotsURL, err)
	}
	if err := ctx.Err(); err != nil {
		return Result{}, err
	}
	if len(body) > maxBodyBytes {
		return Result{}, fmt.Errorf(
			"read robots.txt %s: document exceeds %d-byte limit",
			robotsURL,
			maxBodyBytes,
		)
	}

	data, err := robotstxt.FromBytes(body)
	if err != nil {
		return Result{}, fmt.Errorf("parse robots.txt %s: %w", robotsURL, err)
	}

	userAgent := strings.TrimSpace(options.UserAgent)
	if userAgent == "" {
		userAgent = defaultUserAgent
	}
	group := data.FindGroup(userAgent)
	origin := urlutil.Origin(baseURL)

	return Result{
		IsAllowed: func(candidate *url.URL) bool {
			if candidate == nil {
				return false
			}
			if urlutil.Origin(candidate) != origin {
				return true
			}
			return group.Test(requestTarget(candidate))
		},
		SitemapURLs: collectSitemaps(data.Sitemaps),
	}, nil
}

func allowAll() Result {
	return Result{
		IsAllowed:   func(*url.URL) bool { return true },
		SitemapURLs: make([]*url.URL, 0),
	}
}

func collectSitemaps(values []string) []*url.URL {
	result := make([]*url.URL, 0, len(values))
	seen := make(map[string]struct{}, len(values))

	for _, value := range values {
		candidate, ok := urlutil.ParseHTTP(value)
		if !ok {
			continue
		}

		normalized := urlutil.Normalize(candidate, true)
		key := normalized.String()
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		result = append(result, normalized)
	}

	return result
}

func requestTarget(candidate *url.URL) string {
	path := candidate.EscapedPath()
	if path == "" {
		path = "/"
	}
	if candidate.RawQuery != "" {
		path += "?" + candidate.RawQuery
	}
	return path
}

func cloneResult(result Result) Result {
	cloned := Result{IsAllowed: result.IsAllowed}
	cloned.SitemapURLs = make([]*url.URL, 0, len(result.SitemapURLs))
	for _, sitemapURL := range result.SitemapURLs {
		if sitemapURL == nil {
			cloned.SitemapURLs = append(cloned.SitemapURLs, nil)
			continue
		}
		copy := *sitemapURL
		cloned.SitemapURLs = append(cloned.SitemapURLs, &copy)
	}
	return cloned
}
