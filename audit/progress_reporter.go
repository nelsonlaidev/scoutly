package audit

import (
	"net/url"
	"sync"

	"github.com/nelsonlaidev/scoutly/internal/ptrutil"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

type progressReporter struct {
	mu       sync.Mutex
	callback func(Progress) error
	state    Progress
	err      error
}

func newProgressReporter(callback func(Progress) error) *progressReporter {
	return &progressReporter{
		callback: callback,
		state: Progress{
			Phase:    PhaseRobots,
			Pages:    PageProgress{},
			Sitemaps: SitemapProgress{},
			Links:    ResourceProgress{},
			Images:   ResourceProgress{},
		},
	}
}

func (reporter *progressReporter) setPhase(phase Phase, currentURL *url.URL) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Phase = phase
	reporter.state.CurrentURL = urlutil.String(currentURL)
	return reporter.emitLocked()
}

func (reporter *progressReporter) pageDiscovered(currentURL *url.URL) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Pages.Discovered++
	reporter.state.CurrentURL = urlutil.String(currentURL)
	return reporter.emitLocked()
}

func (reporter *progressReporter) pageCrawled(currentURL *url.URL) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Pages.Crawled++
	reporter.state.CurrentURL = urlutil.String(currentURL)
	return reporter.emitLocked()
}

func (reporter *progressReporter) sitemapFetched(currentURL *url.URL) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Sitemaps.Fetched++
	reporter.state.CurrentURL = urlutil.String(currentURL)
	return reporter.emitLocked()
}

func (reporter *progressReporter) linksStarted(total int) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Phase = PhaseLinks
	reporter.state.CurrentURL = ""
	reporter.state.Links.Total = new(total)
	return reporter.emitLocked()
}

func (reporter *progressReporter) linkChecked(currentURL *url.URL) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Links.Checked++
	reporter.state.CurrentURL = urlutil.String(currentURL)
	return reporter.emitLocked()
}

func (reporter *progressReporter) imagesStarted(total int) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Phase = PhaseImages
	reporter.state.CurrentURL = ""
	reporter.state.Images.Total = new(total)
	return reporter.emitLocked()
}

func (reporter *progressReporter) imageChecked(currentURL string) error {
	reporter.mu.Lock()
	defer reporter.mu.Unlock()
	if reporter.err != nil {
		return reporter.err
	}

	reporter.state.Images.Checked++
	reporter.state.CurrentURL = currentURL
	return reporter.emitLocked()
}

func (reporter *progressReporter) emitLocked() error {
	if reporter.err != nil {
		return reporter.err
	}

	if reporter.callback == nil {
		return nil
	}

	snapshot := cloneProgress(reporter.state)
	if err := reporter.callback(snapshot); err != nil {
		reporter.err = err
		return err
	}

	return nil
}

func cloneProgress(progress Progress) Progress {
	progress.Links.Total = ptrutil.Clone(progress.Links.Total)
	progress.Images.Total = ptrutil.Clone(progress.Images.Total)
	return progress
}
