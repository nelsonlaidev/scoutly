package config

import (
	"reflect"
	"strconv"
	"testing"
	"time"
)

func TestDefaults(t *testing.T) {
	want := Config{
		MaxDepth:            10,
		MaxPages:            500,
		KeepFragments:       false,
		IgnoreRedirects:     false,
		RateLimit:           0,
		RespectRobots:       true,
		Sitemaps:            true,
		Images:              true,
		MaxSitemapDocuments: 1000,
		Timeout:             30_000,
		MaxRedirects:        10,
		UserAgent:           "scoutly/1.2.3",
		Concurrency:         20,
		Format:              "text",
		Progress:            "auto",
	}

	if got := defaults("1.2.3"); !reflect.DeepEqual(got, want) {
		t.Fatalf("Defaults() = %#v, want %#v", got, want)
	}
}

func TestDefaultsUsesDevelopmentVersionFallback(t *testing.T) {
	if got := defaults("").UserAgent; got != "scoutly/dev" {
		t.Fatalf("UserAgent = %q, want %q", got, "scoutly/dev")
	}
}

func TestApplyOverridesEveryField(t *testing.T) {
	base := Config{
		MaxDepth:            1,
		MaxPages:            1,
		KeepFragments:       true,
		IgnoreRedirects:     true,
		RateLimit:           1,
		RespectRobots:       true,
		Sitemaps:            true,
		Images:              true,
		MaxSitemapDocuments: 1,
		Timeout:             1,
		MaxRedirects:        1,
		UserAgent:           "before",
		Concurrency:         1,
		Format:              "text",
		Progress:            "auto",
	}
	want := Config{
		MaxDepth:            0,
		MaxPages:            2,
		KeepFragments:       false,
		IgnoreRedirects:     false,
		RateLimit:           0,
		RespectRobots:       false,
		Sitemaps:            false,
		Images:              false,
		MaxSitemapDocuments: 2,
		Timeout:             2,
		MaxRedirects:        0,
		UserAgent:           "after",
		Concurrency:         2,
		Format:              "json",
		Progress:            "never",
	}
	overrides := Overrides{
		MaxDepth:            new(0),
		MaxPages:            new(2),
		KeepFragments:       new(false),
		IgnoreRedirects:     new(false),
		RateLimit:           new(0.0),
		RespectRobots:       new(false),
		Sitemaps:            new(false),
		Images:              new(false),
		MaxSitemapDocuments: new(2),
		Timeout:             new(2),
		MaxRedirects:        new(0),
		UserAgent:           new("after"),
		Concurrency:         new(2),
		Format:              new("json"),
		Progress:            new("never"),
	}

	got := apply(base, overrides)
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("apply() = %#v, want %#v", got, want)
	}
	if base.MaxDepth != 1 || !base.RespectRobots {
		t.Fatalf("apply() modified base: %#v", base)
	}
}

func TestApplyIgnoresUnsetOverrides(t *testing.T) {
	base := defaults("test")
	if got := apply(base, Overrides{}); !reflect.DeepEqual(got, base) {
		t.Fatalf("apply() = %#v, want unchanged %#v", got, base)
	}
}

func TestResolveValidatesEffectiveConfig(t *testing.T) {
	base := defaults("test")
	base.MaxPages = 0

	cfg, err := Resolve(base, Overrides{MaxPages: new(2)})
	if err != nil {
		t.Fatalf("Resolve() error = %v", err)
	}
	if cfg.MaxPages != 2 {
		t.Fatalf("MaxPages = %d, want 2", cfg.MaxPages)
	}

	if _, err := Resolve(base, Overrides{}); err == nil {
		t.Fatal("Resolve() error = nil for invalid effective config")
	}
}

func TestAuditOptionsRejectsTimeoutOverflow(t *testing.T) {
	if strconv.IntSize < 64 {
		t.Skip("int cannot represent a time.Duration-overflowing millisecond value")
	}
	cfg := defaults("test")
	cfg.Timeout = int(maxTimeoutMilliseconds + 1)

	if got := cfg.AuditOptions().Timeout; got != 0 {
		t.Fatalf("Timeout = %s, want invalid zero duration", got)
	}
	if _, err := Resolve(cfg, Overrides{}); err == nil {
		t.Fatal("Resolve() accepted an overflowing timeout")
	}

	cfg.Timeout = int(maxTimeoutMilliseconds)
	if got := cfg.AuditOptions().Timeout; got != time.Duration(cfg.Timeout)*time.Millisecond {
		t.Fatalf("Timeout = %s", got)
	}
}
