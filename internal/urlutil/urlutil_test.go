package urlutil

import (
	"net/url"
	"testing"
)

func TestNormalizeHTTPURL(t *testing.T) {
	input := mustParse(t, "HTTPS://EXAMPLE.COM:443#section")
	got := Normalize(input, false)
	if got.String() != "https://example.com/" {
		t.Fatalf("Normalize() = %q, want https://example.com/", got)
	}
	if input.String() == got.String() {
		t.Fatal("Normalize() modified its input or returned it unchanged")
	}
}

func TestNormalizePreservesNonHTTPURLShape(t *testing.T) {
	input := mustParse(t, "mailto:person@example.com#section")
	got := Normalize(input, false)
	if got.String() != "mailto:person@example.com" {
		t.Fatalf("Normalize() = %q", got)
	}
}

func TestNormalizeNilURL(t *testing.T) {
	t.Parallel()

	if Normalize(nil, true) != nil {
		t.Fatal("Normalize(nil) returned a URL")
	}
}

func TestCanonicalHostHandlesIPv6AndInvalidIDNA(t *testing.T) {
	t.Parallel()

	for _, test := range []struct {
		scheme string
		host   string
		want   string
	}{
		{scheme: "https", host: "[2001:DB8::1]:443", want: "[2001:db8::1]"},
		{scheme: "https", host: "[2001:DB8::1]:8443", want: "[2001:db8::1]:8443"},
		{scheme: "https", host: "xn--", want: ""},
	} {
		if got := CanonicalHost(test.scheme, test.host); got != test.want {
			t.Errorf("CanonicalHost(%q, %q) = %q, want %q", test.scheme, test.host, got, test.want)
		}
	}
}

func TestSameHostHandlesExplicitDefaultPorts(t *testing.T) {
	tests := []struct {
		left  string
		right string
		want  bool
	}{
		{left: "https://example.com", right: "https://example.com:443", want: true},
		{left: "http://example.com", right: "http://example.com:80", want: true},
		{left: "https://example.com:8443", right: "https://example.com", want: false},
	}
	for _, test := range tests {
		if got := SameHost(mustParse(t, test.left), mustParse(t, test.right)); got != test.want {
			t.Errorf("SameHost(%q, %q) = %t, want %t", test.left, test.right, got, test.want)
		}
	}
	if SameHost(nil, mustParse(t, "https://example.com")) || SameHost(mustParse(t, "https://example.com"), nil) {
		t.Fatal("SameHost() accepted a nil URL")
	}
}

func TestSameHostNormalizesInternationalizedDomains(t *testing.T) {
	unicode := mustParse(t, "https://bücher.example/")
	punycode := mustParse(t, "https://xn--bcher-kva.example/")
	if !SameHost(unicode, punycode) {
		t.Fatalf("SameHost(%q, %q) = false", unicode, punycode)
	}
	if got := Normalize(unicode, true).String(); got != "https://xn--bcher-kva.example/" {
		t.Fatalf("Normalize() = %q", got)
	}
}

func TestString(t *testing.T) {
	t.Parallel()

	if got := String(mustParse(t, "https://example.com/path")); got != "https://example.com/path" {
		t.Errorf("String() = %q", got)
	}
	if got := String(nil); got != "" {
		t.Errorf("String(nil) = %q, want empty string", got)
	}
}

func TestIsHTTPWithHost(t *testing.T) {
	tests := []struct {
		input string
		want  bool
	}{
		{input: "https://example.com", want: true},
		{input: "http://example.com", want: true},
		{input: "HTTPS://EXAMPLE.COM:443/start", want: true},
		{input: "mailto:person@example.com", want: false},
		{input: "ftp://example.com/files", want: false},
		{input: "https://", want: false},
	}
	for _, test := range tests {
		if got := IsHTTPWithHost(mustParse(t, test.input)); got != test.want {
			t.Errorf("IsHTTPWithHost(%q) = %t, want %t", test.input, got, test.want)
		}
	}
	if IsHTTPWithHost(nil) {
		t.Error("IsHTTPWithHost(nil) = true, want false")
	}
}

func TestParseHTTP(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{input: "https://example.com", want: "https://example.com"},
		{input: "  https://example.com/start  ", want: "https://example.com/start"},
		{input: "HTTPS://EXAMPLE.COM:443", want: "https://EXAMPLE.COM:443"},
	}
	for _, test := range tests {
		parsed, ok := ParseHTTP(test.input)
		if !ok {
			t.Errorf("ParseHTTP(%q) ok = false, want true", test.input)
			continue
		}
		if got := parsed.String(); got != test.want {
			t.Errorf("ParseHTTP(%q) = %q, want %q", test.input, got, test.want)
		}
	}
	for _, input := range []string{"", "not a url", "mailto:person@example.com", "https://"} {
		if parsed, ok := ParseHTTP(input); ok {
			t.Errorf("ParseHTTP(%q) = %v, true, want nil, false", input, parsed)
		}
	}
}

func TestOrigin(t *testing.T) {
	unicode := mustParse(t, "https://bücher.example/private")
	punycode := mustParse(t, "https://xn--bcher-kva.example/start")
	if Origin(unicode) != Origin(punycode) {
		t.Fatalf("origins differ: %q != %q", Origin(unicode), Origin(punycode))
	}
	if got, want := Origin(mustParse(t, "https://example.com:443/start")), "https://example.com"; got != want {
		t.Errorf("Origin() = %q, want %q", got, want)
	}
	if got := Origin(nil); got != "" {
		t.Errorf("Origin(nil) = %q, want empty string", got)
	}
}

func TestParseTarget(t *testing.T) {
	t.Parallel()

	target, err := ParseTarget("HTTPS://EXAMPLE.COM:443/path#fragment")
	if err != nil {
		t.Fatalf("ParseTarget() error = %v", err)
	}
	if got := target.String(); got != "https://example.com/path#fragment" {
		t.Fatalf("ParseTarget() = %q", got)
	}

	for _, value := range []string{
		"://bad",
		"/relative",
		"ftp://example.com",
		"https://example.com:0",
		"https://example.com:65536",
		"https://example.com:invalid",
	} {
		if parsed, err := ParseTarget(value); err == nil || parsed != nil {
			t.Errorf("ParseTarget(%q) = %v, %v, want nil, error", value, parsed, err)
		}
	}
}

func mustParse(t *testing.T, value string) *url.URL {
	t.Helper()
	parsed, err := url.Parse(value)
	if err != nil {
		t.Fatal(err)
	}
	return parsed
}
