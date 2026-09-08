package audit

import "testing"

func TestLinkIsBroken(t *testing.T) {
	t.Parallel()

	ok := 200
	notFound := 404
	tests := []struct {
		name string
		link Link
		want bool
	}{
		{name: "successful response", link: Link{Result: LinkResult{Kind: ResultResponse, StatusCode: &ok}}},
		{name: "HTTP error", link: Link{Result: LinkResult{Kind: ResultResponse, StatusCode: &notFound}}, want: true},
		{name: "request failure", link: Link{Result: LinkResult{Kind: ResultFailed}}, want: true},
		{name: "blocked", link: Link{Result: LinkResult{Kind: ResultBlocked, StatusCode: &notFound}}},
		{name: "skipped", link: Link{Result: LinkResult{Kind: ResultSkipped}}},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := test.link.IsBroken(); got != test.want {
				t.Fatalf("IsBroken() = %t, want %t", got, test.want)
			}
		})
	}
}

func TestLinkIsRedirected(t *testing.T) {
	t.Parallel()

	sameURL := "https://example.com/page"
	otherURL := "https://example.com/other"
	tests := []struct {
		name string
		link Link
		want bool
	}{
		{name: "same URL", link: Link{URL: sameURL, Result: LinkResult{Kind: ResultResponse, FinalURL: &sameURL}}},
		{name: "different URL", link: Link{URL: sameURL, Result: LinkResult{Kind: ResultResponse, FinalURL: &otherURL}}, want: true},
		{name: "fragment-only difference", link: Link{URL: sameURL + "#section", Result: LinkResult{Kind: ResultResponse, FinalURL: &sameURL}}},
		{name: "malformed request URL", link: Link{URL: ":", Result: LinkResult{Kind: ResultResponse, FinalURL: &otherURL}}, want: true},
		{name: "failed result", link: Link{URL: sameURL, Result: LinkResult{Kind: ResultFailed, FinalURL: &otherURL}}},
		{name: "missing final URL", link: Link{URL: sameURL, Result: LinkResult{Kind: ResultResponse}}},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := test.link.IsRedirected(); got != test.want {
				t.Fatalf("IsRedirected() = %t, want %t", got, test.want)
			}
		})
	}
}

func TestImageStatus(t *testing.T) {
	t.Parallel()

	ok := 200
	notFound := 404
	imageType := "image/png"
	textType := "text/plain"
	tests := []struct {
		name        string
		image       Image
		wantBroken  bool
		wantInvalid bool
	}{
		{name: "valid response", image: Image{Result: ImageResult{Kind: ResultResponse, StatusCode: &ok, ContentType: &imageType}}},
		{name: "HTTP error", image: Image{Result: ImageResult{Kind: ResultResponse, StatusCode: &notFound, ContentType: &textType}}, wantBroken: true},
		{name: "request failure", image: Image{Result: ImageResult{Kind: ResultFailed}}, wantBroken: true},
		{name: "invalid URL", image: Image{Result: ImageResult{Kind: ResultInvalid}}, wantInvalid: true},
		{name: "invalid content type", image: Image{Result: ImageResult{Kind: ResultResponse, StatusCode: &ok, ContentType: &textType}}, wantInvalid: true},
		{name: "missing content type", image: Image{Result: ImageResult{Kind: ResultResponse, StatusCode: &ok}}, wantInvalid: true},
		{name: "blocked", image: Image{Result: ImageResult{Kind: ResultBlocked, StatusCode: &notFound, ContentType: &textType}}},
		{name: "skipped", image: Image{Result: ImageResult{Kind: ResultSkipped}}},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := test.image.IsBroken(); got != test.wantBroken {
				t.Errorf("IsBroken() = %t, want %t", got, test.wantBroken)
			}
			if got := test.image.IsInvalid(); got != test.wantInvalid {
				t.Errorf("IsInvalid() = %t, want %t", got, test.wantInvalid)
			}
		})
	}
}

func TestImageIsRedirected(t *testing.T) {
	t.Parallel()

	requestURL := "https://example.com/image.png"
	finalURL := "https://cdn.example.com/image.png"
	image := Image{
		URL: requestURL,
		Result: ImageResult{
			Kind:     ResultResponse,
			FinalURL: &finalURL,
		},
	}
	if !image.IsRedirected() {
		t.Fatal("IsRedirected() = false")
	}
	image.Result.Kind = ResultFailed
	if image.IsRedirected() {
		t.Fatal("failed image IsRedirected() = true")
	}
}
