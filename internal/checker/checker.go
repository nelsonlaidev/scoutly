// Package checker checks links and image references with bounded concurrency.
package checker

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"sync"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/ptrutil"
	"github.com/nelsonlaidev/scoutly/internal/resource"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

var (
	ErrNilContext         = errors.New("checker: nil context")
	ErrInvalidConcurrency = errors.New("checker: concurrency must be positive")
	ErrNilFetcher         = errors.New("checker: fetcher is nil")
	ErrNilLink            = errors.New("checker: link is nil")
)

// ResultKind identifies the outcome of a logical link or image check.
type ResultKind string

const (
	ResultReachable ResultKind = "reachable"
	ResultBlocked   ResultKind = "blocked"
	ResultFailed    ResultKind = "failed"
	ResultSkipped   ResultKind = "skipped"
	ResultInvalid   ResultKind = "invalid"
)

// Reason identifies why a result was skipped or invalid.
type Reason string

const (
	ReasonUnsupportedProtocol Reason = "unsupported-protocol"
	ReasonInvalidURL          Reason = "invalid-url"
)

// Target identifies the type of logical result delivered to OnComplete.
type Target string

const (
	TargetLink  Target = "link"
	TargetImage Target = "image"
)

// Completion describes one completed logical result. For links, Key and URL
// are the distinct input URL. For images, Key is ImageReferenceKey and URL is
// the display URL.
type Completion struct {
	Target Target
	Key    string
	URL    string
}

// Options configures CheckLinks and CheckImages.
type Options struct {
	Concurrency int
	OnComplete  func(Completion) error
}

// LinkResult describes one distinct input link. RequestURL omits the fragment
// used only for the HTTP request and cache key.
type LinkResult struct {
	URL        string
	RequestURL string
	Kind       ResultKind
	StatusCode int
	FinalURL   string
	Failure    resource.Failure
	Reason     Reason
}

// ImageResult describes one unique image reference key. URL is the display URL
// and is fragment-free for parsed URLs.
type ImageResult struct {
	Key         string
	URL         string
	Kind        ResultKind
	StatusCode  int
	FinalURL    string
	ContentType *string
	Failure     resource.Failure
	Reason      Reason
}

// CheckLinks checks distinct input links using one request for each
// fragment-free URL. Results preserve the order of the first occurrence of
// each distinct input URL.
func CheckLinks(
	ctx context.Context,
	inputs []*url.URL,
	httpFetcher *fetcher.Fetcher,
	cache *resource.Cache,
	options Options,
) ([]LinkResult, error) {
	if err := validate(ctx, httpFetcher, options); err != nil {
		return nil, err
	}

	groups, resultCount, err := groupLinks(inputs)
	if err != nil {
		return nil, err
	}
	results := make([]LinkResult, resultCount)
	callback := serializedCallback{function: options.OnComplete}

	err = runWorkers(ctx, len(groups), options.Concurrency, func(workerContext context.Context, index int) error {
		group := groups[index]
		resourceResult := resource.Result{}
		if urlutil.IsHTTPWithHost(group.requestURL) {
			var checkErr error
			resourceResult, checkErr = resource.Check(
				workerContext,
				group.requestURL,
				httpFetcher,
				cache,
			)
			if checkErr != nil {
				return checkErr
			}
		}

		for _, alias := range group.aliases {
			result := linkResult(alias.url, group.requestURL, resourceResult)
			results[alias.order] = result

			if err := callback.call(workerContext, Completion{
				Target: TargetLink,
				Key:    result.URL,
				URL:    result.URL,
			}); err != nil {
				return err
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}

	return results, nil
}

// CheckImages checks one result for each distinct ImageReferenceKey. Results
// preserve the first occurrence order.
func CheckImages(
	ctx context.Context,
	references []page.ImageReference,
	httpFetcher *fetcher.Fetcher,
	cache *resource.Cache,
	options Options,
) ([]ImageResult, error) {
	if err := validate(ctx, httpFetcher, options); err != nil {
		return nil, err
	}

	groups := groupImages(references)
	results := make([]ImageResult, len(groups))
	callback := serializedCallback{function: options.OnComplete}

	err := runWorkers(ctx, len(groups), options.Concurrency, func(workerContext context.Context, index int) error {
		group := groups[index]
		result := ImageResult{
			Key: group.key,
			URL: group.displayURL,
		}

		switch {
		case group.reference.URL == nil:
			result.Kind = ResultInvalid
			result.Reason = ReasonInvalidURL
		case !urlutil.IsHTTPWithHost(group.reference.URL):
			result.Kind = ResultSkipped
			result.Reason = ReasonUnsupportedProtocol
		default:
			resourceResult, checkErr := resource.Check(
				workerContext,
				group.reference.URL,
				httpFetcher,
				cache,
			)
			if checkErr != nil {
				return checkErr
			}
			result = imageResult(group, resourceResult)
		}

		results[index] = result
		return callback.call(workerContext, Completion{
			Target: TargetImage,
			Key:    result.Key,
			URL:    result.URL,
		})
	})
	if err != nil {
		return nil, err
	}

	return results, nil
}

// ImageReferenceKey returns the stable de-duplication key for a parsed image
// reference.
func ImageReferenceKey(reference page.ImageReference) string {
	if reference.URL == nil {
		return "invalid:" + reference.OriginalURL
	}
	return resource.Key(reference.URL)
}

func validate(ctx context.Context, httpFetcher *fetcher.Fetcher, options Options) error {
	if ctx == nil {
		return ErrNilContext
	}
	if cause := context.Cause(ctx); cause != nil {
		return cause
	}
	if options.Concurrency <= 0 {
		return ErrInvalidConcurrency
	}
	if httpFetcher == nil {
		return ErrNilFetcher
	}
	return nil
}

type linkAlias struct {
	url   string
	order int
}

type linkGroup struct {
	requestURL *url.URL
	aliases    []linkAlias
}

func groupLinks(inputs []*url.URL) ([]linkGroup, int, error) {
	groups := make([]linkGroup, 0)
	groupIndexes := make(map[string]int)
	seenAliases := make(map[string]struct{})
	resultCount := 0

	for index, input := range inputs {
		if input == nil {
			return nil, 0, fmt.Errorf("%w at index %d", ErrNilLink, index)
		}

		aliasURL := input.String()
		if _, seen := seenAliases[aliasURL]; seen {
			continue
		}
		seenAliases[aliasURL] = struct{}{}

		requestURL := resource.RequestURL(input)
		requestKey := requestURL.String()
		alias := linkAlias{url: aliasURL, order: resultCount}
		resultCount++

		groupIndex, exists := groupIndexes[requestKey]
		if !exists {
			groupIndexes[requestKey] = len(groups)
			groups = append(groups, linkGroup{
				requestURL: requestURL,
				aliases:    []linkAlias{alias},
			})
			continue
		}

		groups[groupIndex].aliases = append(groups[groupIndex].aliases, alias)
	}

	return groups, resultCount, nil
}

func linkResult(aliasURL string, requestURL *url.URL, checked resource.Result) LinkResult {
	result := LinkResult{
		URL:        aliasURL,
		RequestURL: requestURL.String(),
	}

	if !urlutil.IsHTTPWithHost(requestURL) {
		result.Kind = ResultSkipped
		result.Reason = ReasonUnsupportedProtocol
		return result
	}

	if checked.Kind == resource.KindReachable {
		result.Kind = ResultReachable
		result.StatusCode = checked.StatusCode
		result.FinalURL = urlutil.String(checked.FinalURL)
		return result
	}
	if checked.Kind == resource.KindBlocked {
		result.Kind = ResultBlocked
		result.StatusCode = checked.StatusCode
		result.FinalURL = urlutil.String(checked.FinalURL)
		result.Failure = checked.Failure
		return result
	}

	result.Kind = ResultFailed
	result.Failure = checked.Failure
	return result
}

type imageGroup struct {
	key        string
	displayURL string
	reference  page.ImageReference
}

func groupImages(references []page.ImageReference) []imageGroup {
	groups := make([]imageGroup, 0)
	seen := make(map[string]struct{})

	for _, reference := range references {
		key := ImageReferenceKey(reference)
		if _, exists := seen[key]; exists {
			continue
		}
		seen[key] = struct{}{}

		displayURL := reference.OriginalURL
		if reference.URL != nil {
			displayURL = resource.Key(reference.URL)
		}
		groups = append(groups, imageGroup{
			key:        key,
			displayURL: displayURL,
			reference:  reference,
		})
	}

	return groups
}

func imageResult(group imageGroup, checked resource.Result) ImageResult {
	result := ImageResult{
		Key: group.key,
		URL: group.displayURL,
	}

	if checked.Kind == resource.KindReachable {
		result.Kind = ResultReachable
		result.StatusCode = checked.StatusCode
		result.FinalURL = urlutil.String(checked.FinalURL)
		result.ContentType = ptrutil.Clone(checked.ContentType)
		return result
	}
	if checked.Kind == resource.KindBlocked {
		result.Kind = ResultBlocked
		result.StatusCode = checked.StatusCode
		result.FinalURL = urlutil.String(checked.FinalURL)
		result.ContentType = ptrutil.Clone(checked.ContentType)
		result.Failure = checked.Failure
		return result
	}

	result.Kind = ResultFailed
	result.Failure = checked.Failure
	return result
}

type serializedCallback struct {
	function func(Completion) error

	mutex sync.Mutex
	err   error
}

func (callback *serializedCallback) call(ctx context.Context, completion Completion) error {
	if cause := context.Cause(ctx); cause != nil {
		return cause
	}
	if callback.function == nil {
		return nil
	}

	callback.mutex.Lock()
	defer callback.mutex.Unlock()

	if cause := context.Cause(ctx); cause != nil {
		return cause
	}
	if callback.err != nil {
		return callback.err
	}
	callback.err = callback.function(completion)
	return callback.err
}

func runWorkers(
	ctx context.Context,
	jobCount int,
	concurrency int,
	work func(context.Context, int) error,
) error {
	if jobCount == 0 {
		return nil
	}

	workerContext, cancel := context.WithCancelCause(ctx)
	defer cancel(nil)

	jobs := make(chan int)
	workerCount := min(concurrency, jobCount)
	var workers sync.WaitGroup
	workers.Add(workerCount)

	for range workerCount {
		go func() {
			defer workers.Done()

			for {
				select {
				case <-workerContext.Done():
					return
				case index, ok := <-jobs:
					if !ok {
						return
					}
					if err := work(workerContext, index); err != nil {
						cancel(err)
						return
					}
				}
			}
		}()
	}

enqueue:
	for index := range jobCount {
		select {
		case jobs <- index:
		case <-workerContext.Done():
			break enqueue
		}
	}
	close(jobs)
	workers.Wait()

	return context.Cause(workerContext)
}
