package testutil

import (
	"strings"
	"testing"
)

func TestSitemapDocumentsEscapeURLs(t *testing.T) {
	t.Parallel()

	for name, document := range map[string]string{
		"url set":       URLSet("https://example.com/?a=1&b=2"),
		"sitemap index": SitemapIndex("https://example.com/index.xml?a=1&b=2"),
	} {
		if !strings.Contains(document, "a=1&amp;b=2") {
			t.Errorf("%s = %q, want escaped URL", name, document)
		}
	}
}
