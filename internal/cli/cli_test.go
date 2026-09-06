package cli

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/nelsonlaidev/scoutly/internal/config"
)

func TestRunKeepsProgressOnStderrAndReportOnStdout(t *testing.T) {
	server := httptest.NewServer(auditSite{})
	defer server.Close()

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cfg := testConfig()
	cfg.Progress = "always"
	cfg.MaxDepth = 0
	cfg.MaxPages = 1
	cfg.RespectRobots = false
	cfg.Sitemaps = false
	cfg.Images = false

	err := Run(
		context.Background(),
		server.URL,
		cfg,
		&stdout,
		&stderr,
	)
	if err != nil {
		t.Fatalf("Run() error = %v", err)
	}
	if !strings.Contains(stdout.String(), "Scoutly Audit Report") {
		t.Fatalf("stdout = %q", stdout.String())
	}
	if strings.Contains(stdout.String(), "[crawl]") {
		t.Fatalf("progress leaked to stdout: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "[crawl]") {
		t.Fatalf("stderr = %q", stderr.String())
	}
}

func TestRunReturnsAuditErrorWithoutReport(t *testing.T) {
	var stdout bytes.Buffer
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := Run(
		ctx,
		"https://example.com",
		testConfig(),
		&stdout,
		&bytes.Buffer{},
	)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Run() error = %v", err)
	}
	if stdout.Len() != 0 {
		t.Fatalf("stdout = %q", stdout.String())
	}
}

func TestRunReturnsRobotsFailureWithoutReport(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
		<-request.Context().Done()
	}))
	defer server.Close()

	var stdout bytes.Buffer
	cfg := testConfig()
	cfg.Timeout = 25

	err := Run(
		context.Background(),
		server.URL,
		cfg,
		&stdout,
		&bytes.Buffer{},
	)
	if err == nil || !strings.Contains(err.Error(), "fetch robots.txt") {
		t.Fatalf("Run() error = %v, want robots.txt fetch error", err)
	}
	if stdout.Len() != 0 {
		t.Fatalf("stdout = %q", stdout.String())
	}
}

func TestRunValidatesWritersAndReportsOutputFailure(t *testing.T) {
	t.Parallel()

	cfg := testConfig()
	if err := Run(context.Background(), "https://example.com", cfg, nil, &bytes.Buffer{}); err == nil || !strings.Contains(err.Error(), "stdout writer is nil") {
		t.Fatalf("Run(nil stdout) error = %v", err)
	}
	if err := Run(context.Background(), "https://example.com", cfg, &bytes.Buffer{}, nil); err == nil || !strings.Contains(err.Error(), "stderr writer is nil") {
		t.Fatalf("Run(nil stderr) error = %v", err)
	}

	server := httptest.NewServer(auditSite{})
	defer server.Close()
	cfg.RespectRobots = false
	cfg.Sitemaps = false
	cfg.Images = false
	cfg.MaxDepth = 0
	cfg.MaxPages = 1
	if err := Run(context.Background(), server.URL, cfg, failingWriter{}, &bytes.Buffer{}); err == nil || !strings.Contains(err.Error(), "write audit report") {
		t.Fatalf("Run(failing stdout) error = %v", err)
	}
}

type auditSite struct{}

func (auditSite) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	response.Header().Set("Content-Type", "text/html")
	_, _ = response.Write([]byte("<html><title>Test</title><body></body></html>"))
}

func testConfig() config.Config {
	return config.Config{
		MaxDepth:            10,
		MaxPages:            500,
		RespectRobots:       true,
		Sitemaps:            true,
		Images:              true,
		MaxSitemapDocuments: 1000,
		Timeout:             30_000,
		MaxRedirects:        10,
		UserAgent:           "scoutly/test",
		Concurrency:         20,
		Format:              "text",
		Progress:            "never",
	}
}

type failingWriter struct{}

func (failingWriter) Write([]byte) (int, error) {
	return 0, errors.New("write failed")
}
