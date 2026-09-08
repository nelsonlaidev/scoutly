package main

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/alecthomas/kong"

	"github.com/nelsonlaidev/scoutly/audit"
	"github.com/nelsonlaidev/scoutly/internal/config"
)

func TestParseOptionsPreservesUnsetFields(t *testing.T) {
	options, _, err := parseOptions(nil)
	if err != nil {
		t.Fatalf("parseOptions() error = %v", err)
	}

	if options.KeepFragments != nil || options.RespectRobots != nil || options.Format != nil {
		t.Fatalf("parseOptions() populated unset fields: %#v", options)
	}
}

func TestParseOptionsSupportsNegatableBoolPointers(t *testing.T) {
	tests := []struct {
		name string
		arg  string
		want bool
	}{
		{name: "positive", arg: "--respect-robots", want: true},
		{name: "negative", arg: "--no-respect-robots", want: false},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			options, _, err := parseOptions([]string{test.arg})
			if err != nil {
				t.Fatalf("parseOptions() error = %v", err)
			}
			if options.RespectRobots == nil || *options.RespectRobots != test.want {
				t.Fatalf("RespectRobots = %v, want %t", options.RespectRobots, test.want)
			}
		})
	}
}

func TestParseOptionsAcceptsZeroRateLimit(t *testing.T) {
	options, _, err := parseOptions([]string{"--rate-limit=0"})
	if err != nil {
		t.Fatalf("parseOptions() error = %v", err)
	}
	if options.RateLimit == nil || *options.RateLimit != 0 {
		t.Fatalf("RateLimit = %v, want 0", options.RateLimit)
	}
}

func TestMainPrintsVersion(t *testing.T) {
	//nolint:gosec // os.Args[0] is the current test binary, not user-controlled input.
	command := exec.Command(os.Args[0], "-test.run=^TestMainPrintsVersionHelperProcess$")
	command.Env = append(os.Environ(), "SCOUTLY_TEST_VERSION=1.2.3")

	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("scoutly --version error = %v, output = %q", err, output)
	}
	if got, want := string(output), "scoutly 1.2.3\n"; got != want {
		t.Fatalf("scoutly --version output = %q, want %q", got, want)
	}
}

func TestMainPrintsVersionHelperProcess(t *testing.T) {
	testVersion := os.Getenv("SCOUTLY_TEST_VERSION")
	if testVersion == "" {
		return
	}

	version = testVersion
	os.Args = []string{"scoutly", "--version"}
	main()
}

func TestResolveMapsEveryOverride(t *testing.T) {
	options := Options{
		MaxDepth:            new(1),
		MaxPages:            new(2),
		KeepFragments:       new(true),
		IgnoreRedirects:     new(true),
		RateLimit:           new(3.0),
		RespectRobots:       new(false),
		Sitemaps:            new(false),
		Images:              new(false),
		MaxSitemapDocuments: new(4),
		Timeout:             new(5),
		MaxRedirects:        new(6),
		UserAgent:           new("agent"),
		Concurrency:         new(7),
		Format:              new("json"),
		Progress:            new("never"),
	}
	want := config.Config{
		MaxDepth:            1,
		MaxPages:            2,
		KeepFragments:       true,
		IgnoreRedirects:     true,
		RateLimit:           3,
		RespectRobots:       false,
		Sitemaps:            false,
		Images:              false,
		MaxSitemapDocuments: 4,
		Timeout:             5,
		MaxRedirects:        6,
		UserAgent:           "agent",
		Concurrency:         7,
		Format:              "json",
		Progress:            "never",
		Rules:               audit.Rules{},
	}

	_, got, err := options.resolve(t.TempDir())
	if err != nil {
		t.Fatalf("resolve() error = %v", err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("resolve() config = %#v, want %#v", got, want)
	}
}

func TestParseOptionsRejectsConflictingConfigSelection(t *testing.T) {
	if _, _, err := parseOptions([]string{"--config=scoutly.toml", "--no-config"}); err == nil {
		t.Fatal("parseOptions() error = nil")
	}
}

func TestRunRejectsEmptyConfigPath(t *testing.T) {
	if err := run([]string{"--config="}, t.TempDir()); err == nil {
		t.Fatal("run() error = nil")
	}
}

func TestOptionsRunReportsResolveErrors(t *testing.T) {
	empty := ""
	if err := (&Options{Config: &empty}).Run(); err == nil {
		t.Fatal("Options.Run(empty config) error = nil")
	}

	missing := filepath.Join(t.TempDir(), "missing.toml")
	if err := (&Options{Config: &missing}).Run(); err == nil {
		t.Fatal("Options.Run(missing config) error = nil")
	}
}

func TestOptionsRunMapsInterruptToExitCodeError(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("os.Process.Signal(os.Interrupt) is not supported on Windows")
	}

	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		close(started)
		<-request.Context().Done()
	}))
	defer server.Close()

	target := server.URL
	options := Options{
		Target:        &target,
		NoConfig:      true,
		MaxDepth:      new(0),
		MaxPages:      new(1),
		RespectRobots: new(false),
		Sitemaps:      new(false),
		Images:        new(false),
		Progress:      new("never"),
	}
	errorsChannel := make(chan error, 1)
	go func() {
		errorsChannel <- options.Run()
	}()

	select {
	case <-started:
	case <-time.After(5 * time.Second):
		t.Fatal("audit request did not start")
	}
	process, err := os.FindProcess(os.Getpid())
	if err != nil {
		t.Fatalf("os.FindProcess() error = %v", err)
	}
	if err := process.Signal(os.Interrupt); err != nil {
		t.Fatalf("Signal() error = %v", err)
	}

	select {
	case err := <-errorsChannel:
		if _, ok := errors.AsType[*cancelledError](err); !ok {
			t.Fatalf("Options.Run() error = %T %v", err, err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Options.Run() did not stop after interrupt")
	}
}

func TestMainRunsSuccessfulAudit(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		response.Header().Set("Content-Type", "text/html")
		_, _ = response.Write([]byte("<html><title>Home</title><body><h1>Home</h1></body></html>"))
	}))
	defer server.Close()

	originalArgs := os.Args
	os.Args = []string{
		"scoutly",
		"--no-config",
		"--max-depth=0",
		"--max-pages=1",
		"--no-respect-robots",
		"--no-sitemaps",
		"--no-images",
		"--progress=never",
		server.URL,
	}
	t.Cleanup(func() { os.Args = originalArgs })

	originalStdout := os.Stdout
	output, err := os.OpenFile(os.DevNull, os.O_WRONLY, 0)
	if err != nil {
		t.Fatalf("open null output: %v", err)
	}
	os.Stdout = output
	t.Cleanup(func() {
		os.Stdout = originalStdout
		_ = output.Close()
	})

	main()
}

func TestRunResolvesExplicitConfigFromWorkingDirectory(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "custom.toml")
	if err := os.WriteFile(path, []byte("max_pages = 2\n"), 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	if err := run([]string{"--config=custom.toml", "https://example.com"}, directory); err != nil {
		t.Fatalf("run() error = %v", err)
	}
}

func TestRunAllowsCLIToOverrideInvalidFileValue(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "scoutly.toml")
	if err := os.WriteFile(path, []byte("max_pages = 0\n"), 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	if err := run([]string{"--max-pages=2", "https://example.com"}, directory); err != nil {
		t.Fatalf("run() error = %v", err)
	}
}

func TestRunRejectsInvalidEffectiveConfig(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "scoutly.toml")
	if err := os.WriteFile(path, []byte("max_pages = 0\n"), 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	if err := run([]string{"https://example.com"}, directory); err == nil {
		t.Fatal("run() error = nil")
	}
}

func TestRunRejectsInvalidTarget(t *testing.T) {
	targets := []string{
		"example.com",
		"http://:8080",
		"http://user@:80",
		"https://example.com:65536",
		"https://example.com:0",
	}

	for _, target := range targets {
		t.Run(target, func(t *testing.T) {
			options := Options{
				Target:   new(target),
				NoConfig: true,
			}
			if err := options.Run(); err == nil {
				t.Fatal("Options.Run() error = nil")
			}
		})
	}
}

func TestRunAcceptsCaseInsensitiveHTTPScheme(t *testing.T) {
	if err := run([]string{"HTTP://example.com"}, t.TempDir()); err != nil {
		t.Fatalf("run() error = %v", err)
	}
}

func TestResolveAllowsMissingTargetForTUI(t *testing.T) {
	options, _, err := parseOptions(nil)
	if err != nil {
		t.Fatalf("parseOptions() error = %v", err)
	}
	target, _, err := options.resolve(t.TempDir())
	if err != nil {
		t.Fatalf("resolve() error = %v", err)
	}
	if target != "" {
		t.Fatalf("target = %q, want empty", target)
	}
}

func TestRunRejectsTUIWithoutTerminalOutput(t *testing.T) {
	reader, writer, err := os.Pipe()
	if err != nil {
		t.Fatalf("os.Pipe() error = %v", err)
	}
	defer func() {
		_ = reader.Close()
		_ = writer.Close()
	}()

	stdout := os.Stdout
	os.Stdout = writer
	defer func() {
		os.Stdout = stdout
	}()

	err = (&Options{NoConfig: true}).Run()
	if err == nil || !strings.Contains(err.Error(), "TUI requires a terminal") {
		t.Fatalf("Run() error = %v", err)
	}
}

func TestOptionsRunStartsTUIOnTerminal(t *testing.T) {
	terminal, err := os.OpenFile("/dev/tty", os.O_RDWR, 0)
	if err != nil {
		t.Skip("test process has no controlling terminal")
	}
	defer func() { _ = terminal.Close() }()

	originalStdin, originalStdout := os.Stdin, os.Stdout
	os.Stdin, os.Stdout = terminal, terminal
	defer func() { os.Stdin, os.Stdout = originalStdin, originalStdout }()

	time.AfterFunc(100*time.Millisecond, func() {
		process, findErr := os.FindProcess(os.Getpid())
		if findErr == nil {
			_ = process.Signal(os.Interrupt)
		}
	})
	err = (&Options{NoConfig: true}).Run()
	if _, ok := errors.AsType[*cancelledError](err); !ok {
		t.Fatalf("Options.Run() error = %T %v", err, err)
	}
}

func TestCancelledError(t *testing.T) {
	err := &cancelledError{}

	if got := err.Error(); got != "audit canceled" {
		t.Fatalf("Error() = %q, want %q", got, "audit canceled")
	}
	if got := err.ExitCode(); got != 130 {
		t.Fatalf("ExitCode() = %d, want 130", got)
	}
}

func parseOptions(args []string) (Options, *kong.Context, error) {
	options := Options{}

	parser, err := kong.New(&options,
		kong.Description("A fast website auditing tool."),
		kong.Vars{"version": "scoutly " + version},
	)
	if err != nil {
		return Options{}, nil, err
	}

	context, err := parser.Parse(args)
	return options, context, err
}

func run(args []string, directory string) (err error) {
	options, _, err := parseOptions(args)
	if err != nil {
		return err
	}

	_, _, err = options.resolve(directory)
	return err
}
