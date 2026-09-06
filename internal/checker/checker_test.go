package checker

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"sort"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/resource"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestCheckLinksDeduplicatesAndPreservesOrder(t *testing.T) {
	t.Parallel()

	site := &staticCheckerSite{
		statuses: map[string]int{"/docs": http.StatusNotFound},
	}
	server := httptest.NewServer(site)
	defer server.Close()

	var completionsMutex sync.Mutex
	completions := make([]Completion, 0)
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	cache := resource.NewCache()
	options := Options{
		Concurrency: 3,
		OnComplete: func(completion Completion) error {
			completionsMutex.Lock()
			defer completionsMutex.Unlock()
			completions = append(completions, completion)
			return nil
		},
	}

	results, err := CheckLinks(context.Background(), []*url.URL{
		testutil.ParseURL(t, server.URL+"/docs"),
		testutil.ParseURL(t, server.URL+"/docs"),
		testutil.ParseURL(t, server.URL+"/page#intro"),
		testutil.ParseURL(t, server.URL+"/page#api"),
		testutil.ParseURL(t, "mailto:hello@example.test"),
	}, httpFetcher, cache, options)
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}

	wantURLs := []string{
		server.URL + "/docs",
		server.URL + "/page#intro",
		server.URL + "/page#api",
		"mailto:hello@example.test",
	}
	gotURLs := make([]string, 0, len(results))
	for _, result := range results {
		gotURLs = append(gotURLs, result.URL)
	}
	if !reflect.DeepEqual(gotURLs, wantURLs) {
		t.Errorf("result URLs = %#v, want %#v", gotURLs, wantURLs)
	}
	if got := results[0].StatusCode; got != http.StatusNotFound {
		t.Errorf("docs status = %d, want %d", got, http.StatusNotFound)
	}
	for _, index := range []int{1, 2} {
		if got, want := results[index].RequestURL, server.URL+"/page"; got != want {
			t.Errorf("results[%d].RequestURL = %q, want %q", index, got, want)
		}
	}
	if results[3].Kind != ResultSkipped || results[3].Reason != ReasonUnsupportedProtocol {
		t.Errorf("mailto result = %#v", results[3])
	}

	site.mutex.Lock()
	gotRequests := append([]string(nil), site.requested...)
	site.mutex.Unlock()
	sort.Strings(gotRequests)
	if want := []string{"/docs", "/page"}; !reflect.DeepEqual(gotRequests, want) {
		t.Errorf("requested paths = %#v, want %#v", gotRequests, want)
	}

	completionsMutex.Lock()
	gotCompletions := append([]Completion(nil), completions...)
	completionsMutex.Unlock()
	assertUniqueCompletions(t, gotCompletions, TargetLink, wantURLs)
}

func TestCheckLinksClassifiesNetworkFailures(t *testing.T) {
	t.Parallel()

	blockingServer := httptest.NewServer(blockingCheckerSite{})
	defer blockingServer.Close()
	timeoutFetcher := testutil.NewFetcher(t, fetcher.Options{Timeout: 25 * time.Millisecond})

	results, err := CheckLinks(context.Background(), []*url.URL{
		testutil.ParseURL(t, blockingServer.URL),
	}, timeoutFetcher, nil, Options{Concurrency: 1})
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}
	if results[0].Kind != ResultFailed || results[0].Failure != resource.FailureRequestTimedOut {
		t.Fatalf("timeout result = %#v", results[0])
	}

	closedServer := httptest.NewServer(&staticCheckerSite{})
	closedURL := closedServer.URL
	closedServer.Close()
	results, err = CheckLinks(context.Background(), []*url.URL{
		testutil.ParseURL(t, closedURL),
	}, testutil.NewFetcher(t, fetcher.Options{}), nil, Options{Concurrency: 1})
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}
	if results[0].Kind != ResultFailed || results[0].Failure != resource.FailureConnectionFailed {
		t.Fatalf("connection result = %#v", results[0])
	}
}

func TestChecksPropagateAntiBotChallenges(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		response.Header().Set("Cf-Mitigated", "challenge")
		response.Header().Set("Content-Type", "image/png")
		response.WriteHeader(http.StatusForbidden)
	}))
	defer server.Close()
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})

	links, err := CheckLinks(
		context.Background(),
		[]*url.URL{testutil.ParseURL(t, server.URL)},
		httpFetcher,
		nil,
		Options{Concurrency: 1},
	)
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}
	if got := links[0]; got.Kind != ResultBlocked ||
		got.StatusCode != http.StatusForbidden ||
		got.FinalURL != server.URL+"/" ||
		got.Failure != resource.FailureAntiBotChallenge {
		t.Fatalf("blocked link = %#v", got)
	}

	images, err := CheckImages(
		context.Background(),
		[]page.ImageReference{imageReference(t, server.URL)},
		httpFetcher,
		nil,
		Options{Concurrency: 1},
	)
	if err != nil {
		t.Fatalf("CheckImages() error = %v", err)
	}
	if got := images[0]; got.Kind != ResultBlocked ||
		got.StatusCode != http.StatusForbidden ||
		got.FinalURL != server.URL+"/" ||
		got.ContentType == nil ||
		*got.ContentType != "image/png" ||
		got.Failure != resource.FailureAntiBotChallenge {
		t.Fatalf("blocked image = %#v", got)
	}
}

func TestCheckImagesClassifiesAndDeduplicatesReferences(t *testing.T) {
	t.Parallel()

	site := &staticCheckerSite{contentTypes: map[string]string{"/image.svg": "image/svg+xml"}}
	server := httptest.NewServer(site)
	defer server.Close()

	results, err := CheckImages(context.Background(), []page.ImageReference{
		imageReference(t, server.URL+"/image.svg#first"),
		imageReference(t, server.URL+"/image.svg#second"),
		imageReference(t, "data:image/png;base64,AAAA"),
		{
			OriginalURL: "https://[invalid",
			Element:     page.ImageReferenceElementImage,
			Attribute:   page.ImageReferenceAttributeSrc,
		},
	}, testutil.NewFetcher(t, fetcher.Options{}), nil, Options{
		Concurrency: 3,
	})
	if err != nil {
		t.Fatalf("CheckImages() error = %v", err)
	}
	if got, want := len(results), 3; got != want {
		t.Fatalf("len(results) = %d, want %d", got, want)
	}
	if results[0].Kind != ResultReachable ||
		results[0].ContentType == nil ||
		*results[0].ContentType != "image/svg+xml" {
		t.Errorf("HTTP result = %#v", results[0])
	}
	if results[1].Kind != ResultSkipped || results[1].Reason != ReasonUnsupportedProtocol {
		t.Errorf("unsupported result = %#v", results[1])
	}
	if results[2].Kind != ResultInvalid || results[2].Reason != ReasonInvalidURL {
		t.Errorf("invalid result = %#v", results[2])
	}

	site.mutex.Lock()
	requestCount := len(site.requested)
	site.mutex.Unlock()
	if requestCount != 1 {
		t.Errorf("request count = %d, want 1", requestCount)
	}
}

func TestLinkAndImageChecksShareResourceCache(t *testing.T) {
	t.Parallel()

	site := &staticCheckerSite{contentTypes: map[string]string{"/shared.png": "image/png"}}
	server := httptest.NewServer(site)
	defer server.Close()

	cache := resource.NewCache()
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	options := Options{
		Concurrency: 2,
	}
	linkResults, err := CheckLinks(context.Background(), []*url.URL{
		testutil.ParseURL(t, server.URL+"/shared.png#link"),
	}, httpFetcher, cache, options)
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}
	imageResults, err := CheckImages(context.Background(), []page.ImageReference{
		imageReference(t, server.URL+"/shared.png#image"),
	}, httpFetcher, cache, options)
	if err != nil {
		t.Fatalf("CheckImages() error = %v", err)
	}

	site.mutex.Lock()
	requestCount := len(site.requested)
	site.mutex.Unlock()
	if requestCount != 1 {
		t.Errorf("request count = %d, want 1", requestCount)
	}
	if linkResults[0].FinalURL != server.URL+"/shared.png" {
		t.Errorf("link FinalURL = %q", linkResults[0].FinalURL)
	}
	if imageResults[0].ContentType == nil || *imageResults[0].ContentType != "image/png" {
		t.Errorf("image ContentType = %v", imageResults[0].ContentType)
	}
}

func TestChecksUseBoundedConcurrencyAndStableOutput(t *testing.T) {
	t.Parallel()

	site := &gatedCheckerSite{
		started: make(chan struct{}, 3),
		release: make(chan struct{}),
	}
	server := httptest.NewServer(site)
	defer server.Close()

	inputs := []*url.URL{
		testutil.ParseURL(t, server.URL+"/first"),
		testutil.ParseURL(t, server.URL+"/second"),
		testutil.ParseURL(t, server.URL+"/third"),
	}
	type checkResult struct {
		results []LinkResult
		err     error
	}
	done := make(chan checkResult, 1)
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	go func() {
		results, err := CheckLinks(context.Background(), inputs, httpFetcher, nil, Options{
			Concurrency: 2,
		})
		done <- checkResult{results: results, err: err}
	}()

	for range 2 {
		select {
		case <-site.started:
		case <-time.After(time.Second):
			t.Fatal("workers did not start")
		}
	}
	select {
	case <-site.started:
		t.Fatal("third request started before a worker became available")
	case <-time.After(25 * time.Millisecond):
	}
	close(site.release)

	checked := <-done
	if checked.err != nil {
		t.Fatalf("CheckLinks() error = %v", checked.err)
	}
	if got := site.maximum.Load(); got != 2 {
		t.Errorf("maximum concurrency = %d, want 2", got)
	}
	for index, result := range checked.results {
		if result.URL != inputs[index].String() {
			t.Errorf("results[%d].URL = %q, want %q", index, result.URL, inputs[index])
		}
	}
}

func TestCallbackCallsAreSerialized(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(&staticCheckerSite{})
	defer server.Close()

	var active atomic.Int32
	var maximum atomic.Int32
	var calls atomic.Int32
	inputs := make([]*url.URL, 0, 8)
	for index := range 8 {
		inputs = append(inputs, testutil.ParseURL(t, server.URL+"/"+string(rune('a'+index))))
	}

	_, err := CheckLinks(context.Background(), inputs, testutil.NewFetcher(t, fetcher.Options{}), nil, Options{
		Concurrency: 4,
		OnComplete: func(Completion) error {
			current := active.Add(1)
			updateMaximum(&maximum, current)
			time.Sleep(time.Millisecond)
			active.Add(-1)
			calls.Add(1)
			return nil
		},
	})
	if err != nil {
		t.Fatalf("CheckLinks() error = %v", err)
	}
	if int(calls.Load()) != len(inputs) {
		t.Errorf("callback calls = %d, want %d", calls.Load(), len(inputs))
	}
	if maximum.Load() != 1 {
		t.Errorf("maximum concurrent callbacks = %d, want 1", maximum.Load())
	}
}

func TestCallbackErrorCancelsOutstandingWork(t *testing.T) {
	t.Parallel()

	site := &callbackCancellationSite{
		triggerStarted:  make(chan struct{}),
		blockedStarted:  make(chan struct{}),
		releaseTrigger:  make(chan struct{}),
		blockedCanceled: make(chan struct{}),
	}
	server := httptest.NewServer(site)
	defer server.Close()
	callbackError := errors.New("stop progress")
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})

	result := make(chan error, 1)
	go func() {
		_, err := CheckLinks(context.Background(), []*url.URL{
			testutil.ParseURL(t, server.URL+"/trigger"),
			testutil.ParseURL(t, server.URL+"/blocked"),
		}, httpFetcher, nil, Options{
			Concurrency: 2,
			OnComplete: func(completion Completion) error {
				if completion.URL == server.URL+"/trigger" {
					return callbackError
				}
				return nil
			},
		})
		result <- err
	}()

	for _, started := range []<-chan struct{}{site.triggerStarted, site.blockedStarted} {
		select {
		case <-started:
		case <-time.After(time.Second):
			t.Fatal("concurrent work did not start")
		}
	}
	close(site.releaseTrigger)

	select {
	case err := <-result:
		if !errors.Is(err, callbackError) {
			t.Fatalf("CheckLinks() error = %v, want callback error", err)
		}
	case <-time.After(time.Second):
		t.Fatal("CheckLinks() did not stop after callback error")
	}
	select {
	case <-site.blockedCanceled:
	case <-time.After(time.Second):
		t.Fatal("outstanding request was not canceled")
	}
}

func TestCallerCancellationStopsWorkers(t *testing.T) {
	t.Parallel()

	site := &cancelCheckerSite{started: make(chan struct{})}
	server := httptest.NewServer(site)
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})

	go func() {
		_, err := CheckImages(ctx, []page.ImageReference{
			imageReference(t, server.URL+"/image.png"),
		}, httpFetcher, nil, Options{
			Concurrency: 1,
		})
		result <- err
	}()

	<-site.started
	cancel()
	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatalf("CheckImages() error = %v, want context canceled", err)
	}
}

func TestChecksValidateInputs(t *testing.T) {
	t.Parallel()

	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	tests := []struct {
		name    string
		ctx     context.Context
		inputs  []*url.URL
		fetcher *fetcher.Fetcher
		options Options
		want    error
	}{
		{name: "nil context", fetcher: httpFetcher, options: Options{Concurrency: 1}, want: ErrNilContext},
		{name: "zero concurrency", ctx: context.Background(), fetcher: httpFetcher, want: ErrInvalidConcurrency},
		{name: "nil fetcher", ctx: context.Background(), options: Options{Concurrency: 1}, want: ErrNilFetcher},
		{name: "nil link", ctx: context.Background(), inputs: []*url.URL{nil}, fetcher: httpFetcher, options: Options{Concurrency: 1}, want: ErrNilLink},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			_, err := CheckLinks(test.ctx, test.inputs, test.fetcher, nil, test.options)
			if !errors.Is(err, test.want) {
				t.Fatalf("CheckLinks() error = %v, want %v", err, test.want)
			}
		})
	}

	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := CheckImages(canceled, nil, httpFetcher, nil, Options{Concurrency: 1}); !errors.Is(err, context.Canceled) {
		t.Fatalf("CheckImages() error = %v, want context.Canceled", err)
	}
}

func TestEmptyInputsReturnNonNilSlices(t *testing.T) {
	t.Parallel()

	options := Options{
		Concurrency: 1,
	}
	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	links, err := CheckLinks(context.Background(), nil, httpFetcher, nil, options)
	if err != nil || links == nil || len(links) != 0 {
		t.Errorf("CheckLinks() = %#v, %v", links, err)
	}
	images, err := CheckImages(context.Background(), nil, httpFetcher, nil, options)
	if err != nil || images == nil || len(images) != 0 {
		t.Errorf("CheckImages() = %#v, %v", images, err)
	}
}

func TestSerializedCallbackRechecksCancellationAfterLock(t *testing.T) {
	ctx := &cancelOnSecondCheckContext{}
	calls := 0
	callback := serializedCallback{
		function: func(Completion) error {
			calls++
			return nil
		},
	}
	err := callback.call(ctx, Completion{})
	if !errors.Is(err, context.Canceled) || calls != 0 {
		t.Fatalf("call() = %v, calls = %d", err, calls)
	}
}

func TestImageResultAndSerializedCallbackFailurePaths(t *testing.T) {
	t.Parallel()

	checked := imageResult(imageGroup{key: "image", displayURL: "image.png"}, resource.Result{
		Kind: resource.KindFailed, Failure: resource.FailureConnectionFailed,
	})
	if checked.Kind != ResultFailed || checked.Failure != resource.FailureConnectionFailed {
		t.Fatalf("imageResult() = %#v", checked)
	}

	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	callback := serializedCallback{function: func(Completion) error { return nil }}
	if err := callback.call(canceled, Completion{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled callback.call() = %v", err)
	}

	want := errors.New("callback failed")
	callback = serializedCallback{function: func(Completion) error { return want }}
	if err := callback.call(context.Background(), Completion{}); !errors.Is(err, want) {
		t.Fatalf("first callback.call() = %v", err)
	}
	if err := callback.call(context.Background(), Completion{}); !errors.Is(err, want) {
		t.Fatalf("cached callback.call() = %v", err)
	}
}

func TestRunWorkersStopsForCanceledContext(t *testing.T) {
	t.Parallel()

	want := errors.New("stop workers")
	ctx, cancel := context.WithCancelCause(context.Background())
	cancel(want)
	err := runWorkers(ctx, 4, 2, func(workerContext context.Context, _ int) error {
		return context.Cause(workerContext)
	})
	if !errors.Is(err, want) {
		t.Fatalf("runWorkers() error = %v, want %v", err, want)
	}
}

type staticCheckerSite struct {
	mutex        sync.Mutex
	requested    []string
	statuses     map[string]int
	contentTypes map[string]string
}

func (site *staticCheckerSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	if site == nil {
		response.WriteHeader(http.StatusOK)
		return
	}
	site.mutex.Lock()
	site.requested = append(site.requested, request.URL.Path)
	status := site.statuses[request.URL.Path]
	contentType := site.contentTypes[request.URL.Path]
	site.mutex.Unlock()
	if status == 0 {
		status = http.StatusOK
	}
	if contentType != "" {
		response.Header().Set("Content-Type", contentType)
	}
	response.WriteHeader(status)
}

type blockingCheckerSite struct{}

func (blockingCheckerSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	<-request.Context().Done()
}

type gatedCheckerSite struct {
	active  atomic.Int32
	maximum atomic.Int32
	started chan struct{}
	release chan struct{}
}

func (site *gatedCheckerSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	current := site.active.Add(1)
	defer site.active.Add(-1)
	updateMaximum(&site.maximum, current)
	site.started <- struct{}{}
	select {
	case <-site.release:
		response.WriteHeader(http.StatusOK)
	case <-request.Context().Done():
	}
}

type callbackCancellationSite struct {
	triggerStarted  chan struct{}
	blockedStarted  chan struct{}
	releaseTrigger  chan struct{}
	blockedCanceled chan struct{}
}

func (site *callbackCancellationSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	switch request.URL.Path {
	case "/trigger":
		close(site.triggerStarted)
		<-site.releaseTrigger
		response.WriteHeader(http.StatusOK)
	case "/blocked":
		close(site.blockedStarted)
		<-request.Context().Done()
		close(site.blockedCanceled)
	default:
		http.NotFound(response, request)
	}
}

type cancelCheckerSite struct {
	started chan struct{}
}

func (site *cancelCheckerSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	close(site.started)
	<-request.Context().Done()
}

type cancelOnSecondCheckContext struct {
	checks atomic.Int32
}

func (*cancelOnSecondCheckContext) Deadline() (time.Time, bool) { return time.Time{}, false }
func (*cancelOnSecondCheckContext) Done() <-chan struct{}       { return nil }
func (*cancelOnSecondCheckContext) Value(any) any               { return nil }

func (ctx *cancelOnSecondCheckContext) Err() error {
	if ctx.checks.Add(1) >= 2 {
		return context.Canceled
	}
	return nil
}

func imageReference(t *testing.T, value string) page.ImageReference {
	t.Helper()
	return page.ImageReference{
		URL:         testutil.ParseURL(t, value),
		OriginalURL: value,
		Element:     page.ImageReferenceElementImage,
		Attribute:   page.ImageReferenceAttributeSrc,
	}
}

func updateMaximum(maximum *atomic.Int32, value int32) {
	for {
		previous := maximum.Load()
		if previous >= value || maximum.CompareAndSwap(previous, value) {
			return
		}
	}
}

func assertUniqueCompletions(t *testing.T, completions []Completion, target Target, wantURLs []string) {
	t.Helper()
	gotURLs := make([]string, 0, len(completions))
	seen := make(map[string]int)
	for _, completion := range completions {
		if completion.Target != target {
			t.Errorf("completion target = %q, want %q", completion.Target, target)
		}
		gotURLs = append(gotURLs, completion.URL)
		seen[completion.URL]++
	}
	sort.Strings(gotURLs)
	sortedWant := append([]string(nil), wantURLs...)
	sort.Strings(sortedWant)
	if !reflect.DeepEqual(gotURLs, sortedWant) {
		t.Errorf("completion URLs = %#v, want %#v", gotURLs, sortedWant)
	}
	for displayURL, count := range seen {
		if count != 1 {
			t.Errorf("completion URL %q called %d times, want once", displayURL, count)
		}
	}
}
