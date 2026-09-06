package testutil

import (
	"encoding/xml"
	"strings"
)

// URLSet returns a sitemap urlset document containing urls.
func URLSet(urls ...string) string {
	return sitemapDocument("urlset", "url", urls)
}

// SitemapIndex returns a sitemap index document containing urls.
func SitemapIndex(urls ...string) string {
	return sitemapDocument("sitemapindex", "sitemap", urls)
}

func sitemapDocument(root, entry string, urls []string) string {
	var builder strings.Builder
	builder.WriteString(`<` + root + ` xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">`)
	for _, value := range urls {
		builder.WriteString("<" + entry + "><loc>")
		_ = xml.EscapeText(&builder, []byte(value))
		builder.WriteString("</loc></" + entry + ">")
	}
	builder.WriteString("</" + root + ">")
	return builder.String()
}
