package audit

import (
	"fmt"
	"math"
	"net/url"
	"slices"
	"strings"
	"time"

	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

// Options controls an audit. Start with DefaultOptions and override the fields
// that need to differ for a particular audit.
type Options struct {
	MaxDepth            int
	MaxPages            int
	KeepFragments       bool
	IgnoreRedirects     bool
	RateLimit           float64
	RespectRobots       bool
	Sitemaps            bool
	Images              bool
	MaxSitemapDocuments int
	Timeout             time.Duration
	MaxRedirects        int
	UserAgent           string
	Concurrency         int
	Rules               Rules
	// IncludePaths and ExcludePaths restrict page crawling by path prefix.
	// Referenced links and images are still checked, including outside this scope.
	IncludePaths []string
	ExcludePaths []string
	// IgnoreRules suppresses findings for matching target URLs, not source pages.
	IgnoreRules []RuleIgnore
}

// RuleIgnore suppresses the named rules for a URL origin and path prefix.
// URLPrefix must be an absolute HTTP(S) URL without credentials, query or fragment.
type RuleIgnore struct {
	URLPrefix string   `json:"url_prefix" yaml:"url_prefix" toml:"url_prefix"`
	Rules     []string `json:"rules" yaml:"rules" toml:"rules"`
}

// Clone copies the rule names so callers can safely retain independent settings.
func (ignore RuleIgnore) Clone() RuleIgnore {
	ignore.Rules = slices.Clone(ignore.Rules)
	return ignore
}

// AllowsPage reports whether a parsed URL is within the configured path scope.
// Options must pass Validate before use. Origin restrictions are handled by the crawler.
func (options Options) AllowsPage(candidate *url.URL) bool {
	if candidate == nil {
		return false
	}
	path := candidate.Path
	if path == "" {
		path = "/"
	}
	for _, prefix := range options.ExcludePaths {
		parsed, _ := url.Parse(prefix)
		if urlutil.MatchesPathPrefix(path, parsed.Path) {
			return false
		}
	}
	if len(options.IncludePaths) == 0 {
		return true
	}
	for _, prefix := range options.IncludePaths {
		parsed, _ := url.Parse(prefix)
		if urlutil.MatchesPathPrefix(path, parsed.Path) {
			return true
		}
	}
	return false
}

// DefaultOptions returns the default audit settings.
func DefaultOptions() Options {
	return Options{
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
		Rules:               make(Rules),
	}
}

// FieldError describes one invalid option.
type FieldError struct {
	Field   string
	Message string
}

// ValidationError reports every invalid option in one error.
type ValidationError struct {
	Fields []FieldError
}

func (e *ValidationError) Error() string {
	lines := make([]string, 0, len(e.Fields)+1)
	lines = append(lines, "invalid options:")
	for _, field := range e.Fields {
		lines = append(lines, fmt.Sprintf("- %s %s", field.Field, field.Message))
	}

	return strings.Join(lines, "\n")
}

// Validate checks that all options are usable by Audit.
func (options Options) Validate() error {
	fields := make([]FieldError, 0)
	add := func(field, message string) {
		fields = append(fields, FieldError{Field: field, Message: message})
	}

	if options.MaxDepth < 0 {
		add("max_depth", "must be greater than or equal to 0")
	}
	if options.MaxPages <= 0 {
		add("max_pages", "must be greater than 0")
	}
	if options.RateLimit < 0 || math.IsInf(options.RateLimit, 0) || math.IsNaN(options.RateLimit) {
		add("rate_limit", "must be finite and greater than or equal to 0")
	}
	if options.MaxSitemapDocuments <= 0 {
		add("max_sitemap_documents", "must be greater than 0")
	}
	if options.Timeout <= 0 {
		add("timeout", "must be greater than 0")
	}
	if options.MaxRedirects < 0 {
		add("max_redirects", "must be greater than or equal to 0")
	}
	if options.UserAgent == "" {
		add("user_agent", "is required")
	} else if strings.ContainsAny(options.UserAgent, "\r\n") {
		add("user_agent", "must not contain a newline")
	}
	if options.Concurrency <= 0 {
		add("concurrency", "must be greater than 0")
	}

	ruleCodes := make([]string, 0, len(options.Rules))
	for name := range options.Rules {
		ruleCodes = append(ruleCodes, name)
	}
	slices.Sort(ruleCodes)
	for _, name := range ruleCodes {
		level := options.Rules[name]
		if _, exists := defaultRuleLevels[name]; !exists {
			add("rules."+name, "is not a known rule")
			continue
		}
		if !isValidRuleLevel(level) {
			add("rules."+name, "must be one of: off, info, warning, error")
		}
	}
	for _, list := range []struct {
		name  string
		paths []string
	}{{"include_paths", options.IncludePaths}, {"exclude_paths", options.ExcludePaths}} {
		for index, path := range list.paths {
			parsed, err := url.Parse(path)
			if err != nil || !strings.HasPrefix(path, "/") || strings.HasPrefix(path, "//") || parsed.Host != "" ||
				strings.ContainsAny(path, "?#*[]") {
				add(fmt.Sprintf("%s[%d]", list.name, index), "must be an absolute URL path without query, fragment or wildcards")
			}
		}
	}
	for index, ignore := range options.IgnoreRules {
		field := fmt.Sprintf("ignore_rules[%d]", index)
		parsed, err := url.Parse(ignore.URLPrefix)
		if err != nil || !urlutil.IsHTTPWithHost(parsed) || parsed.Hostname() == "" ||
			parsed.User != nil || strings.ContainsAny(ignore.URLPrefix, "?#*") || strings.ContainsAny(parsed.Path, "[]") {
			add(field+".url_prefix", "must be an HTTP(S) URL without credentials, query, fragment or wildcards")
		} else if _, err := urlutil.ParseTarget(ignore.URLPrefix); err != nil {
			add(field+".url_prefix", "must be a valid HTTP(S) URL")
		}
		if len(ignore.Rules) == 0 {
			add(field+".rules", "must contain at least one rule")
		}
		for ruleIndex, name := range ignore.Rules {
			if _, exists := defaultRuleLevels[name]; !exists {
				add(fmt.Sprintf("%s.rules[%d]", field, ruleIndex), "must be a known snake_case rule name")
			}
		}
	}

	if len(fields) > 0 {
		return &ValidationError{Fields: fields}
	}

	return nil
}
