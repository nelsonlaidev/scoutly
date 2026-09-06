package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/signal"

	"github.com/alecthomas/kong"
	"golang.org/x/term"

	"github.com/nelsonlaidev/scoutly/internal/cli"
	"github.com/nelsonlaidev/scoutly/internal/config"
	"github.com/nelsonlaidev/scoutly/internal/tui"
)

var version = "dev"

type Options struct {
	Version kong.VersionFlag `name:"version" help:"Print version information and quit."`

	Target *string `arg:"" optional:"" name:"target" help:"The website target URL to audit."`

	Config   *string `name:"config" xor:"config-selection" help:"Path to a Scoutly config file."`
	NoConfig bool    `name:"no-config" xor:"config-selection" help:"Disable automatic config discovery."`

	MaxDepth            *int     `name:"max-depth" help:"Maximum depth to crawl."`
	MaxPages            *int     `name:"max-pages" help:"Maximum number of pages to crawl."`
	KeepFragments       *bool    `name:"keep-fragments" negatable:"" help:"Keep URL fragments when crawling."`
	IgnoreRedirects     *bool    `name:"ignore-redirects" negatable:"" help:"Do not report redirects as issues."`
	RateLimit           *float64 `name:"rate-limit" help:"Maximum HTTP requests per second."`
	RespectRobots       *bool    `name:"respect-robots" negatable:"" help:"Respect robots.txt crawl rules."`
	Sitemaps            *bool    `name:"sitemaps" negatable:"" help:"Discover pages from XML sitemaps."`
	Images              *bool    `name:"images" negatable:"" help:"Check discovered images."`
	MaxSitemapDocuments *int     `name:"max-sitemap-documents" help:"Maximum sitemap documents to fetch."`
	Timeout             *int     `name:"timeout" help:"Request timeout in milliseconds."`
	MaxRedirects        *int     `name:"max-redirects" help:"Maximum number of redirects per request."`
	UserAgent           *string  `name:"user-agent" help:"User-Agent header sent with requests."`
	Concurrency         *int     `name:"concurrency" help:"Maximum number of concurrent page crawls, link checks, and image checks."`

	Format   *string `name:"format" enum:"text,json" help:"Output format."`
	Progress *string `name:"progress" enum:"auto,always,never" help:"Progress display mode."`
}

func (options *Options) Run() error {
	workingDirectory, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("get working directory: %w", err)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()

	target, cfg, err := options.resolve(workingDirectory)
	if err != nil {
		return err
	}

	if options.Target == nil {
		if !term.IsTerminal(int(os.Stdin.Fd())) ||
			!term.IsTerminal(int(os.Stdout.Fd())) {
			return errors.New("TUI requires a terminal; pass a target URL for non-interactive mode")
		}
		err = tui.Run(ctx, cfg.AuditOptions())
	} else {
		err = cli.Run(ctx, target, cfg, os.Stdout, os.Stderr)
	}

	if errors.Is(err, context.Canceled) {
		return &cancelledError{}
	}

	return err
}

func (options Options) resolve(workingDirectory string) (string, config.Config, error) {
	var configPath string
	if options.Config != nil {
		if *options.Config == "" {
			return "", config.Config{}, fmt.Errorf("--config requires a non-empty path")
		}
		configPath = *options.Config
	}

	loaded, err := config.Load(config.LoadOptions{
		Directory:        workingDirectory,
		ConfigPath:       configPath,
		DisableDiscovery: options.NoConfig,
		Version:          version,
	})
	if err != nil {
		return "", config.Config{}, err
	}

	cfg, err := config.Resolve(loaded.Base, config.Overrides{
		MaxDepth:            options.MaxDepth,
		MaxPages:            options.MaxPages,
		KeepFragments:       options.KeepFragments,
		IgnoreRedirects:     options.IgnoreRedirects,
		RateLimit:           options.RateLimit,
		RespectRobots:       options.RespectRobots,
		Sitemaps:            options.Sitemaps,
		Images:              options.Images,
		MaxSitemapDocuments: options.MaxSitemapDocuments,
		Timeout:             options.Timeout,
		MaxRedirects:        options.MaxRedirects,
		UserAgent:           options.UserAgent,
		Concurrency:         options.Concurrency,
		Format:              options.Format,
		Progress:            options.Progress,
	})
	if err != nil {
		return "", config.Config{}, err
	}

	if options.Target == nil {
		return "", cfg, nil
	}
	return *options.Target, cfg, nil
}

func main() {
	options := Options{}

	ctx := kong.Parse(&options,
		kong.Description("A fast website auditing tool."),
		kong.Vars{"version": "scoutly " + version})

	err := ctx.Run()
	ctx.FatalIfErrorf(err)
}

type cancelledError struct{}

func (*cancelledError) Error() string {
	return "audit canceled"
}

func (*cancelledError) ExitCode() int {
	return 130
}
