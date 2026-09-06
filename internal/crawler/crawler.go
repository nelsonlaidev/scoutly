// Package crawler discovers and fetches pages in deterministic breadth-first
// order.
package crawler

import (
	"context"
	"errors"
	"io"
	"net/url"
	"strings"
	"sync"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/ptrutil"
	"github.com/nelsonlaidev/scoutly/internal/sitemap"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

const maxHTMLBytes = 10 * 1024 * 1024

var (
	ErrNilContext          = errors.New("crawler: nil context")
	ErrNilStartURL         = errors.New("crawler: start URL is nil")
	ErrNilFetcher          = errors.New("crawler: fetcher is nil")
	ErrMaxPagesNotPositive = errors.New("crawler: max pages must be positive")
	ErrNegativeMaxDepth    = errors.New("crawler: max depth must not be negative")
	ErrInvalidConcurrency  = errors.New("crawler: concurrency must be positive")
)

type Options struct {
	Allowed               func(*url.URL) bool
	SitemapURLs           []*url.URL
	SitemapURLsFor        func(*url.URL) []*url.URL
	ScanSitemaps          bool
	MaxDepth              int
	MaxPages              int
	MaxSitemapDocuments   int
	KeepFragments         bool
	Concurrency           int
	OnPageDiscovered      func(*url.URL) error
	OnPageCrawled         func(*url.URL) error
	OnSitemapPhaseStarted func() error
	OnSitemapFetched      func(*url.URL) error
	RedirectPolicy        fetcher.RedirectPolicy
}

type CrawledPage struct {
	Page        page.Page
	URL         *url.URL
	FinalURL    *url.URL
	Depth       int
	StatusCode  *int
	ContentType *string
}

type queuedPage struct {
	URL   *url.URL
	Key   string
	Depth int
}

type crawledQueuedPage struct {
	Queued queuedPage
	Page   CrawledPage
}

func Crawl(
	ctx context.Context,
	startURL *url.URL,
	httpFetcher *fetcher.Fetcher,
	options Options,
) ([]CrawledPage, error) {
	if ctx == nil {
		return nil, ErrNilContext
	}
	if startURL == nil {
		return nil, ErrNilStartURL
	}
	if httpFetcher == nil {
		return nil, ErrNilFetcher
	}
	if options.MaxPages <= 0 {
		return nil, ErrMaxPagesNotPositive
	}
	if options.MaxDepth < 0 {
		return nil, ErrNegativeMaxDepth
	}
	if options.Concurrency <= 0 {
		return nil, ErrInvalidConcurrency
	}
	allowed := options.Allowed
	if allowed == nil {
		allowed = func(*url.URL) bool { return true }
	}

	initial := createQueuedPage(startURL, 0, options.KeepFragments)
	pages := make([]CrawledPage, 0, min(options.MaxPages, 128))
	if !allowed(initial.URL) {
		if options.OnSitemapPhaseStarted != nil {
			if err := options.OnSitemapPhaseStarted(); err != nil {
				return nil, err
			}
		}
		return pages, nil
	}

	queue := []queuedPage{initial}
	enqueued := map[string]struct{}{initial.Key: {}}
	crawlScopeURL := ptrutil.Clone(initial.URL)
	if options.OnPageDiscovered != nil {
		if err := options.OnPageDiscovered(ptrutil.Clone(initial.URL)); err != nil {
			return nil, err
		}
	}

	for len(queue) > 0 && len(pages) < options.MaxPages {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		batchSize := min(len(queue), options.Concurrency, options.MaxPages-len(pages))
		batch := append([]queuedPage(nil), queue[:batchSize]...)
		queue = queue[batchSize:]

		crawled, err := crawlBatch(ctx, batch, httpFetcher, options)
		if err != nil {
			return nil, err
		}
		for _, result := range crawled {
			pages = append(pages, result.Page)
			if result.Queued.Key == initial.Key && result.Page.FinalURL != nil {
				crawlScopeURL = ptrutil.Clone(result.Page.FinalURL)
			}
		}
		for _, result := range crawled {
			if result.Queued.Depth >= options.MaxDepth {
				continue
			}

			for _, link := range result.Page.Page.Links {
				if len(pages)+len(queue) >= options.MaxPages {
					break
				}
				if !isCrawlable(link, crawlScopeURL) {
					continue
				}
				next := createQueuedPage(link.URL, result.Queued.Depth+1, options.KeepFragments)
				if _, exists := enqueued[next.Key]; exists || !allowed(next.URL) {
					continue
				}

				enqueued[next.Key] = struct{}{}
				queue = append(queue, next)
				if options.OnPageDiscovered != nil {
					if err := options.OnPageDiscovered(ptrutil.Clone(next.URL)); err != nil {
						return nil, err
					}
				}
			}
		}
	}

	if options.OnSitemapPhaseStarted != nil {
		if err := options.OnSitemapPhaseStarted(); err != nil {
			return nil, err
		}
	}
	if !options.ScanSitemaps || options.MaxDepth == 0 || len(pages) >= options.MaxPages {
		return pages, nil
	}

	initialSitemaps := cloneURLs(options.SitemapURLs)
	if options.SitemapURLsFor != nil {
		initialSitemaps = cloneURLs(options.SitemapURLsFor(ptrutil.Clone(crawlScopeURL)))
	}
	if len(initialSitemaps) == 0 {
		initialSitemaps = []*url.URL{{
			Scheme: crawlScopeURL.Scheme,
			Host:   crawlScopeURL.Host,
			Path:   "/sitemap.xml",
		}}
	}

	discoveryContext, cancelDiscovery := context.WithCancelCause(ctx)
	defer cancelDiscovery(nil)

	var callbackMu sync.Mutex
	var callbackErr error
	sitemapURLs, err := sitemap.Crawl(discoveryContext, initialSitemaps, httpFetcher, sitemap.Options{
		Allow:            allowed,
		Scheme:           crawlScopeURL.Scheme,
		Host:             crawlScopeURL.Host,
		KeepFragments:    options.KeepFragments,
		ExcludedPageKeys: enqueued,
		MaxDocuments:     options.MaxSitemapDocuments,
		MaxPageURLs:      options.MaxPages - len(pages),
		OnDocumentFetched: func(currentURL *url.URL) {
			callbackMu.Lock()
			defer callbackMu.Unlock()
			if callbackErr != nil {
				return
			}
			if options.OnSitemapFetched != nil {
				callbackErr = options.OnSitemapFetched(ptrutil.Clone(currentURL))
			}
			if callbackErr != nil {
				cancelDiscovery(callbackErr)
			}
		},
	})
	callbackMu.Lock()
	progressErr := callbackErr
	callbackMu.Unlock()
	if progressErr != nil {
		return nil, progressErr
	}
	if err != nil {
		return nil, err
	}

	sitemapQueue := make([]queuedPage, 0, len(sitemapURLs))
	for _, pageURL := range sitemapURLs {
		queued := createQueuedPage(pageURL, -1, options.KeepFragments)
		if _, exists := enqueued[queued.Key]; exists {
			continue
		}
		enqueued[queued.Key] = struct{}{}
		sitemapQueue = append(sitemapQueue, queued)
		if options.OnPageDiscovered != nil {
			if err := options.OnPageDiscovered(ptrutil.Clone(queued.URL)); err != nil {
				return nil, err
			}
		}
	}

	for len(sitemapQueue) > 0 && len(pages) < options.MaxPages {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		batchSize := min(len(sitemapQueue), options.Concurrency, options.MaxPages-len(pages))
		batch := append([]queuedPage(nil), sitemapQueue[:batchSize]...)
		sitemapQueue = sitemapQueue[batchSize:]

		crawled, err := crawlBatch(ctx, batch, httpFetcher, options)
		if err != nil {
			return nil, err
		}
		for _, result := range crawled {
			pages = append(pages, result.Page)
		}
	}

	return pages, nil
}

func crawlBatch(
	ctx context.Context,
	batch []queuedPage,
	httpFetcher *fetcher.Fetcher,
	options Options,
) ([]crawledQueuedPage, error) {
	batchContext, cancel := context.WithCancelCause(ctx)
	defer cancel(nil)

	results := make([]crawledQueuedPage, len(batch))
	var waitGroup sync.WaitGroup
	for index, queued := range batch {
		waitGroup.Go(func() {
			if batchContext.Err() != nil {
				return
			}

			crawled, err := crawlPage(batchContext, queued, httpFetcher, options)
			if err != nil {
				cancel(err)
				return
			}
			results[index] = crawledQueuedPage{Queued: queued, Page: crawled}
			if options.OnPageCrawled != nil {
				if err := options.OnPageCrawled(ptrutil.Clone(queued.URL)); err != nil {
					cancel(err)
				}
			}
		})
	}
	waitGroup.Wait()

	if cause := context.Cause(batchContext); cause != nil {
		return nil, cause
	}
	return results, nil
}

func crawlPage(
	ctx context.Context,
	queued queuedPage,
	httpFetcher *fetcher.Fetcher,
	options Options,
) (CrawledPage, error) {
	result := CrawledPage{
		Page:     page.New(),
		URL:      ptrutil.Clone(queued.URL),
		FinalURL: ptrutil.Clone(queued.URL),
		Depth:    queued.Depth,
	}

	response, err := httpFetcher.Fetch(ctx, queued.URL, options.RedirectPolicy)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return CrawledPage{}, ctxErr
		}
		return result, nil
	}
	if response == nil || response.Body == nil {
		return result, nil
	}
	defer func() {
		_ = response.Body.Close()
	}()

	baseURL := queued.URL
	if response.Request != nil && response.Request.URL != nil {
		baseURL = response.Request.URL
		result.FinalURL = urlutil.Normalize(response.Request.URL, options.KeepFragments)
	}

	statusCode := response.StatusCode
	result.StatusCode = &statusCode
	contentTypeValue, hasContentType := response.Header["Content-Type"]
	contentType := strings.Join(contentTypeValue, ", ")
	if hasContentType {
		result.ContentType = &contentType
	}

	if !page.IsHTMLContentType(contentType) {
		_, _ = io.Copy(io.Discard, io.LimitReader(response.Body, 32*1024))
		result.Page.ContentType = contentType
		return result, nil
	}

	limited := &io.LimitedReader{R: response.Body, N: maxHTMLBytes + 1}
	parsed, err := page.Parse(limited, baseURL)
	if err != nil || limited.N <= 0 {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return CrawledPage{}, ctxErr
		}
		result.StatusCode = nil
		result.ContentType = nil
		result.Page = page.New()
		return result, nil
	}
	parsed.ContentType = contentType
	result.Page = parsed
	return result, nil
}

func createQueuedPage(input *url.URL, depth int, keepFragments bool) queuedPage {
	normalized := urlutil.Normalize(input, keepFragments)
	return queuedPage{URL: normalized, Key: normalized.String(), Depth: depth}
}

func isCrawlable(link page.Link, scopeURL *url.URL) bool {
	if link.URL == nil {
		return false
	}
	if link.Element != page.LinkElementAnchor && link.Element != page.LinkElementIframe {
		return false
	}
	if !urlutil.IsHTTPWithHost(link.URL) {
		return false
	}
	return urlutil.Origin(link.URL) == urlutil.Origin(scopeURL)
}

func cloneURLs(input []*url.URL) []*url.URL {
	result := make([]*url.URL, 0, len(input))
	for _, item := range input {
		result = append(result, ptrutil.Clone(item))
	}
	return result
}
