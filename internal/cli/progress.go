package cli

import (
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"

	"golang.org/x/term"

	"github.com/nelsonlaidev/scoutly/audit"
	"github.com/nelsonlaidev/scoutly/internal/progressfmt"
)

const progressRefreshInterval = 100 * time.Millisecond

type cliProgress struct {
	enabled      bool
	mu           sync.Mutex
	writer       io.Writer
	terminal     bool
	width        int
	startedAt    time.Time
	lastRendered time.Time
	lastPhase    audit.Phase
	latest       *audit.Progress
	rendered     bool
	spinner      int
	closed       bool
}

func newProgressDisplay(mode string, writer io.Writer) *cliProgress {
	if mode == "never" {
		return &cliProgress{}
	}

	file, isFile := writer.(*os.File)
	isTerminal := isFile && term.IsTerminal(int(file.Fd()))
	if mode == "auto" && !isTerminal {
		return &cliProgress{}
	}

	width := 80
	if isTerminal {
		if terminalWidth, _, err := term.GetSize(int(file.Fd())); err == nil && terminalWidth > 0 {
			width = terminalWidth
		}
	}

	return &cliProgress{
		enabled:   true,
		writer:    writer,
		terminal:  isTerminal,
		width:     width,
		startedAt: time.Now(),
	}
}

func (progress *cliProgress) Update(snapshot audit.Progress) error {
	if !progress.enabled {
		return nil
	}
	progress.mu.Lock()
	defer progress.mu.Unlock()

	copy := snapshot
	progress.latest = &copy
	now := time.Now()
	if progress.rendered &&
		snapshot.Phase == progress.lastPhase &&
		now.Sub(progress.lastRendered) < progressRefreshInterval {
		return nil
	}

	return progress.render(now)
}

func (progress *cliProgress) Close() error {
	if !progress.enabled {
		return nil
	}
	progress.mu.Lock()
	defer progress.mu.Unlock()
	if progress.closed {
		return nil
	}
	progress.closed = true

	if progress.latest != nil &&
		time.Since(progress.lastRendered) >= progressRefreshInterval {
		if err := progress.render(time.Now()); err != nil {
			return err
		}
	}
	if !progress.terminal || !progress.rendered {
		return nil
	}

	_, err := io.WriteString(progress.writer, "\x1b[3A\r\x1b[2K\n\r\x1b[2K\n\r\x1b[2K")
	return err
}

func (progress *cliProgress) render(now time.Time) error {
	if progress.latest == nil {
		return nil
	}

	snapshot := *progress.latest
	if progress.terminal {
		frame := formatProgressFrame(snapshot, now.Sub(progress.startedAt), progress.spinner, progress.width)
		prefix := ""
		if progress.rendered {
			prefix = "\x1b[3A"
		}
		lines := strings.Split(frame, "\n")
		renderedLines := make([]string, 0, len(lines))
		for _, line := range lines {
			renderedLines = append(renderedLines, "\r\x1b[2K"+line)
		}
		if _, err := io.WriteString(progress.writer, prefix+strings.Join(renderedLines, "\n")+"\n"); err != nil {
			return err
		}
	} else {
		if _, err := fmt.Fprintf(
			progress.writer,
			"[%s] pages %d/%d, sitemaps %d, links %s, images %s%s\n",
			snapshot.Phase,
			snapshot.Pages.Crawled,
			snapshot.Pages.Discovered,
			snapshot.Sitemaps.Fetched,
			progressfmt.Resource(snapshot.Links),
			progressfmt.Resource(snapshot.Images),
			formatCurrentURL(snapshot.CurrentURL),
		); err != nil {
			return err
		}
	}

	progress.rendered = true
	progress.lastRendered = now
	progress.lastPhase = snapshot.Phase
	progress.spinner++
	return nil
}

func formatProgressFrame(progress audit.Progress, elapsed time.Duration, spinnerIndex, width int) string {
	spinners := []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}
	labels := map[audit.Phase]string{
		audit.PhaseRobots:   "Checking robots.txt",
		audit.PhaseCrawl:    "Crawling pages",
		audit.PhaseSitemaps: "Reading sitemaps",
		audit.PhaseLinks:    "Checking links",
		audit.PhaseImages:   "Checking images",
		audit.PhaseReport:   "Building report",
	}
	lineOne := fmt.Sprintf(
		"%s %s · %s",
		spinners[spinnerIndex%len(spinners)],
		labels[progress.Phase],
		progressfmt.Duration(elapsed),
	)
	lineTwo := fmt.Sprintf(
		"Pages %d/%d · Sitemaps %d · Links %s · Images %s",
		progress.Pages.Crawled,
		progress.Pages.Discovered,
		progress.Sitemaps.Fetched,
		progressfmt.Resource(progress.Links),
		progressfmt.Resource(progress.Images),
	)
	currentURL := progress.CurrentURL
	if currentURL == "" {
		currentURL = "Waiting for the next request..."
	}

	return strings.Join([]string{
		truncateRunes(lineOne, width),
		truncateRunes(lineTwo, width),
		truncateRunes(currentURL, width),
	}, "\n")
}

func formatCurrentURL(value string) string {
	if value == "" {
		return ""
	}
	return " · " + value
}

func truncateRunes(value string, maximum int) string {
	if maximum < 1 {
		maximum = 1
	}
	runes := []rune(value)
	if len(runes) <= maximum {
		return value
	}
	if maximum == 1 {
		return "…"
	}
	return string(runes[:maximum-1]) + "…"
}
