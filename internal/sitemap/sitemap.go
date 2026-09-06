// Package sitemap discovers pages from XML sitemap documents.
package sitemap

import (
	"bufio"
	"compress/gzip"
	"context"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

const (
	maxXMLDepth          = 32
	maxEntries           = 50_000
	maxCompressedBytes   = 51 * 1024 * 1024
	maxDecompressedBytes = 50 * 1024 * 1024
)

var (
	ErrDOCTYPE          = errors.New("sitemap: doctype is not allowed")
	ErrMalformed        = errors.New("sitemap: malformed XML")
	ErrUnsupportedRoot  = errors.New("sitemap: unsupported root element")
	ErrTooDeep          = errors.New("sitemap: XML nesting exceeds 32 elements")
	ErrTooManyEntries   = errors.New("sitemap: document exceeds 50000 entries")
	ErrCompressedSize   = errors.New("sitemap: compressed document exceeds 51 MiB")
	ErrDecompressedSize = errors.New("sitemap: decompressed document exceeds 50 MiB")
	ErrNilContext       = errors.New("sitemap: nil context")
	ErrNilFetcher       = errors.New("sitemap: fetcher is nil")
	ErrNilResponse      = errors.New("sitemap: fetcher returned a nil response")
	ErrNilBody          = errors.New("sitemap: response body is nil")
)

// Options controls sitemap discovery.
type Options struct {
	Allow             func(*url.URL) bool
	Scheme            string
	Host              string
	KeepFragments     bool
	ExcludedPageKeys  map[string]struct{}
	MaxDocuments      int
	MaxPageURLs       int
	OnDocumentFetched func(*url.URL)
}

// Kind identifies the root element of a parsed sitemap document.
type Kind uint8

const (
	KindURLSet Kind = iota + 1
	KindIndex
)

// Parsed is a validated sitemap document in source order.
type Parsed struct {
	Kind Kind
	URLs []*url.URL
}

// Crawl traverses sitemap indexes breadth-first and returns discovered page
// URLs in document order. Individual document failures are ignored, while
// context cancellation aborts the traversal.
func Crawl(
	ctx context.Context,
	initial []*url.URL,
	httpFetcher *fetcher.Fetcher,
	options Options,
) ([]*url.URL, error) {
	if ctx == nil {
		return nil, ErrNilContext
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if options.MaxDocuments <= 0 || options.MaxPageURLs <= 0 {
		return make([]*url.URL, 0), nil
	}
	if httpFetcher == nil {
		return nil, ErrNilFetcher
	}

	allowed := options.Allow
	if allowed == nil {
		allowed = func(*url.URL) bool { return true }
	}
	siteURL := &url.URL{Scheme: options.Scheme, Host: options.Host}

	queue := make([]*url.URL, 0, len(initial))
	enqueuedDocuments := make(map[string]struct{}, len(initial))
	enqueueDocuments(initial, allowed, &queue, enqueuedDocuments, options.MaxDocuments)

	excluded := make(map[string]struct{}, len(options.ExcludedPageKeys))
	for key := range options.ExcludedPageKeys {
		excluded[key] = struct{}{}
	}

	pages := make([]*url.URL, 0, min(options.MaxPageURLs, 256))
	fetchedDocuments := 0

	for len(queue) > 0 && fetchedDocuments < options.MaxDocuments && len(pages) < options.MaxPageURLs {
		if err := ctx.Err(); err != nil {
			return nil, err
		}

		documentURL := queue[0]
		queue = queue[1:]
		fetchedDocuments++

		parsed, err := fetchDocument(ctx, documentURL, httpFetcher)
		if ctxErr := ctx.Err(); ctxErr != nil {
			return nil, ctxErr
		}
		if options.OnDocumentFetched != nil {
			options.OnDocumentFetched(documentURL)
		}
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if err != nil {
			continue
		}

		if parsed.Kind == KindIndex {
			enqueueDocuments(
				parsed.URLs,
				allowed,
				&queue,
				enqueuedDocuments,
				options.MaxDocuments-fetchedDocuments,
			)
			continue
		}

		for _, pageURL := range parsed.URLs {
			if urlutil.Origin(pageURL) != urlutil.Origin(siteURL) || !allowed(pageURL) {
				continue
			}

			normalized := urlutil.Normalize(pageURL, options.KeepFragments)
			key := normalized.String()
			if _, seen := excluded[key]; seen {
				continue
			}

			excluded[key] = struct{}{}
			pages = append(pages, normalized)
			if len(pages) >= options.MaxPageURLs {
				break
			}
		}
	}

	if err := ctx.Err(); err != nil {
		return nil, err
	}
	return pages, nil
}

// Parse validates and streams a urlset or sitemapindex document.
func Parse(reader io.Reader) (Parsed, error) {
	if reader == nil {
		return Parsed{}, fmt.Errorf("%w: reader is nil", ErrMalformed)
	}

	decoder := xml.NewDecoder(reader)
	decoder.Strict = true

	var (
		kind            Kind
		rootSeen        bool
		rootEnded       bool
		depth           int
		entryDepth      int
		entryName       string
		entryCount      int
		entryLocation   string
		locationDepth   int
		locationText    strings.Builder
		locationInvalid bool
		urls            = make([]*url.URL, 0)
	)

	for {
		token, err := decoder.Token()
		if err != nil {
			if errors.Is(err, io.EOF) {
				break
			}
			return Parsed{}, fmt.Errorf("%w: %w", ErrMalformed, err)
		}

		switch value := token.(type) {
		case xml.Directive:
			if isDOCTYPE(value) {
				return Parsed{}, ErrDOCTYPE
			}

		case xml.StartElement:
			if rootEnded {
				return Parsed{}, fmt.Errorf("%w: multiple root elements", ErrMalformed)
			}

			depth++
			if depth > maxXMLDepth {
				return Parsed{}, ErrTooDeep
			}

			if !rootSeen {
				rootSeen = true
				switch value.Name.Local {
				case "urlset":
					kind = KindURLSet
					entryName = "url"
				case "sitemapindex":
					kind = KindIndex
					entryName = "sitemap"
				default:
					return Parsed{}, fmt.Errorf("%w: %s", ErrUnsupportedRoot, value.Name.Local)
				}
				continue
			}

			if depth == 2 && value.Name.Local == entryName {
				entryCount++
				if entryCount > maxEntries {
					return Parsed{}, ErrTooManyEntries
				}
				entryDepth = depth
				entryLocation = ""
				continue
			}

			if entryDepth != 0 && depth == entryDepth+1 && value.Name.Local == "loc" && locationDepth == 0 {
				locationDepth = depth
				locationInvalid = false
				locationText.Reset()
				continue
			}
			if locationDepth != 0 && depth > locationDepth {
				locationInvalid = true
			}

		case xml.CharData:
			if !rootSeen || rootEnded {
				if strings.TrimSpace(string(value)) != "" {
					return Parsed{}, fmt.Errorf("%w: character data outside root", ErrMalformed)
				}
				continue
			}
			if locationDepth != 0 && !locationInvalid {
				_, _ = locationText.Write(value)
			}

		case xml.EndElement:
			if locationDepth == depth {
				if !locationInvalid && entryLocation == "" {
					entryLocation = strings.TrimSpace(locationText.String())
				}
				locationDepth = 0
				locationText.Reset()
			}

			if entryDepth == depth {
				if parsedURL, ok := urlutil.ParseHTTP(entryLocation); ok {
					urls = append(urls, parsedURL)
				}
				entryDepth = 0
				entryLocation = ""
			}

			if depth == 1 {
				rootEnded = true
			}
			depth--
			if depth < 0 {
				return Parsed{}, ErrMalformed
			}
		}
	}

	if !rootSeen || !rootEnded || depth != 0 {
		return Parsed{}, ErrMalformed
	}
	return Parsed{Kind: kind, URLs: urls}, nil
}

func fetchDocument(
	ctx context.Context,
	target *url.URL,
	httpFetcher *fetcher.Fetcher,
) (Parsed, error) {
	resp, err := httpFetcher.Fetch(ctx, target)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Parsed{}, ctxErr
		}
		return Parsed{}, err
	}
	if resp == nil {
		return Parsed{}, ErrNilResponse
	}
	if resp.Body == nil {
		return Parsed{}, ErrNilBody
	}
	defer func() {
		_ = resp.Body.Close()
	}()

	if resp.StatusCode < http.StatusOK || resp.StatusCode >= http.StatusMultipleChoices {
		return Parsed{}, fmt.Errorf("sitemap: unexpected HTTP status %d", resp.StatusCode)
	}
	if contentLength(resp) > maxCompressedBytes {
		return Parsed{}, ErrCompressedSize
	}

	compressed := &maxBytesReader{
		reader:   &contextReader{ctx: ctx, reader: resp.Body},
		max:      maxCompressedBytes,
		tooLarge: ErrCompressedSize,
	}
	buffered := bufio.NewReader(compressed)
	header, peekErr := buffered.Peek(2)
	if peekErr != nil && !errors.Is(peekErr, io.EOF) {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Parsed{}, ctxErr
		}
		return Parsed{}, peekErr
	}

	var document io.Reader = buffered
	if len(header) == 2 && header[0] == 0x1f && header[1] == 0x8b {
		gzipReader, err := gzip.NewReader(buffered)
		if err != nil {
			return Parsed{}, err
		}
		defer func() {
			_ = gzipReader.Close()
		}()
		document = gzipReader
	}

	decompressed := &maxBytesReader{
		reader:   &contextReader{ctx: ctx, reader: document},
		max:      maxDecompressedBytes,
		tooLarge: ErrDecompressedSize,
	}
	parsed, err := Parse(decompressed)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return Parsed{}, ctxErr
		}
		return Parsed{}, err
	}
	if err := ctx.Err(); err != nil {
		return Parsed{}, err
	}
	return parsed, nil
}

func enqueueDocuments(
	candidates []*url.URL,
	allowed func(*url.URL) bool,
	queue *[]*url.URL,
	enqueued map[string]struct{},
	maxQueued int,
) {
	for _, candidate := range candidates {
		if len(*queue) >= maxQueued {
			return
		}
		if !urlutil.IsHTTPWithHost(candidate) || !allowed(candidate) {
			continue
		}
		normalized := urlutil.Normalize(candidate, true)
		key := normalized.String()
		if _, seen := enqueued[key]; seen {
			continue
		}
		enqueued[key] = struct{}{}
		*queue = append(*queue, normalized)
	}
}

func isDOCTYPE(directive xml.Directive) bool {
	trimmed := strings.TrimSpace(string(directive))
	if len(trimmed) < len("DOCTYPE") {
		return false
	}
	return strings.EqualFold(trimmed[:len("DOCTYPE")], "DOCTYPE")
}

func contentLength(resp *http.Response) int64 {
	if resp.ContentLength > 0 {
		return resp.ContentLength
	}
	value := strings.TrimSpace(resp.Header.Get("Content-Length"))
	if value == "" {
		return -1
	}
	length, err := strconv.ParseInt(value, 10, 64)
	if err != nil {
		return -1
	}
	return length
}

type contextReader struct {
	ctx    context.Context
	reader io.Reader
}

func (reader *contextReader) Read(buffer []byte) (int, error) {
	if err := reader.ctx.Err(); err != nil {
		return 0, err
	}
	n, err := reader.reader.Read(buffer)
	if err != nil {
		if ctxErr := reader.ctx.Err(); ctxErr != nil {
			return n, ctxErr
		}
	}
	return n, err
}

type maxBytesReader struct {
	reader   io.Reader
	max      int64
	read     int64
	tooLarge error
	done     bool
}

func (reader *maxBytesReader) Read(buffer []byte) (int, error) {
	if reader.done {
		return 0, reader.tooLarge
	}

	remainingWithProbe := reader.max - reader.read + 1
	if remainingWithProbe <= 0 {
		reader.done = true
		return 0, reader.tooLarge
	}
	if int64(len(buffer)) > remainingWithProbe {
		buffer = buffer[:remainingWithProbe]
	}

	n, err := reader.reader.Read(buffer)
	if reader.read+int64(n) > reader.max {
		allowed := int(reader.max - reader.read)
		reader.read = reader.max
		reader.done = true
		return allowed, reader.tooLarge
	}
	reader.read += int64(n)
	return n, err
}
