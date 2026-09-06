package page

import (
	"errors"
	"io"
	"net/url"
	"os"
	"reflect"
	"strings"
	"testing"
	"testing/iotest"

	"golang.org/x/net/html"

	"github.com/nelsonlaidev/scoutly/internal/testutil"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

func TestParseMetadata(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name             string
		fixture          string
		title            *string
		description      *string
		h1               []string
		openGraphStrings map[string]*string
		openGraphImage   string
		openGraphURL     string
	}{
		{
			name:        "complete page",
			fixture:     "page-complete.html",
			title:       new("Example Page"),
			description: new("An example page used to test HTML extraction."),
			h1:          []string{"Main heading", "Secondary heading"},
			openGraphStrings: map[string]*string{
				"title":       new("Example Open Graph Title"),
				"description": new("Example Open Graph description."),
				"type":        new("website"),
				"siteName":    new("Example Site"),
				"locale":      new("en_US"),
			},
			openGraphImage: "https://example.com/images/social.png",
			openGraphURL:   "https://example.com/articles/page.html",
		},
		{
			name:        "minimal page",
			fixture:     "page-minimal.html",
			title:       nil,
			description: nil,
			h1:          []string{},
			openGraphStrings: map[string]*string{
				"title":       nil,
				"description": nil,
				"type":        nil,
				"siteName":    nil,
				"locale":      nil,
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			parsed := parseFixture(t, test.fixture)

			if !reflect.DeepEqual(parsed.Title, test.title) {
				t.Errorf("Title = %v, want %v", parsed.Title, test.title)
			}
			if !reflect.DeepEqual(parsed.Description, test.description) {
				t.Errorf("Description = %v, want %v", parsed.Description, test.description)
			}
			if !reflect.DeepEqual(parsed.Headings.H1, test.h1) {
				t.Errorf("Headings.H1 = %#v, want %#v", parsed.Headings.H1, test.h1)
			}

			actualOpenGraphStrings := map[string]*string{
				"title":       parsed.OpenGraph.Title,
				"description": parsed.OpenGraph.Description,
				"type":        parsed.OpenGraph.Type,
				"siteName":    parsed.OpenGraph.SiteName,
				"locale":      parsed.OpenGraph.Locale,
			}
			if !reflect.DeepEqual(actualOpenGraphStrings, test.openGraphStrings) {
				t.Errorf("OpenGraph string fields = %#v, want %#v", actualOpenGraphStrings, test.openGraphStrings)
			}
			if got := urlutil.String(parsed.OpenGraph.Image); got != test.openGraphImage {
				t.Errorf("OpenGraph.Image = %q, want %q", got, test.openGraphImage)
			}
			if got := urlutil.String(parsed.OpenGraph.URL); got != test.openGraphURL {
				t.Errorf("OpenGraph.URL = %q, want %q", got, test.openGraphURL)
			}

			if test.fixture == "page-minimal.html" {
				assertNonNilEmptyCollections(t, parsed)
			}
		})
	}
}

func TestParseLinks(t *testing.T) {
	t.Parallel()

	page := parseFixture(t, "page-complete.html")
	type expectedLink struct {
		element     LinkElement
		url         string
		originalURL string
		text        string
		isExternal  bool
	}
	expected := []expectedLink{
		{LinkElementAnchor, "https://example.com/about?source=page#team", "../about?source=page#team", "About us", false},
		{LinkElementAnchor, "https://example.com:8443/status", "https://example.com:8443/status", "Different port", true},
		{LinkElementAnchor, "https://other.example/contact", "https://other.example/contact", "External contact", true},
		{LinkElementAnchor, "https://example.com/articles/page.html#details", "#details", "Details", false},
		{LinkElementAnchor, "mailto:hello@example.com", "mailto:hello@example.com", "Email us", true},
		{LinkElementIframe, "https://example.com/frame.html", "/frame.html", "[iframe] Example frame", false},
		{LinkElementVideo, "https://example.com/media/intro.mp4", "/media/intro.mp4", "[video]", false},
		{LinkElementSource, "https://example.com/media/intro.webm", "/media/intro.webm", "[source type=video/webm]", false},
		{LinkElementSource, "https://example.com/media/intro.ogv", "/media/intro.ogv", "[source]", false},
		{LinkElementAudio, "https://example.com/media/intro.mp3", "/media/intro.mp3", "[audio]", false},
		{LinkElementEmbed, "https://example.com/media/document.pdf", "/media/document.pdf", "[embed]", false},
		{LinkElementObject, "https://example.com/media/chart.svg", "/media/chart.svg", "[object]", false},
	}

	if len(page.Links) != len(expected) {
		t.Fatalf("len(Links) = %d, want %d", len(page.Links), len(expected))
	}
	for index, want := range expected {
		got := page.Links[index]
		if got.Element != want.element ||
			urlutil.String(got.URL) != want.url ||
			got.OriginalURL != want.originalURL ||
			got.Text != want.text ||
			got.IsExternal != want.isExternal {
			t.Errorf("Links[%d] = %#v, want %#v", index, got, want)
		}
	}
}

func TestParseImages(t *testing.T) {
	t.Parallel()

	page := parseFixture(t, "page-complete.html")
	if len(page.Images) != 2 {
		t.Fatalf("len(Images) = %d, want 2", len(page.Images))
	}

	expectedImages := []struct {
		src string
		alt *string
	}{
		{src: "https://example.com/images/hero.jpg", alt: new("Example hero")},
		{src: "https://cdn.example.com/logo.svg", alt: nil},
	}
	for index, want := range expectedImages {
		got := page.Images[index]
		if urlutil.String(got.Src) != want.src || !reflect.DeepEqual(got.Alt, want.alt) {
			t.Errorf("Images[%d] = %#v, want %#v", index, got, want)
		}
	}

	expectedAltTexts := []*string{
		new("Example hero"),
		nil,
		new("Invalid image URL"),
	}
	if !reflect.DeepEqual(page.ImageAltTexts, expectedAltTexts) {
		t.Errorf("ImageAltTexts = %#v, want %#v", page.ImageAltTexts, expectedAltTexts)
	}
}

func TestParseImageReferences(t *testing.T) {
	t.Parallel()

	page := parseFixture(t, "page-complete.html")
	expected := []struct {
		url         string
		originalURL string
		element     ImageReferenceElement
		attribute   ImageReferenceAttribute
		descriptor  *string
		alt         *string
	}{
		{"https://example.com/images/hero.jpg", "/images/hero.jpg", ImageReferenceElementImage, ImageReferenceAttributeSrc, nil, new("Example hero")},
		{"https://example.com/images/hero@2x.jpg", "/images/hero@2x.jpg", ImageReferenceElementImage, ImageReferenceAttributeSrcset, new("2x"), new("Example hero")},
		{"https://cdn.example.com/hero@3x.jpg", "https://cdn.example.com/hero@3x.jpg", ImageReferenceElementImage, ImageReferenceAttributeSrcset, new("3x"), new("Example hero")},
		{"https://cdn.example.com/logo.svg", "https://cdn.example.com/logo.svg", ImageReferenceElementImage, ImageReferenceAttributeSrc, nil, nil},
		{"", "https://[invalid", ImageReferenceElementImage, ImageReferenceAttributeSrc, nil, new("Invalid image URL")},
		{"https://example.com/images/hero-small.webp", "/images/hero-small.webp", ImageReferenceElementSource, ImageReferenceAttributeSrcset, new("480w"), new("Example hero")},
		{"https://example.com/images/hero-large.webp", "/images/hero-large.webp", ImageReferenceElementSource, ImageReferenceAttributeSrcset, new("960w"), new("Example hero")},
		{"https://example.com/images/social.png", "/images/social.png", ImageReferenceElementMeta, ImageReferenceAttributeContent, nil, nil},
		{"https://example.com/images/social-wide.png", "/images/social-wide.png", ImageReferenceElementMeta, ImageReferenceAttributeContent, nil, nil},
	}

	if len(page.ImageReferences) != len(expected) {
		t.Fatalf("len(ImageReferences) = %d, want %d", len(page.ImageReferences), len(expected))
	}
	for index, want := range expected {
		got := page.ImageReferences[index]
		if urlutil.String(got.URL) != want.url ||
			got.OriginalURL != want.originalURL ||
			got.Element != want.element ||
			got.Attribute != want.attribute ||
			!reflect.DeepEqual(got.Descriptor, want.descriptor) ||
			!reflect.DeepEqual(got.Alt, want.alt) {
			t.Errorf("ImageReferences[%d] = %#v, want %#v", index, got, want)
		}
	}
}

func TestParseDocumentBaseURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		html     string
		wantLink string
	}{
		{
			name: "base without href is ignored",
			html: strings.Join([]string{
				`<base target="_blank">`,
				`<base href="https://cdn.example/assets/">`,
				`<a href="guide.html">Guide</a>`,
			}, ""),
			wantLink: "https://cdn.example/assets/guide.html",
		},
		{
			name: "first base href",
			html: strings.Join([]string{
				`<base href="https://cdn.example/assets/">`,
				`<base href="https://ignored.example/">`,
				`<a href="guide.html">Guide</a>`,
			}, ""),
			wantLink: "https://cdn.example/assets/guide.html",
		},
		{
			name: "invalid first base falls back to page URL",
			html: strings.Join([]string{
				`<base href="https://[invalid">`,
				`<base href="https://ignored.example/">`,
				`<a href="guide.html">Guide</a>`,
			}, ""),
			wantLink: "https://example.com/articles/guide.html",
		},
		{
			name:     "missing base uses page URL",
			html:     `<a href="guide.html">Guide</a>`,
			wantLink: "https://example.com/articles/guide.html",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			page, err := Parse(strings.NewReader(test.html), testutil.ParseURL(t, "https://example.com/articles/page.html"))
			if err != nil {
				t.Fatalf("Parse() error = %v", err)
			}
			if len(page.Links) != 1 {
				t.Fatalf("len(Links) = %d, want 1", len(page.Links))
			}
			if got := urlutil.String(page.Links[0].URL); got != test.wantLink {
				t.Errorf("Links[0].URL = %q, want %q", got, test.wantLink)
			}
		})
	}
}

func TestParseSkipsElementsWithoutRequiredImageAttributes(t *testing.T) {
	t.Parallel()

	document := `<img alt="missing source"><picture><source><img alt="fallback"></picture><meta property="og:image">`
	parsed, err := Parse(strings.NewReader(document), testutil.ParseURL(t, "https://example.com/page"))
	if err != nil {
		t.Fatal(err)
	}
	if len(parsed.Images) != 0 || len(parsed.ImageReferences) != 0 {
		t.Fatalf("parsed images = %#v, references = %#v", parsed.Images, parsed.ImageReferences)
	}
	if len(parsed.ImageAltTexts) != 2 {
		t.Fatalf("alt texts = %#v", parsed.ImageAltTexts)
	}
}

func TestLinkTextAndPictureAltFallbackEdges(t *testing.T) {
	t.Parallel()

	if got := linkText(&html.Node{}, LinkElement("unknown")); got != "" {
		t.Fatalf("linkText(unknown) = %q", got)
	}
	if alt := firstPictureImageAlt(&html.Node{}); alt != nil {
		t.Fatalf("firstPictureImageAlt(orphan) = %q", *alt)
	}
	picture := &html.Node{Type: html.ElementNode, Data: "picture"}
	wrapper := &html.Node{Type: html.ElementNode, Data: "div", Parent: picture}
	source := &html.Node{Type: html.ElementNode, Data: "source", Parent: wrapper}
	if alt := firstPictureImageAlt(source); alt != nil {
		t.Fatalf("firstPictureImageAlt(no image) = %q", *alt)
	}
}

func TestParseLinkExternalClassification(t *testing.T) {
	t.Parallel()

	const document = `
		<a href="https://EXAMPLE.com:443/same-default-port">same default port</a>
		<a href="http://example.com/same-host-other-scheme">same host</a>
		<a href="https://example.com:8443/different-port">different port</a>
		<a href="https://other.example/different-host">different host</a>
	`
	page, err := Parse(strings.NewReader(document), testutil.ParseURL(t, "https://example.com/articles/page.html"))
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}

	got := make([]bool, 0, len(page.Links))
	for _, link := range page.Links {
		got = append(got, link.IsExternal)
	}
	want := []bool{false, false, true, true}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("external flags = %v, want %v", got, want)
	}
}

func TestParseNormalizesInternationalizedLinkURLs(t *testing.T) {
	t.Parallel()

	baseURL, err := url.Parse("https://xn--bcher-kva.example/start")
	if err != nil {
		t.Fatal(err)
	}
	page, err := Parse(
		strings.NewReader(`<a href="HTTPS://BÜCHER.example:443/next">next</a>`),
		baseURL,
	)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if got := page.Links[0].URL.String(); got != "https://xn--bcher-kva.example/next" {
		t.Fatalf("link URL = %q", got)
	}
	if page.Links[0].IsExternal {
		t.Fatal("equivalent Unicode host classified as external")
	}
}

func TestParseErrors(t *testing.T) {
	t.Parallel()

	readFailure := errors.New("read failure")
	tests := []struct {
		name    string
		reader  io.Reader
		baseURL *url.URL
		wantErr error
	}{
		{name: "nil reader", reader: nil, baseURL: testutil.ParseURL(t, "https://example.com/articles/page.html"), wantErr: ErrNilReader},
		{name: "nil base URL", reader: strings.NewReader(""), baseURL: nil, wantErr: ErrNilBaseURL},
		{name: "reader failure", reader: iotest.ErrReader(readFailure), baseURL: testutil.ParseURL(t, "https://example.com/articles/page.html"), wantErr: readFailure},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			_, err := Parse(test.reader, test.baseURL)
			if !errors.Is(err, test.wantErr) {
				t.Errorf("Parse() error = %v, want errors.Is(_, %v)", err, test.wantErr)
			}
		})
	}
}

func parseFixture(t *testing.T, name string) Page {
	t.Helper()

	root, err := os.OpenRoot("testdata")
	if err != nil {
		t.Fatalf("open fixture root: %v", err)
	}
	defer func() {
		if closeErr := root.Close(); closeErr != nil {
			t.Errorf("close fixture root: %v", closeErr)
		}
	}()

	file, err := root.Open(name)
	if err != nil {
		t.Fatalf("open fixture: %v", err)
	}
	defer func() {
		if closeErr := file.Close(); closeErr != nil {
			t.Errorf("close fixture: %v", closeErr)
		}
	}()

	page, err := Parse(file, testutil.ParseURL(t, "https://example.com/articles/page.html"))
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	return page
}

func assertNonNilEmptyCollections(t *testing.T, page Page) {
	t.Helper()

	collections := []struct {
		name  string
		value any
	}{
		{name: "Headings.H1", value: page.Headings.H1},
		{name: "Links", value: page.Links},
		{name: "Images", value: page.Images},
		{name: "ImageAltTexts", value: page.ImageAltTexts},
		{name: "ImageReferences", value: page.ImageReferences},
	}
	for _, collection := range collections {
		value := reflect.ValueOf(collection.value)
		if value.IsNil() || value.Len() != 0 {
			t.Errorf("%s = %#v, want non-nil empty slice", collection.name, collection.value)
		}
	}
}
