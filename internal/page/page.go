// Package page parses HTML documents into an SEO-relevant representation.
package page

import (
	"net/url"
	"strings"
)

// LinkElement identifies the HTML element from which a link was extracted.
type LinkElement string

const (
	LinkElementAnchor LinkElement = "a"
	LinkElementIframe LinkElement = "iframe"
	LinkElementVideo  LinkElement = "video"
	LinkElementSource LinkElement = "source"
	LinkElementAudio  LinkElement = "audio"
	LinkElementEmbed  LinkElement = "embed"
	LinkElementObject LinkElement = "object"
)

// Link is a URL-bearing element found in a page.
type Link struct {
	Element     LinkElement
	URL         *url.URL
	OriginalURL string
	Text        string
	IsExternal  bool
}

// Image is an img element with a valid src URL.
type Image struct {
	Src *url.URL
	Alt *string
}

// ImageReferenceElement identifies an element that can refer to an image.
type ImageReferenceElement string

const (
	ImageReferenceElementImage  ImageReferenceElement = "img"
	ImageReferenceElementSource ImageReferenceElement = "source"
	ImageReferenceElementMeta   ImageReferenceElement = "meta"
)

// ImageReferenceAttribute identifies the attribute containing an image URL.
type ImageReferenceAttribute string

const (
	ImageReferenceAttributeSrc     ImageReferenceAttribute = "src"
	ImageReferenceAttributeSrcset  ImageReferenceAttribute = "srcset"
	ImageReferenceAttributeContent ImageReferenceAttribute = "content"
)

// ImageReference describes an image URL exactly where it appeared in the HTML.
// URL is nil when OriginalURL cannot be resolved.
type ImageReference struct {
	URL         *url.URL
	OriginalURL string
	Element     ImageReferenceElement
	Attribute   ImageReferenceAttribute
	Descriptor  *string
	Alt         *string
}

// OpenGraph contains the supported Open Graph fields from a page.
type OpenGraph struct {
	Title       *string
	Description *string
	Image       *url.URL
	URL         *url.URL
	Type        *string
	SiteName    *string
	Locale      *string
}

// Headings contains headings extracted from a page.
type Headings struct {
	H1 []string
}

// Page is the parsed, SEO-relevant representation of an HTML page.
//
// ContentType is intentionally not populated by Parse. Callers that obtained the
// document over HTTP may set it before inspecting the page. An empty
// ContentType is treated as HTML by IsHTMLContentType.
type Page struct {
	ContentType     string
	Title           *string
	Description     *string
	Headings        Headings
	Links           []Link
	Images          []Image
	ImageAltTexts   []*string
	ImageReferences []ImageReference
	OpenGraph       OpenGraph
}

// New returns an empty Page with initialized collections.
func New() Page {
	return Page{
		Headings:        Headings{H1: make([]string, 0)},
		Links:           make([]Link, 0),
		Images:          make([]Image, 0),
		ImageAltTexts:   make([]*string, 0),
		ImageReferences: make([]ImageReference, 0),
	}
}

// IsHTMLContentType reports whether contentType represents HTML or XHTML.
// An empty value is treated as HTML because HTTP responses may omit the header.
func IsHTMLContentType(contentType string) bool {
	if contentType == "" {
		return true
	}
	normalized := strings.ToLower(contentType)
	return strings.Contains(normalized, "text/html") ||
		strings.Contains(normalized, "application/xhtml")
}
