package audit

import (
	"errors"
	"math"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestOptionsPageScope(t *testing.T) {
	t.Parallel()
	options := DefaultOptions()
	options.IncludePaths = []string{"/docs/", "/about", "/caf%C3%A9"}
	options.ExcludePaths = []string{"/docs/archive"}
	if err := options.Validate(); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		path string
		want bool
	}{
		{"/docs", true}, {"/docs/start?q=1#part", true},
		{"/docs%2Fstart", true}, {"/docs-old", false}, {"/Docs", false},
		{"/docs/archive", false}, {"/docs/archive/item", false},
		{"/docs/archived", true}, {"/about", true}, {"/caf%C3%A9/menu", true},
		{"/", false},
	} {
		t.Run(test.path, func(t *testing.T) {
			candidate := testutil.ParseURL(t, "https://example.com"+test.path)
			if got := options.AllowsPage(candidate); got != test.want {
				t.Fatalf("AllowsPage(%q) = %t", test.path, got)
			}
		})
	}
	options.IncludePaths = nil
	if !options.AllowsPage(testutil.ParseURL(t, "https://example.com")) || options.AllowsPage(nil) {
		t.Fatal("empty inclusion or nil URL handled incorrectly")
	}
}

func TestOptionsRejectInvalidScopeAndIgnores(t *testing.T) {
	t.Parallel()
	for _, path := range []string{"", "docs", "https://example.com/docs", "//example.com/docs", "/docs?", "/docs#", "/docs/**", "/[ab]", "/bad%", "/bad\n"} {
		t.Run("path_"+path, func(t *testing.T) {
			options := DefaultOptions()
			options.IncludePaths = []string{path}
			options.ExcludePaths = []string{path}
			if err := options.Validate(); err == nil {
				t.Fatalf("accepted invalid path %q", path)
			}
		})
	}
	for _, ignore := range []RuleIgnore{
		{URLPrefix: "/docs", Rules: []string{"missing_title"}},
		{URLPrefix: "https://user:pass@example.com", Rules: []string{"missing_title"}}, //nolint:gosec // Synthetic credentials verify that URL userinfo is rejected.
		{URLPrefix: "https://example.com:99999", Rules: []string{"missing_title"}},
		{URLPrefix: "https://example.com/?", Rules: []string{"missing_title"}},
		{URLPrefix: "https://example.com/#", Rules: []string{"missing_title"}},
		{URLPrefix: "https://example.com/*", Rules: []string{"missing_title"}},
		{URLPrefix: "https://example.com/%", Rules: []string{"missing_title"}},
		{URLPrefix: "https://example.com", Rules: nil},
		{URLPrefix: "https://example.com", Rules: []string{"missing-title"}},
	} {
		options := DefaultOptions()
		options.IgnoreRules = []RuleIgnore{ignore}
		if err := options.Validate(); err == nil {
			t.Fatalf("accepted invalid ignore %#v", ignore)
		}
	}
	options := DefaultOptions()
	options.IgnoreRules = []RuleIgnore{{URLPrefix: "https://[::1]/docs", Rules: []string{"missing_title"}}}
	if err := options.Validate(); err != nil {
		t.Fatalf("valid IPv6 URL rejected: %v", err)
	}
}

func TestValidationErrorFormatsFields(t *testing.T) {
	t.Parallel()

	err := (&ValidationError{Fields: []FieldError{
		{Field: "max_pages", Message: "must be greater than 0"},
		{Field: "timeout", Message: "must be greater than 0"},
	}}).Error()
	want := "invalid options:\n- max_pages must be greater than 0\n- timeout must be greater than 0"
	if err != want {
		t.Fatalf("ValidationError.Error() = %q, want %q", err, want)
	}
}

func TestDefaultOptions(t *testing.T) {
	want := Options{
		MaxDepth:            10,
		MaxPages:            500,
		KeepFragments:       false,
		IgnoreRedirects:     false,
		RateLimit:           0,
		RespectRobots:       true,
		Sitemaps:            true,
		Images:              true,
		MaxSitemapDocuments: 1000,
		Timeout:             30 * time.Second,
		MaxRedirects:        10,
		UserAgent:           "scoutly/dev",
		Concurrency:         20,
		Rules:               Rules{},
	}

	if got := DefaultOptions(); !reflect.DeepEqual(got, want) {
		t.Fatalf("DefaultOptions() = %#v, want %#v", got, want)
	}
}

func TestDefaultOptionsReturnsIndependentRules(t *testing.T) {
	t.Parallel()

	first := DefaultOptions()
	first.Rules[ruleName(IssueTitleTooShort)] = RuleLevelWarning
	second := DefaultOptions()
	if _, exists := second.Rules[ruleName(IssueTitleTooShort)]; exists {
		t.Fatalf("DefaultOptions() shared Rules map: %#v", second.Rules)
	}
}

func TestOptionsValidateAcceptsBoundaryValues(t *testing.T) {
	options := DefaultOptions()
	options.MaxDepth = 0
	options.MaxRedirects = 0
	options.RateLimit = 0

	if err := options.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
}

func TestOptionsValidateReportsEveryInvalidField(t *testing.T) {
	options := Options{
		MaxDepth:            -1,
		MaxPages:            0,
		RateLimit:           math.NaN(),
		MaxSitemapDocuments: 0,
		Timeout:             0,
		MaxRedirects:        -1,
		UserAgent:           "",
		Concurrency:         0,
	}

	err := options.Validate()
	var validationError *ValidationError
	if !errors.As(err, &validationError) {
		t.Fatalf("Validate() error = %T, want *ValidationError", err)
	}
	if got, want := len(validationError.Fields), 8; got != want {
		t.Fatalf("field errors = %d, want %d: %v", got, want, err)
	}
}

func TestOptionsValidateRejectsUnsafeUserAgent(t *testing.T) {
	options := DefaultOptions()
	options.UserAgent = "scoutly/test\r\nX-Injected: true"

	err := options.Validate()
	var validationError *ValidationError
	if !errors.As(err, &validationError) {
		t.Fatalf("Validate() error = %T, want *ValidationError", err)
	}
	if len(validationError.Fields) != 1 ||
		validationError.Fields[0].Field != "user_agent" {
		t.Fatalf("validation fields = %#v", validationError.Fields)
	}
}

func TestOptionsValidateRules(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		rules Rules
		field string
	}{
		{name: "valid", rules: Rules{"title_too_short": RuleLevelWarning}},
		{name: "unknown rule", rules: Rules{"unknown_rule": RuleLevelOff}, field: "rules.unknown_rule"},
		{name: "noncanonical rule", rules: Rules{"title-too-short": RuleLevelOff}, field: "rules.title-too-short"},
		{name: "invalid level", rules: Rules{"title_too_short": RuleLevel("sometimes")}, field: "rules.title_too_short"},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			options := DefaultOptions()
			options.Rules = test.rules

			err := options.Validate()
			if test.field == "" {
				if err != nil {
					t.Fatalf("Validate() error = %v", err)
				}
				return
			}
			if err == nil || !strings.Contains(err.Error(), test.field) {
				t.Fatalf("Validate() error = %v, want field %q", err, test.field)
			}
		})
	}
}
