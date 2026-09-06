package audit

import (
	"fmt"
	"math"
	"strings"
	"time"
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

	if len(fields) > 0 {
		return &ValidationError{Fields: fields}
	}

	return nil
}
