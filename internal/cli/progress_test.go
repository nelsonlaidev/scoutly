package cli

import (
	"bytes"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/nelsonlaidev/scoutly/audit"
)

func TestNewProgressDisplayModes(t *testing.T) {
	never := newProgressDisplay("never", &bytes.Buffer{})
	if never.enabled {
		t.Fatal("never progress display is enabled")
	}
	if err := never.Close(); err != nil {
		t.Fatal(err)
	}

	var autoOutput bytes.Buffer
	auto := newProgressDisplay("auto", &autoOutput)
	if err := auto.Update(audit.Progress{Phase: audit.PhaseCrawl}); err != nil {
		t.Fatal(err)
	}
	if autoOutput.Len() != 0 {
		t.Fatalf("auto output = %q", autoOutput.String())
	}

	var alwaysOutput bytes.Buffer
	always := newProgressDisplay("always", &alwaysOutput)
	if err := always.Update(audit.Progress{Phase: audit.PhaseCrawl}); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(alwaysOutput.String(), "[crawl]") {
		t.Fatalf("always output = %q", alwaysOutput.String())
	}
}

func TestNewProgressDisplayUsesTerminalWidth(t *testing.T) {
	terminal, err := os.OpenFile("/dev/tty", os.O_RDWR, 0)
	if err != nil {
		t.Skip("test process has no controlling terminal")
	}
	defer func() { _ = terminal.Close() }()

	progress := newProgressDisplay("always", terminal)
	if !progress.enabled || !progress.terminal || progress.width <= 0 {
		t.Fatalf("terminal progress = %#v", progress)
	}
	if err := progress.Update(audit.Progress{Phase: audit.PhaseCrawl}); err != nil {
		t.Fatalf("Update() error = %v", err)
	}
	if err := progress.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestFormatProgressFrame(t *testing.T) {
	total := 3
	frame := formatProgressFrame(audit.Progress{
		Phase:      audit.PhaseLinks,
		CurrentURL: "https://example.com/a/very/long/path",
		Pages:      audit.PageProgress{Discovered: 2, Crawled: 1},
		Links:      audit.ResourceProgress{Checked: 1, Total: &total},
	}, 65*time.Second, 0, 30)

	lines := strings.Split(frame, "\n")
	if len(lines) != 3 {
		t.Fatalf("lines = %d, frame = %q", len(lines), frame)
	}
	for _, line := range lines {
		if len([]rune(line)) > 30 {
			t.Fatalf("line too wide: %q", line)
		}
	}
	if !strings.Contains(lines[0], "1:05") {
		t.Fatalf("frame = %q", frame)
	}
}

func TestProgressRenderCloseAndThrottlingEdges(t *testing.T) {
	t.Parallel()

	var terminalOutput bytes.Buffer
	progress := &cliProgress{
		enabled: true, writer: &terminalOutput, terminal: true, width: 80, startedAt: time.Now(),
	}
	if err := progress.render(time.Now()); err != nil {
		t.Fatal(err)
	}
	progress.latest = &audit.Progress{Phase: audit.PhaseCrawl}
	if err := progress.render(time.Now()); err != nil {
		t.Fatal(err)
	}
	if err := progress.render(time.Now()); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(terminalOutput.String(), "\x1b[3A") {
		t.Fatalf("terminal redraw output = %q", terminalOutput.String())
	}
	if err := progress.Close(); err != nil {
		t.Fatal(err)
	}
	if err := progress.Close(); err != nil {
		t.Fatal(err)
	}

	var output bytes.Buffer
	throttled := &cliProgress{enabled: true, writer: &output, startedAt: time.Now()}
	snapshot := audit.Progress{Phase: audit.PhaseLinks}
	if err := throttled.Update(snapshot); err != nil {
		t.Fatal(err)
	}
	before := output.Len()
	if err := throttled.Update(snapshot); err != nil {
		t.Fatal(err)
	}
	if output.Len() != before {
		t.Fatal("throttled update rendered another frame")
	}
	throttled.lastRendered = time.Now().Add(-progressRefreshInterval)
	if err := throttled.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestProgressWriterErrors(t *testing.T) {
	t.Parallel()

	now := time.Now()
	for _, progress := range []*cliProgress{
		{enabled: true, writer: failingWriter{}, latest: &audit.Progress{Phase: audit.PhaseCrawl}, startedAt: now},
		{enabled: true, writer: failingWriter{}, terminal: true, latest: &audit.Progress{Phase: audit.PhaseCrawl}, startedAt: now},
	} {
		if err := progress.render(now); err == nil {
			t.Fatal("render() error = nil")
		}
	}
	pending := &cliProgress{
		enabled: true, writer: failingWriter{}, latest: &audit.Progress{Phase: audit.PhaseCrawl}, startedAt: now,
		lastRendered: now.Add(-progressRefreshInterval),
	}
	if err := pending.Close(); err == nil {
		t.Fatal("Close() pending render error = nil")
	}
	clearing := &cliProgress{enabled: true, writer: failingWriter{}, terminal: true, rendered: true}
	if err := clearing.Close(); err == nil {
		t.Fatal("Close() clear error = nil")
	}
}

func TestProgressFormattingEdges(t *testing.T) {
	t.Parallel()

	frame := formatProgressFrame(audit.Progress{Phase: audit.PhaseReport}, 0, 11, 80)
	if !strings.Contains(frame, "Waiting for the next request...") {
		t.Fatalf("frame = %q", frame)
	}
	for _, test := range []struct {
		value   string
		maximum int
		want    string
	}{
		{value: "abc", maximum: 0, want: "…"},
		{value: "a", maximum: 1, want: "a"},
		{value: "abc", maximum: 1, want: "…"},
		{value: "abcdef", maximum: 4, want: "abc…"},
	} {
		if got := truncateRunes(test.value, test.maximum); got != test.want {
			t.Errorf("truncateRunes(%q, %d) = %q, want %q", test.value, test.maximum, got, test.want)
		}
	}
}
