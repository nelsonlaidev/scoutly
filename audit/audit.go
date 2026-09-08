package audit

import (
	"context"
	"errors"
	"fmt"
	"maps"
	"net/url"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/checker"
	"github.com/nelsonlaidev/scoutly/internal/crawler"
	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/resource"
	"github.com/nelsonlaidev/scoutly/internal/robots"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

// Audit crawls target and returns a complete website audit. It never returns a
// partial report: cancellation, invalid options, or a progress callback error
// produce a nil report and a non-nil error.
func Audit(
	ctx context.Context,
	target string,
	options Options,
	progress func(Progress) error,
) (*Report, error) {
	if ctx == nil {
		return nil, errors.New("scoutly: nil context")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	options.Rules = maps.Clone(options.Rules)
	if err := options.Validate(); err != nil {
		return nil, err
	}

	startURL, err := urlutil.ParseTarget(target)
	if err != nil {
		return nil, err
	}

	httpFetcher, err := fetcher.New(fetcher.Options{
		Timeout:      options.Timeout,
		MaxRedirects: options.MaxRedirects,
		RateLimit:    options.RateLimit,
		UserAgent:    options.UserAgent,
	})
	if err != nil {
		return nil, fmt.Errorf("create HTTP fetcher: %w", err)
	}
	reporter := newProgressReporter(progress)
	robotsCache := robots.NewCache()
	robotsOptions := robots.Options{
		Respect:   options.RespectRobots,
		UserAgent: options.UserAgent,
	}

	robotsURL := &url.URL{Scheme: startURL.Scheme, Host: startURL.Host, Path: "/robots.txt"}
	if err := reporter.setPhase(PhaseRobots, robotsURL); err != nil {
		return nil, err
	}
	_, err = robotsCache.GetForOrigin(ctx, startURL, httpFetcher, robotsOptions)
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	if err := reporter.setPhase(PhaseCrawl, startURL); err != nil {
		return nil, err
	}
	crawledPages, err := crawler.Crawl(ctx, startURL, httpFetcher, crawler.Options{
		Allowed: func(candidate *url.URL) bool {
			policy, ok := robotsCache.Lookup(candidate)
			if !ok {
				// Sitemap documents may be hosted outside the audited origin.
				// Page and redirect destinations are constrained separately.
				return true
			}
			return policy.IsAllowed(candidate)
		},
		SitemapURLsFor: func(scopeURL *url.URL) []*url.URL {
			policy, ok := robotsCache.Lookup(scopeURL)
			if !ok {
				return nil
			}
			return policy.SitemapURLs
		},
		ScanSitemaps:        options.Sitemaps,
		MaxDepth:            options.MaxDepth,
		MaxPages:            options.MaxPages,
		MaxSitemapDocuments: options.MaxSitemapDocuments,
		KeepFragments:       options.KeepFragments,
		Concurrency:         options.Concurrency,
		OnPageDiscovered:    reporter.pageDiscovered,
		OnPageCrawled:       reporter.pageCrawled,
		OnSitemapPhaseStarted: func() error {
			return reporter.setPhase(PhaseSitemaps, nil)
		},
		OnSitemapFetched: reporter.sitemapFetched,
		RedirectPolicy: func(policyContext context.Context, destination *url.URL) error {
			policy, policyErr := robotsCache.GetForOrigin(
				policyContext,
				destination,
				httpFetcher,
				robotsOptions,
			)
			if policyErr != nil {
				return policyErr
			}
			if !policy.IsAllowed(destination) {
				return fmt.Errorf("robots.txt disallows %s", destination)
			}
			return nil
		},
	})
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	cache := resource.NewCache()
	linkURLs, err := collectLinkURLs(ctx, crawledPages)
	if err != nil {
		return nil, err
	}
	if err := reporter.linksStarted(len(linkURLs)); err != nil {
		return nil, err
	}
	checkedLinks, err := checker.CheckLinks(ctx, linkURLs, httpFetcher, cache, checker.Options{
		Concurrency: options.Concurrency,
		OnComplete: func(completion checker.Completion) error {
			currentURL, parseErr := url.Parse(completion.URL)
			if parseErr != nil {
				return parseErr
			}
			return reporter.linkChecked(currentURL)
		},
	})
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	var checkedImages []checker.ImageResult
	if options.Images {
		imageReferences, collectErr := collectImageReferences(ctx, crawledPages)
		if collectErr != nil {
			return nil, collectErr
		}
		imageCount, countErr := countUniqueImages(ctx, imageReferences)
		if countErr != nil {
			return nil, countErr
		}
		if err := reporter.imagesStarted(imageCount); err != nil {
			return nil, err
		}
		checkedImages, err = checker.CheckImages(ctx, imageReferences, httpFetcher, cache, checker.Options{
			Concurrency: options.Concurrency,
			OnComplete: func(completion checker.Completion) error {
				return reporter.imageChecked(completion.URL)
			},
		})
		if err != nil {
			return nil, err
		}
	} else {
		checkedImages = make([]checker.ImageResult, 0)
		if err := reporter.imagesStarted(0); err != nil {
			return nil, err
		}
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	if err := reporter.setPhase(PhaseReport, nil); err != nil {
		return nil, err
	}
	report, err := buildReport(
		ctx,
		startURL,
		crawledPages,
		checkedLinks,
		checkedImages,
		options.Images,
		options.Rules,
		options.IgnoreRedirects,
		time.Now(),
	)
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	return report, nil
}

func collectLinkURLs(ctx context.Context, pages []crawler.CrawledPage) ([]*url.URL, error) {
	result := make([]*url.URL, 0)
	seen := make(map[string]struct{})
	for _, crawled := range pages {
		for _, link := range crawled.Page.Links {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			if link.URL == nil {
				continue
			}
			requestURL := resource.RequestURL(link.URL)
			key := requestURL.String()
			if _, exists := seen[key]; exists {
				continue
			}
			seen[key] = struct{}{}
			result = append(result, requestURL)
		}
	}
	return result, nil
}

func collectImageReferences(
	ctx context.Context,
	pages []crawler.CrawledPage,
) ([]page.ImageReference, error) {
	result := make([]page.ImageReference, 0)
	for _, crawled := range pages {
		for _, reference := range crawled.Page.ImageReferences {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			result = append(result, reference)
		}
	}
	return result, nil
}

func countUniqueImages(ctx context.Context, references []page.ImageReference) (int, error) {
	seen := make(map[string]struct{})
	for _, reference := range references {
		if err := ctx.Err(); err != nil {
			return 0, err
		}
		seen[checker.ImageReferenceKey(reference)] = struct{}{}
	}
	return len(seen), nil
}
