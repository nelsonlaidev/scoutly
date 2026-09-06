package page

import "testing"

func TestIsHTMLContentType(t *testing.T) {
	t.Parallel()

	for _, test := range []struct {
		contentType string
		want        bool
	}{
		{contentType: "", want: true},
		{contentType: "Text/HTML; charset=utf-8", want: true},
		{contentType: "application/xhtml+xml", want: true},
		{contentType: "application/json", want: false},
	} {
		if got := IsHTMLContentType(test.contentType); got != test.want {
			t.Errorf("IsHTMLContentType(%q) = %t, want %t", test.contentType, got, test.want)
		}
	}
}
