package config

import (
	"errors"
	"math"
	"strings"
	"testing"
)

func TestValidateAcceptsBoundaryValues(t *testing.T) {
	cfg := defaults("test")
	cfg.MaxDepth = 0
	cfg.RateLimit = 0
	cfg.MaxRedirects = 0

	if err := validate(cfg); err != nil {
		t.Fatalf("validate() error = %v", err)
	}
}

func TestValidateRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name   string
		field  string
		mutate func(*Config)
	}{
		{name: "max depth", field: "max_depth", mutate: func(cfg *Config) { cfg.MaxDepth = -1 }},
		{name: "max pages", field: "max_pages", mutate: func(cfg *Config) { cfg.MaxPages = 0 }},
		{name: "rate limit", field: "rate_limit", mutate: func(cfg *Config) { cfg.RateLimit = -1 }},
		{name: "max sitemap documents", field: "max_sitemap_documents", mutate: func(cfg *Config) { cfg.MaxSitemapDocuments = 0 }},
		{name: "timeout", field: "timeout", mutate: func(cfg *Config) { cfg.Timeout = 0 }},
		{name: "max redirects", field: "max_redirects", mutate: func(cfg *Config) { cfg.MaxRedirects = -1 }},
		{name: "user agent", field: "user_agent", mutate: func(cfg *Config) { cfg.UserAgent = "" }},
		{name: "concurrency", field: "concurrency", mutate: func(cfg *Config) { cfg.Concurrency = 0 }},
		{name: "format", field: "format", mutate: func(cfg *Config) { cfg.Format = "xml" }},
		{name: "progress", field: "progress", mutate: func(cfg *Config) { cfg.Progress = "sometimes" }},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			cfg := defaults("test")
			test.mutate(&cfg)

			err := validate(cfg)
			if err == nil {
				t.Fatal("validate() error = nil")
			}

			if _, ok := errors.AsType[*validationError](err); !ok {
				t.Fatalf("validate() error type = %T, want *validationError", err)
			}
			if !strings.Contains(err.Error(), test.field) {
				t.Fatalf("validate() error = %q, want field %q", err, test.field)
			}
		})
	}
}

func TestValidateFormatsOneOfError(t *testing.T) {
	cfg := defaults("test")
	cfg.Format = "xml"

	if err := validate(cfg); err == nil || !strings.Contains(err.Error(), "must be one of: text, json") {
		t.Fatalf("validate() error = %v", err)
	}
}

func TestValidateRejectsNonFiniteRateLimit(t *testing.T) {
	values := []float64{math.Inf(1), math.Inf(-1), math.NaN()}
	for _, value := range values {
		cfg := defaults("test")
		cfg.RateLimit = value
		if err := validate(cfg); err == nil {
			t.Fatalf("validate() accepted RateLimit %v", value)
		}
	}
}
