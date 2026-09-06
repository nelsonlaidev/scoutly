package page

import (
	"errors"
	"io"
	"net/url"
	"strings"

	"golang.org/x/net/html"

	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

var (
	ErrNilReader  = errors.New("page: reader is nil")
	ErrNilBaseURL = errors.New("page: base URL is nil")
)

var linkAttributes = map[string]struct {
	element   LinkElement
	attribute string
}{
	"a":      {element: LinkElementAnchor, attribute: "href"},
	"iframe": {element: LinkElementIframe, attribute: "src"},
	"video":  {element: LinkElementVideo, attribute: "src"},
	"source": {element: LinkElementSource, attribute: "src"},
	"audio":  {element: LinkElementAudio, attribute: "src"},
	"embed":  {element: LinkElementEmbed, attribute: "src"},
	"object": {element: LinkElementObject, attribute: "data"},
}

// Parse parses an HTML document from reader and resolves document URLs against
// baseURL or the first valid base[href] element.
func Parse(reader io.Reader, baseURL *url.URL) (Page, error) {
	if reader == nil {
		return Page{}, ErrNilReader
	}
	if baseURL == nil {
		return Page{}, ErrNilBaseURL
	}

	document, err := html.Parse(reader)
	if err != nil {
		return Page{}, err
	}

	documentBaseURL := findDocumentBaseURL(document, baseURL)
	result := New()
	result.Title = firstElementText(document, "title")
	result.Description, _ = firstMetaContent(document, "name", "description")
	result.Headings.H1 = extractH1Headings(document)
	result.Links = extractLinks(document, documentBaseURL, baseURL)
	result.Images, result.ImageAltTexts = extractImages(document, documentBaseURL)
	result.ImageReferences = extractImageReferences(document, documentBaseURL)
	result.OpenGraph = extractOpenGraph(document, documentBaseURL)

	return result, nil
}

func findDocumentBaseURL(document *html.Node, pageURL *url.URL) *url.URL {
	var rawBaseURL string
	found := false
	walk(document, func(node *html.Node) bool {
		if found || !isElement(node, "base") {
			return !found
		}

		value, ok := attribute(node, "href")
		if !ok {
			return true
		}

		rawBaseURL = value
		found = true
		return false
	})

	if !found {
		return pageURL
	}
	if resolved, ok := resolveURL(rawBaseURL, pageURL); ok {
		return resolved
	}
	return pageURL
}

func firstElementText(document *html.Node, name string) *string {
	var value *string
	walk(document, func(node *html.Node) bool {
		if !isElement(node, name) {
			return true
		}

		value = new(strings.TrimSpace(textContent(node)))
		return false
	})
	return value
}

func firstMetaContent(document *html.Node, attributeName, attributeValue string) (*string, bool) {
	var content *string
	found := false
	walk(document, func(node *html.Node) bool {
		if !isElement(node, "meta") {
			return true
		}

		value, ok := attribute(node, attributeName)
		if !ok || !strings.EqualFold(value, attributeValue) {
			return true
		}

		found = true
		if value, ok := attribute(node, "content"); ok {
			content = new(value)
		}
		return false
	})
	return content, found
}

func extractH1Headings(document *html.Node) []string {
	headings := make([]string, 0)
	walk(document, func(node *html.Node) bool {
		if isElement(node, "h1") {
			headings = append(headings, strings.TrimSpace(textContent(node)))
		}
		return true
	})
	return headings
}

func extractLinks(document *html.Node, baseURL, pageURL *url.URL) []Link {
	links := make([]Link, 0)
	walk(document, func(node *html.Node) bool {
		if node.Type != html.ElementNode {
			return true
		}

		config, ok := linkAttributes[node.Data]
		if !ok {
			return true
		}
		rawURL, ok := attribute(node, config.attribute)
		if !ok {
			return true
		}
		resolvedURL, ok := resolveURL(rawURL, baseURL)
		if !ok {
			return true
		}

		links = append(links, Link{
			Element:     config.element,
			URL:         resolvedURL,
			OriginalURL: rawURL,
			Text:        linkText(node, config.element),
			IsExternal:  !urlutil.SameHost(resolvedURL, pageURL),
		})
		return true
	})
	return links
}

func linkText(node *html.Node, element LinkElement) string {
	switch element {
	case LinkElementAnchor:
		return strings.TrimSpace(textContent(node))
	case LinkElementIframe:
		title, _ := attribute(node, "title")
		return "[iframe] " + title
	case LinkElementSource:
		mediaType, _ := attribute(node, "type")
		if mediaType == "" {
			return "[source]"
		}
		return "[source type=" + mediaType + "]"
	case LinkElementVideo:
		return "[video]"
	case LinkElementAudio:
		return "[audio]"
	case LinkElementEmbed:
		return "[embed]"
	case LinkElementObject:
		return "[object]"
	default:
		return ""
	}
}

func extractImages(document *html.Node, baseURL *url.URL) ([]Image, []*string) {
	images := make([]Image, 0)
	altTexts := make([]*string, 0)

	walk(document, func(node *html.Node) bool {
		if !isElement(node, "img") {
			return true
		}

		alt, hasAlt := attribute(node, "alt")
		var altPointer *string
		if hasAlt {
			altPointer = new(alt)
		}
		altTexts = append(altTexts, altPointer)

		rawURL, hasSource := attribute(node, "src")
		if !hasSource {
			return true
		}
		resolvedURL, ok := resolveURL(rawURL, baseURL)
		if !ok {
			return true
		}
		images = append(images, Image{Src: resolvedURL, Alt: altPointer})
		return true
	})

	return images, altTexts
}

func extractImageReferences(document *html.Node, baseURL *url.URL) []ImageReference {
	references := make([]ImageReference, 0)

	walk(document, func(node *html.Node) bool {
		if !isElement(node, "img") {
			return true
		}

		alt, hasAlt := attribute(node, "alt")
		var altPointer *string
		if hasAlt {
			altPointer = new(alt)
		}

		if rawURL, ok := attribute(node, "src"); ok {
			references = append(references, createImageReference(
				rawURL,
				baseURL,
				ImageReferenceElementImage,
				ImageReferenceAttributeSrc,
				altPointer,
				nil,
			))
		}

		if srcset, ok := attribute(node, "srcset"); ok {
			for _, candidate := range parseSrcset(srcset) {
				references = append(references, createImageReference(
					candidate.url,
					baseURL,
					ImageReferenceElementImage,
					ImageReferenceAttributeSrcset,
					altPointer,
					candidate.descriptor,
				))
			}
		}
		return true
	})

	walk(document, func(node *html.Node) bool {
		if !isElement(node, "source") || !hasAncestor(node, "picture") {
			return true
		}
		srcset, ok := attribute(node, "srcset")
		if !ok {
			return true
		}

		alt := firstPictureImageAlt(node)
		for _, candidate := range parseSrcset(srcset) {
			references = append(references, createImageReference(
				candidate.url,
				baseURL,
				ImageReferenceElementSource,
				ImageReferenceAttributeSrcset,
				alt,
				candidate.descriptor,
			))
		}
		return true
	})

	walk(document, func(node *html.Node) bool {
		if !isElement(node, "meta") {
			return true
		}
		property, ok := attribute(node, "property")
		if !ok || !strings.EqualFold(property, "og:image") {
			return true
		}
		content, ok := attribute(node, "content")
		if !ok {
			return true
		}

		references = append(references, createImageReference(
			content,
			baseURL,
			ImageReferenceElementMeta,
			ImageReferenceAttributeContent,
			nil,
			nil,
		))
		return true
	})

	return references
}

func createImageReference(
	originalURL string,
	baseURL *url.URL,
	element ImageReferenceElement,
	attribute ImageReferenceAttribute,
	alt *string,
	descriptor *string,
) ImageReference {
	resolvedURL, _ := resolveURL(originalURL, baseURL)
	return ImageReference{
		URL:         resolvedURL,
		OriginalURL: originalURL,
		Element:     element,
		Attribute:   attribute,
		Descriptor:  descriptor,
		Alt:         alt,
	}
}

func firstPictureImageAlt(source *html.Node) *string {
	picture := source.Parent
	for picture != nil && !isElement(picture, "picture") {
		picture = picture.Parent
	}
	if picture == nil {
		return nil
	}

	var alt *string
	walk(picture, func(node *html.Node) bool {
		if !isElement(node, "img") {
			return true
		}
		if value, ok := attribute(node, "alt"); ok {
			alt = new(value)
		}
		return false
	})
	return alt
}

func extractOpenGraph(document *html.Node, baseURL *url.URL) OpenGraph {
	title, _ := firstMetaContent(document, "property", "og:title")
	description, _ := firstMetaContent(document, "property", "og:description")
	imageValue, _ := firstMetaContent(document, "property", "og:image")
	urlValue, _ := firstMetaContent(document, "property", "og:url")
	openGraphType, _ := firstMetaContent(document, "property", "og:type")
	siteName, _ := firstMetaContent(document, "property", "og:site_name")
	locale, _ := firstMetaContent(document, "property", "og:locale")

	var imageURL *url.URL
	if imageValue != nil {
		imageURL, _ = resolveURL(*imageValue, baseURL)
	}
	var pageURL *url.URL
	if urlValue != nil {
		pageURL, _ = resolveURL(*urlValue, baseURL)
	}

	return OpenGraph{
		Title:       title,
		Description: description,
		Image:       imageURL,
		URL:         pageURL,
		Type:        openGraphType,
		SiteName:    siteName,
		Locale:      locale,
	}
}

func resolveURL(value string, baseURL *url.URL) (*url.URL, bool) {
	reference, err := url.Parse(value)
	if err != nil {
		return nil, false
	}

	resolved := baseURL.ResolveReference(reference)
	if resolved == nil {
		return nil, false
	}
	return urlutil.Normalize(resolved, true), true
}

func hasAncestor(node *html.Node, name string) bool {
	for current := node.Parent; current != nil; current = current.Parent {
		if isElement(current, name) {
			return true
		}
	}
	return false
}

func attribute(node *html.Node, name string) (string, bool) {
	for _, attribute := range node.Attr {
		if attribute.Key == name {
			return attribute.Val, true
		}
	}
	return "", false
}

func textContent(node *html.Node) string {
	var builder strings.Builder
	walk(node, func(current *html.Node) bool {
		if current.Type == html.TextNode {
			builder.WriteString(current.Data)
		}
		return true
	})
	return builder.String()
}

func isElement(node *html.Node, name string) bool {
	return node.Type == html.ElementNode && node.Data == name
}

// walk visits node and its descendants in document order. Returning false from
// visitor stops the entire traversal.
func walk(node *html.Node, visitor func(*html.Node) bool) bool {
	if !visitor(node) {
		return false
	}
	for child := node.FirstChild; child != nil; child = child.NextSibling {
		if !walk(child, visitor) {
			return false
		}
	}
	return true
}
