// Package testutil provides shared helpers for Scoutly tests.
package testutil

import (
	"net/url"
	"testing"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
)

// NewFetcher constructs a Fetcher or fails the current test.
func NewFetcher(t testing.TB, options fetcher.Options) *fetcher.Fetcher {
	t.Helper()
	httpFetcher, err := fetcher.New(options)
	if err != nil {
		t.Fatalf("fetcher.New() error = %v", err)
	}
	return httpFetcher
}

// ParseURL parses value or fails the current test.
func ParseURL(t testing.TB, value string) *url.URL {
	t.Helper()
	parsed, err := url.Parse(value)
	if err != nil {
		t.Fatalf("url.Parse(%q) error = %v", value, err)
	}
	return parsed
}

// URLStrings returns the string representation of each URL.
func URLStrings(urls []*url.URL) []string {
	result := make([]string, 0, len(urls))
	for _, value := range urls {
		result = append(result, value.String())
	}
	return result
}
