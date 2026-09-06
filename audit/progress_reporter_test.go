package audit

import (
	"errors"
	"net/url"
	"testing"
)

func TestProgressReporterEmitsIndependentMonotonicSnapshots(t *testing.T) {
	var snapshots []Progress
	reporter := newProgressReporter(func(progress Progress) error {
		snapshots = append(snapshots, progress)
		return nil
	})
	currentURL, _ := url.Parse("https://example.com")

	if err := reporter.setPhase(PhaseCrawl, currentURL); err != nil {
		t.Fatal(err)
	}
	if err := reporter.pageDiscovered(currentURL); err != nil {
		t.Fatal(err)
	}
	if err := reporter.pageCrawled(currentURL); err != nil {
		t.Fatal(err)
	}
	if err := reporter.sitemapFetched(currentURL); err != nil {
		t.Fatal(err)
	}
	if err := reporter.linksStarted(2); err != nil {
		t.Fatal(err)
	}
	if err := reporter.linkChecked(currentURL); err != nil {
		t.Fatal(err)
	}
	if err := reporter.imagesStarted(3); err != nil {
		t.Fatal(err)
	}
	if err := reporter.imageChecked(currentURL.String()); err != nil {
		t.Fatal(err)
	}

	if got, want := len(snapshots), 8; got != want {
		t.Fatalf("snapshots = %d, want %d", got, want)
	}
	if snapshots[0].Pages.Discovered != 0 || snapshots[2].Pages.Crawled != 1 {
		t.Fatalf("snapshots = %#v", snapshots)
	}
	if snapshots[4].Links.Total == nil || *snapshots[4].Links.Total != 2 {
		t.Fatalf("links total = %v", snapshots[4].Links.Total)
	}
}

func TestProgressReporterStopsAfterCallbackError(t *testing.T) {
	want := errors.New("observer failed")
	reporter := newProgressReporter(func(Progress) error {
		return want
	})

	if err := reporter.setPhase(PhaseReport, nil); !errors.Is(err, want) {
		t.Fatalf("setPhase() error = %v", err)
	}
	if err := reporter.setPhase(PhaseReport, nil); !errors.Is(err, want) {
		t.Fatalf("second setPhase() error = %v", err)
	}
	currentURL, _ := url.Parse("https://example.com")
	for name, call := range map[string]func() error{
		"page discovered": func() error { return reporter.pageDiscovered(currentURL) },
		"page crawled":    func() error { return reporter.pageCrawled(currentURL) },
		"sitemap fetched": func() error { return reporter.sitemapFetched(currentURL) },
		"links started":   func() error { return reporter.linksStarted(1) },
		"link checked":    func() error { return reporter.linkChecked(currentURL) },
		"images started":  func() error { return reporter.imagesStarted(1) },
		"image checked":   func() error { return reporter.imageChecked(currentURL.String()) },
		"emit":            reporter.emitLocked,
	} {
		if err := call(); !errors.Is(err, want) {
			t.Errorf("%s error = %v, want %v", name, err, want)
		}
	}
}
