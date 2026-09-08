package config

import (
	"maps"
	"time"

	"github.com/nelsonlaidev/scoutly/audit"
)

const maxTimeoutMilliseconds = 9_223_372_036_854

type Config struct {
	MaxDepth            int
	MaxPages            int
	KeepFragments       bool
	IgnoreRedirects     bool
	RateLimit           float64
	RespectRobots       bool
	Sitemaps            bool
	Images              bool
	MaxSitemapDocuments int
	Timeout             int
	MaxRedirects        int
	UserAgent           string
	Concurrency         int
	Format              string
	Progress            string
	Rules               audit.Rules
}

type Overrides struct {
	MaxDepth            *int        `json:"max_depth" yaml:"max_depth" toml:"max_depth"`
	MaxPages            *int        `json:"max_pages" yaml:"max_pages" toml:"max_pages"`
	KeepFragments       *bool       `json:"keep_fragments" yaml:"keep_fragments" toml:"keep_fragments"`
	IgnoreRedirects     *bool       `json:"ignore_redirects" yaml:"ignore_redirects" toml:"ignore_redirects"`
	RateLimit           *float64    `json:"rate_limit" yaml:"rate_limit" toml:"rate_limit"`
	RespectRobots       *bool       `json:"respect_robots" yaml:"respect_robots" toml:"respect_robots"`
	Sitemaps            *bool       `json:"sitemaps" yaml:"sitemaps" toml:"sitemaps"`
	Images              *bool       `json:"images" yaml:"images" toml:"images"`
	MaxSitemapDocuments *int        `json:"max_sitemap_documents" yaml:"max_sitemap_documents" toml:"max_sitemap_documents"`
	Timeout             *int        `json:"timeout" yaml:"timeout" toml:"timeout"`
	MaxRedirects        *int        `json:"max_redirects" yaml:"max_redirects" toml:"max_redirects"`
	UserAgent           *string     `json:"user_agent" yaml:"user_agent" toml:"user_agent"`
	Concurrency         *int        `json:"concurrency" yaml:"concurrency" toml:"concurrency"`
	Format              *string     `json:"format" yaml:"format" toml:"format"`
	Progress            *string     `json:"progress" yaml:"progress" toml:"progress"`
	Rules               audit.Rules `json:"rules" yaml:"rules" toml:"rules"`
}

func defaults(version string) Config {
	if version == "" {
		version = "dev"
	}

	auditOptions := audit.DefaultOptions()
	auditOptions.UserAgent = "scoutly/" + version

	return Config{
		MaxDepth:            auditOptions.MaxDepth,
		MaxPages:            auditOptions.MaxPages,
		KeepFragments:       auditOptions.KeepFragments,
		IgnoreRedirects:     auditOptions.IgnoreRedirects,
		RateLimit:           auditOptions.RateLimit,
		RespectRobots:       auditOptions.RespectRobots,
		Sitemaps:            auditOptions.Sitemaps,
		Images:              auditOptions.Images,
		MaxSitemapDocuments: auditOptions.MaxSitemapDocuments,
		Timeout:             int(auditOptions.Timeout / time.Millisecond),
		MaxRedirects:        auditOptions.MaxRedirects,
		UserAgent:           auditOptions.UserAgent,
		Concurrency:         auditOptions.Concurrency,
		Format:              "text",
		Progress:            "auto",
		Rules:               maps.Clone(auditOptions.Rules),
	}
}

func (cfg Config) AuditOptions() audit.Options {
	return audit.Options{
		MaxDepth:            cfg.MaxDepth,
		MaxPages:            cfg.MaxPages,
		KeepFragments:       cfg.KeepFragments,
		IgnoreRedirects:     cfg.IgnoreRedirects,
		RateLimit:           cfg.RateLimit,
		RespectRobots:       cfg.RespectRobots,
		Sitemaps:            cfg.Sitemaps,
		Images:              cfg.Images,
		MaxSitemapDocuments: cfg.MaxSitemapDocuments,
		Timeout:             timeoutDuration(cfg.Timeout),
		MaxRedirects:        cfg.MaxRedirects,
		UserAgent:           cfg.UserAgent,
		Concurrency:         cfg.Concurrency,
		Rules:               maps.Clone(cfg.Rules),
	}
}

func timeoutDuration(milliseconds int) time.Duration {
	if milliseconds <= 0 || int64(milliseconds) > maxTimeoutMilliseconds {
		return 0
	}
	return time.Duration(milliseconds) * time.Millisecond
}

func apply(base Config, overrides Overrides) Config {
	base.Rules = maps.Clone(base.Rules)
	if overrides.MaxDepth != nil {
		base.MaxDepth = *overrides.MaxDepth
	}
	if overrides.MaxPages != nil {
		base.MaxPages = *overrides.MaxPages
	}
	if overrides.KeepFragments != nil {
		base.KeepFragments = *overrides.KeepFragments
	}
	if overrides.IgnoreRedirects != nil {
		base.IgnoreRedirects = *overrides.IgnoreRedirects
	}
	if overrides.RateLimit != nil {
		base.RateLimit = *overrides.RateLimit
	}
	if overrides.RespectRobots != nil {
		base.RespectRobots = *overrides.RespectRobots
	}
	if overrides.Sitemaps != nil {
		base.Sitemaps = *overrides.Sitemaps
	}
	if overrides.Images != nil {
		base.Images = *overrides.Images
	}
	if overrides.MaxSitemapDocuments != nil {
		base.MaxSitemapDocuments = *overrides.MaxSitemapDocuments
	}
	if overrides.Timeout != nil {
		base.Timeout = *overrides.Timeout
	}
	if overrides.MaxRedirects != nil {
		base.MaxRedirects = *overrides.MaxRedirects
	}
	if overrides.UserAgent != nil {
		base.UserAgent = *overrides.UserAgent
	}
	if overrides.Concurrency != nil {
		base.Concurrency = *overrides.Concurrency
	}
	if overrides.Format != nil {
		base.Format = *overrides.Format
	}
	if overrides.Progress != nil {
		base.Progress = *overrides.Progress
	}
	if overrides.Rules != nil {
		if base.Rules == nil {
			base.Rules = make(audit.Rules, len(overrides.Rules))
		}
		maps.Copy(base.Rules, overrides.Rules)
	}

	return base
}

func Resolve(base Config, overrides Overrides) (Config, error) {
	cfg := apply(base, overrides)
	if err := validate(cfg); err != nil {
		return cfg, err
	}

	return cfg, nil
}
