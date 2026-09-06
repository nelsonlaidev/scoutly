package testutil

import (
	"net/url"
	"reflect"
	"testing"

	"github.com/nelsonlaidev/scoutly/internal/fetcher"
)

func TestHTTPHelpers(t *testing.T) {
	t.Parallel()

	if NewFetcher(t, fetcher.Options{}) == nil {
		t.Fatal("NewFetcher() = nil")
	}
	first := ParseURL(t, "https://example.com/one")
	second := ParseURL(t, "https://example.com/two")
	if got, want := URLStrings([]*url.URL{first, second}), []string{first.String(), second.String()}; !reflect.DeepEqual(got, want) {
		t.Fatalf("URLStrings() = %#v, want %#v", got, want)
	}
}
