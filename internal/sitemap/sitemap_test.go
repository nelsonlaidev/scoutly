package sitemap

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestParseURLSetPreservesDocumentOrder(t *testing.T) {
	t.Parallel()

	document := strings.Join([]string{
		`<?xml version="1.0" encoding="UTF-8"?>`,
		`<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">`,
		`<url><loc>https://example.com/products?one=1&amp;two=2</loc><lastmod>2026-01-01</lastmod></url>`,
		`<url><loc>ftp://example.com/ignored</loc></url>`,
		`<url><loc>/relative</loc></url>`,
		`<url><loc>https://example.com/fragment#details</loc></url>`,
		`</urlset>`,
	}, "")

	parsed, err := Parse(strings.NewReader(document))
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if parsed.Kind != KindURLSet {
		t.Fatalf("Kind = %v, want KindURLSet", parsed.Kind)
	}
	want := []string{
		"https://example.com/products?one=1&two=2",
		"https://example.com/fragment#details",
	}
	if got := testutil.URLStrings(parsed.URLs); !reflect.DeepEqual(got, want) {
		t.Fatalf("URLs = %#v, want %#v", got, want)
	}
}

func TestParsePrefixedIndex(t *testing.T) {
	t.Parallel()

	document := strings.Join([]string{
		`<sm:sitemapindex xmlns:sm="http://www.sitemaps.org/schemas/sitemap/0.9">`,
		`<sm:sitemap><sm:loc>https://sitemaps.example/one.xml</sm:loc></sm:sitemap>`,
		`<sm:sitemap><sm:loc>https://sitemaps.example/two.xml.gz</sm:loc></sm:sitemap>`,
		`</sm:sitemapindex>`,
	}, "")
	parsed, err := Parse(strings.NewReader(document))
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if parsed.Kind != KindIndex {
		t.Fatalf("Kind = %v, want KindIndex", parsed.Kind)
	}
	want := []string{
		"https://sitemaps.example/one.xml",
		"https://sitemaps.example/two.xml.gz",
	}
	if got := testutil.URLStrings(parsed.URLs); !reflect.DeepEqual(got, want) {
		t.Fatalf("URLs = %#v, want %#v", got, want)
	}
}

func TestParseRejectsUnsafeOrInvalidDocuments(t *testing.T) {
	t.Parallel()

	deep := "<urlset>" + strings.Repeat("<x>", maxXMLDepth) + strings.Repeat("</x>", maxXMLDepth) + "</urlset>"
	tooManyEntries := "<urlset>" + strings.Repeat("<url/>", maxEntries+1) + "</urlset>"
	tests := []struct {
		name     string
		document string
		want     error
	}{
		{name: "nil reader", document: "", want: nil},
		{name: "doctype", document: `<!DOCTYPE urlset [<!ENTITY x "x">]><urlset/>`, want: ErrDOCTYPE},
		{name: "malformed", document: `<urlset><url></urlset>`, want: ErrMalformed},
		{name: "unsupported root", document: `<rss><channel/></rss>`, want: ErrUnsupportedRoot},
		{name: "multiple roots", document: `<urlset></urlset><urlset/>`, want: ErrMalformed},
		{name: "character data outside root", document: `<urlset/>not-xml`, want: ErrMalformed},
		{name: "too deep", document: deep, want: ErrTooDeep},
		{name: "too many entries", document: tooManyEntries, want: ErrTooManyEntries},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			var reader io.Reader = strings.NewReader(test.document)
			if test.name == "nil reader" {
				reader = nil
				test.want = ErrMalformed
			}
			_, err := Parse(reader)
			if !errors.Is(err, test.want) {
				t.Fatalf("Parse() error = %v, want %v", err, test.want)
			}
		})
	}
}

func TestParseIgnoresWhitespaceAndInvalidNestedLocations(t *testing.T) {
	t.Parallel()

	document := `
		<urlset>
			<url><loc><nested>https://example.com/ignored</nested></loc></url>
			<url><loc>https://example.com/kept</loc></url>
		</urlset>
	`
	parsed, err := Parse(strings.NewReader(document))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := testutil.URLStrings(parsed.URLs), []string{"https://example.com/kept"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("URLs = %#v, want %#v", got, want)
	}
}

func TestParseRejectsDocumentWithoutRoot(t *testing.T) {
	t.Parallel()
	if _, err := Parse(strings.NewReader(" \n\t")); !errors.Is(err, ErrMalformed) {
		t.Fatalf("Parse() error = %v, want ErrMalformed", err)
	}
}

func TestCrawlValidatesArgumentsAndCancellationBoundaries(t *testing.T) {
	t.Parallel()

	httpFetcher := testutil.NewFetcher(t, fetcher.Options{})
	options := Options{MaxDocuments: 1, MaxPageURLs: 1}
	//nolint:staticcheck // Nil intentionally verifies the public boundary guard.
	if _, err := Crawl(nil, nil, httpFetcher, options); !errors.Is(err, ErrNilContext) {
		t.Fatalf("Crawl(nil) error = %v", err)
	}
	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Crawl(canceled, nil, httpFetcher, options); !errors.Is(err, context.Canceled) {
		t.Fatalf("Crawl(canceled) error = %v", err)
	}
	if _, err := Crawl(context.Background(), nil, nil, options); !errors.Is(err, ErrNilFetcher) {
		t.Fatalf("Crawl(nil fetcher) error = %v", err)
	}

	ctx, cancelFinal := context.WithCancel(context.Background())
	initial := []*url.URL{testutil.ParseURL(t, "https://example.com/sitemap.xml")}
	pages, err := Crawl(ctx, initial, httpFetcher, Options{
		MaxDocuments: 1,
		MaxPageURLs:  1,
		Allow: func(*url.URL) bool {
			cancelFinal()
			return false
		},
	})
	if !errors.Is(err, context.Canceled) || pages != nil {
		t.Fatalf("final cancellation = %#v, %v", pages, err)
	}

	ctx, cancelBeforeLoop := context.WithCancel(context.Background())
	_, err = Crawl(ctx, initial, httpFetcher, Options{
		MaxDocuments: 1,
		MaxPageURLs:  1,
		Allow: func(*url.URL) bool {
			cancelBeforeLoop()
			return true
		},
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("loop cancellation error = %v", err)
	}
}

func TestCrawlReturnsCancellationFromDocumentCallback(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(&staticSitemapSite{body: testutil.URLSet("https://example.com/page")})
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	_, err := Crawl(ctx, []*url.URL{testutil.ParseURL(t, server.URL)}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Scheme: server.URL[:4], Host: testutil.ParseURL(t, server.URL).Host,
		MaxDocuments: 1,
		MaxPageURLs:  1,
		OnDocumentFetched: func(*url.URL) {
			cancel()
		},
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Crawl() error = %v", err)
	}
}

func TestCrawlRecursesBreadthFirstAndFiltersPages(t *testing.T) {
	t.Parallel()

	site := &recursiveSitemapSite{}
	server := httptest.NewServer(site)
	defer server.Close()
	site.baseURL = server.URL
	base := testutil.ParseURL(t, server.URL)

	var fetched []string
	pages, err := Crawl(context.Background(), []*url.URL{
		testutil.ParseURL(t, server.URL+"/root.xml"),
	}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Allow: func(candidate *url.URL) bool {
			return candidate.Path != "/blocked"
		},
		Scheme:           base.Scheme,
		Host:             base.Host,
		ExcludedPageKeys: map[string]struct{}{server.URL + "/already": {}},
		MaxDocuments:     10,
		MaxPageURLs:      10,
		OnDocumentFetched: func(document *url.URL) {
			fetched = append(fetched, document.String())
		},
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	wantPages := []string{server.URL + "/one", server.URL + "/two"}
	if got := testutil.URLStrings(pages); !reflect.DeepEqual(got, wantPages) {
		t.Fatalf("pages = %#v, want %#v", got, wantPages)
	}
	wantFetched := []string{
		server.URL + "/root.xml",
		server.URL + "/pages.xml.gz",
		server.URL + "/nested.xml",
		server.URL + "/final.xml",
	}
	if !reflect.DeepEqual(fetched, wantFetched) {
		t.Fatalf("fetched = %#v, want %#v", fetched, wantFetched)
	}
}

func TestCrawlKeepsFragmentsWhenConfigured(t *testing.T) {
	t.Parallel()

	site := &staticSitemapSite{}
	server := httptest.NewServer(site)
	defer server.Close()
	site.body = testutil.URLSet(server.URL+"/page#one", server.URL+"/page#two")
	base := testutil.ParseURL(t, server.URL)

	pages, err := Crawl(context.Background(), []*url.URL{
		testutil.ParseURL(t, server.URL+"/root.xml"),
	}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Scheme:        base.Scheme,
		Host:          base.Host,
		KeepFragments: true,
		MaxDocuments:  1,
		MaxPageURLs:   10,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	want := []string{server.URL + "/page#one", server.URL + "/page#two"}
	if got := testutil.URLStrings(pages); !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %#v, want %#v", got, want)
	}
}

func TestCrawlOnlyReturnsPagesFromConfiguredOrigin(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(&staticSitemapSite{body: testutil.URLSet(
		"https://example.com/secure",
		"http://example.com/insecure",
	)})
	defer server.Close()

	pages, err := Crawl(
		context.Background(),
		[]*url.URL{testutil.ParseURL(t, server.URL)},
		testutil.NewFetcher(t, fetcher.Options{}),
		Options{
			Scheme:       "https",
			Host:         "example.com",
			MaxDocuments: 1,
			MaxPageURLs:  10,
		},
	)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := testutil.URLStrings(pages), []string{"https://example.com/secure"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("pages = %v, want %v", got, want)
	}
}

func TestCrawlEnforcesBudgets(t *testing.T) {
	t.Parallel()

	site := &budgetSitemapSite{}
	server := httptest.NewServer(site)
	defer server.Close()
	site.baseURL = server.URL
	base := testutil.ParseURL(t, server.URL)

	pages, err := Crawl(context.Background(), []*url.URL{
		testutil.ParseURL(t, server.URL+"/root.xml"),
	}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Scheme:       base.Scheme,
		Host:         base.Host,
		MaxDocuments: 2,
		MaxPageURLs:  1,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	if want := []string{server.URL + "/one"}; !reflect.DeepEqual(testutil.URLStrings(pages), want) {
		t.Fatalf("pages = %#v, want %#v", testutil.URLStrings(pages), want)
	}
	site.mutex.Lock()
	fetched := append([]string(nil), site.fetched...)
	site.mutex.Unlock()
	if want := []string{"/root.xml", "/one.xml"}; !reflect.DeepEqual(fetched, want) {
		t.Fatalf("fetched = %#v, want %#v", fetched, want)
	}
}

func TestCrawlSkipsDiscoveryWithoutBudget(t *testing.T) {
	t.Parallel()

	pages, err := Crawl(context.Background(), []*url.URL{
		testutil.ParseURL(t, "https://example.com/sitemap.xml"),
	}, nil, Options{MaxDocuments: 0, MaxPageURLs: 10})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	if pages == nil || len(pages) != 0 {
		t.Fatalf("pages = %#v, want non-nil empty slice", pages)
	}
}

func TestCrawlContinuesAfterDocumentFailures(t *testing.T) {
	t.Parallel()

	site := &failureSitemapSite{}
	server := httptest.NewServer(site)
	defer server.Close()
	site.baseURL = server.URL
	base := testutil.ParseURL(t, server.URL)
	initial := []*url.URL{
		testutil.ParseURL(t, server.URL+"/not-found.xml"),
		testutil.ParseURL(t, server.URL+"/malformed.xml"),
		testutil.ParseURL(t, server.URL+"/corrupt.xml.gz"),
		testutil.ParseURL(t, server.URL+"/oversized.xml"),
		testutil.ParseURL(t, server.URL+"/valid.xml"),
	}

	pages, err := Crawl(context.Background(), initial, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Scheme:       base.Scheme,
		Host:         base.Host,
		MaxDocuments: 10,
		MaxPageURLs:  10,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	if want := []string{server.URL + "/valid"}; !reflect.DeepEqual(testutil.URLStrings(pages), want) {
		t.Fatalf("pages = %#v, want %#v", testutil.URLStrings(pages), want)
	}
}

func TestCrawlFiltersDocumentsThroughPolicy(t *testing.T) {
	t.Parallel()

	site := &filterSitemapSite{}
	server := httptest.NewServer(site)
	defer server.Close()
	site.baseURL = server.URL

	_, err := Crawl(context.Background(), []*url.URL{
		testutil.ParseURL(t, "ftp://sitemaps.example/invalid.xml"),
		testutil.ParseURL(t, server.URL+"/blocked.xml"),
		testutil.ParseURL(t, server.URL+"/root.xml"),
	}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		Allow: func(candidate *url.URL) bool {
			return candidate.Path != "/blocked.xml"
		},
		MaxDocuments: 2,
		MaxPageURLs:  10,
	})
	if err != nil {
		t.Fatalf("Crawl() error = %v", err)
	}
	site.mutex.Lock()
	fetched := append([]string(nil), site.fetched...)
	site.mutex.Unlock()
	if want := []string{"/root.xml", "/allowed.xml"}; !reflect.DeepEqual(fetched, want) {
		t.Fatalf("fetched = %#v, want %#v", fetched, want)
	}
}

func TestCrawlReturnsContextCancellation(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(blockingSitemapSite{})
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		<-time.After(25 * time.Millisecond)
		cancel()
	}()
	_, err := Crawl(ctx, []*url.URL{testutil.ParseURL(t, server.URL)}, testutil.NewFetcher(t, fetcher.Options{}), Options{
		MaxDocuments: 1,
		MaxPageURLs:  1,
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Crawl() error = %v, want context.Canceled", err)
	}
}

func TestFetchDocumentRejectsDecompressedBodyOverLimit(t *testing.T) {
	var compressed bytes.Buffer
	writer := gzip.NewWriter(&compressed)
	_, _ = io.WriteString(writer, "<urlset>")
	_, _ = io.CopyN(writer, repeatingReader(' '), maxDecompressedBytes)
	_, _ = io.WriteString(writer, "</urlset>")
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}

	server := httptest.NewServer(&binarySitemapSite{body: compressed.Bytes()})
	defer server.Close()
	_, err := fetchDocument(
		context.Background(),
		testutil.ParseURL(t, server.URL+"/sitemap.xml.gz"),
		testutil.NewFetcher(t, fetcher.Options{}),
	)
	if !errors.Is(err, ErrDecompressedSize) {
		t.Fatalf("fetchDocument() error = %v, want ErrDecompressedSize", err)
	}
}

func TestMaxBytesReaderEnforcesLimit(t *testing.T) {
	t.Parallel()

	reader := &maxBytesReader{
		reader:   strings.NewReader("1234"),
		max:      3,
		tooLarge: ErrCompressedSize,
	}
	body, err := io.ReadAll(reader)
	if !errors.Is(err, ErrCompressedSize) {
		t.Fatalf("ReadAll() error = %v, want ErrCompressedSize", err)
	}
	if got, want := string(body), "123"; got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}
}

func TestFetchDocumentReportsTruncatedResponse(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		connection, _, err := writer.(http.Hijacker).Hijack()
		if err != nil {
			return
		}
		_, _ = fmt.Fprint(connection, "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx")
		_ = connection.Close()
	}))
	defer server.Close()
	_, err := fetchDocument(context.Background(), testutil.ParseURL(t, server.URL), testutil.NewFetcher(t, fetcher.Options{}))
	if !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatalf("fetchDocument() error = %v, want unexpected EOF", err)
	}
}

func TestFetchDocumentReturnsNetworkFailure(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	target := testutil.ParseURL(t, server.URL)
	server.Close()
	_, err := fetchDocument(context.Background(), target, testutil.NewFetcher(t, fetcher.Options{}))
	if err == nil {
		t.Fatal("fetchDocument() error = nil")
	}
}

func TestSitemapReaderAndHeaderEdges(t *testing.T) {
	t.Parallel()

	if isDOCTYPE(xml.Directive("DOC")) {
		t.Fatal("short directive identified as DOCTYPE")
	}
	for _, test := range []struct {
		response *http.Response
		want     int64
	}{
		{response: &http.Response{ContentLength: 12}, want: 12},
		{response: &http.Response{Header: http.Header{}}, want: -1},
		{response: &http.Response{Header: http.Header{"Content-Length": []string{"invalid"}}}, want: -1},
		{response: &http.Response{Header: http.Header{"Content-Length": []string{"15"}}}, want: 15},
	} {
		if got := contentLength(test.response); got != test.want {
			t.Errorf("contentLength() = %d, want %d", got, test.want)
		}
	}

	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := (&contextReader{ctx: canceled, reader: strings.NewReader("data")}).Read(make([]byte, 4)); !errors.Is(err, context.Canceled) {
		t.Fatalf("contextReader.Read() error = %v", err)
	}

	ctx, cancelDuringRead := context.WithCancel(context.Background())
	reader := &contextReader{ctx: ctx, reader: cancelingReader{cancel: cancelDuringRead}}
	if _, err := reader.Read(make([]byte, 1)); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceling contextReader.Read() error = %v", err)
	}

	limited := &maxBytesReader{reader: strings.NewReader("x"), max: 0, tooLarge: ErrCompressedSize}
	if _, err := limited.Read(make([]byte, 1)); !errors.Is(err, ErrCompressedSize) {
		t.Fatalf("maxBytesReader first error = %v", err)
	}
	if _, err := limited.Read(make([]byte, 1)); !errors.Is(err, ErrCompressedSize) {
		t.Fatalf("maxBytesReader done error = %v", err)
	}
	limited = &maxBytesReader{reader: strings.NewReader("x"), max: 0, read: 1, tooLarge: ErrCompressedSize}
	if _, err := limited.Read(make([]byte, 1)); !errors.Is(err, ErrCompressedSize) {
		t.Fatalf("maxBytesReader exhausted error = %v", err)
	}
}

type recursiveSitemapSite struct {
	baseURL string
}

func (site *recursiveSitemapSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	switch request.URL.Path {
	case "/root.xml":
		_, _ = fmt.Fprint(response, testutil.SitemapIndex(site.baseURL+"/pages.xml.gz", site.baseURL+"/nested.xml"))
	case "/pages.xml.gz":
		gzipWriter := gzip.NewWriter(response)
		_, _ = gzipWriter.Write([]byte(testutil.URLSet(
			site.baseURL+"/one#details",
			site.baseURL+"/already",
			"https://external.example/outside",
			site.baseURL+"/blocked",
		)))
		_ = gzipWriter.Close()
	case "/nested.xml":
		_, _ = fmt.Fprint(response, testutil.SitemapIndex(site.baseURL+"/root.xml", site.baseURL+"/final.xml"))
	case "/final.xml":
		_, _ = fmt.Fprint(response, testutil.URLSet(site.baseURL+"/two", site.baseURL+"/one"))
	default:
		http.NotFound(response, request)
	}
}

type staticSitemapSite struct {
	body string
}

func (site *staticSitemapSite) ServeHTTP(response http.ResponseWriter, _ *http.Request) {
	_, _ = response.Write([]byte(site.body))
}

type budgetSitemapSite struct {
	mutex   sync.Mutex
	baseURL string
	fetched []string
}

func (site *budgetSitemapSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	site.mutex.Lock()
	site.fetched = append(site.fetched, request.URL.Path)
	site.mutex.Unlock()
	if request.URL.Path == "/root.xml" {
		_, _ = fmt.Fprint(response, testutil.SitemapIndex(site.baseURL+"/one.xml", site.baseURL+"/two.xml"))
		return
	}
	//nolint:gosec // URLSet XML-escapes the request-derived fixture value before it is written.
	_, _ = fmt.Fprint(response, testutil.URLSet(site.baseURL+strings.TrimSuffix(request.URL.Path, ".xml")))
}

type failureSitemapSite struct {
	baseURL string
}

func (site *failureSitemapSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	switch request.URL.Path {
	case "/not-found.xml":
		http.NotFound(response, request)
	case "/malformed.xml":
		_, _ = response.Write([]byte("<urlset><url></urlset>"))
	case "/corrupt.xml.gz":
		_, _ = response.Write([]byte{0x1f, 0x8b, 0x00})
	case "/oversized.xml":
		response.Header().Set("Content-Length", fmt.Sprint(maxCompressedBytes+1))
		_, _ = response.Write([]byte("<urlset/>"))
	default:
		_, _ = fmt.Fprint(response, testutil.URLSet(site.baseURL+"/valid"))
	}
}

type filterSitemapSite struct {
	mutex   sync.Mutex
	baseURL string
	fetched []string
}

func (site *filterSitemapSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	site.mutex.Lock()
	site.fetched = append(site.fetched, request.URL.Path)
	site.mutex.Unlock()
	_, _ = fmt.Fprint(response, testutil.SitemapIndex(site.baseURL+"/blocked.xml", site.baseURL+"/allowed.xml"))
}

type blockingSitemapSite struct{}

func (blockingSitemapSite) ServeHTTP(_ http.ResponseWriter, request *http.Request) {
	<-request.Context().Done()
}

type binarySitemapSite struct {
	body []byte
}

func (site *binarySitemapSite) ServeHTTP(response http.ResponseWriter, _ *http.Request) {
	_, _ = response.Write(site.body)
}

type repeatingReader byte

func (reader repeatingReader) Read(buffer []byte) (int, error) {
	for index := range buffer {
		buffer[index] = byte(reader)
	}
	return len(buffer), nil
}

type cancelingReader struct {
	cancel context.CancelFunc
}

func (reader cancelingReader) Read([]byte) (int, error) {
	reader.cancel()
	return 0, errors.New("read failed")
}
