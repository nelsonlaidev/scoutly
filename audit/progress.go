package audit

// Phase identifies the active audit phase.
type Phase string

const (
	PhaseRobots   Phase = "robots"
	PhaseCrawl    Phase = "crawl"
	PhaseSitemaps Phase = "sitemaps"
	PhaseLinks    Phase = "links"
	PhaseImages   Phase = "images"
	PhaseReport   Phase = "report"
)

// PageProgress describes page discovery and crawl progress.
type PageProgress struct {
	Discovered int `json:"discovered"`
	Crawled    int `json:"crawled"`
}

// SitemapProgress describes sitemap discovery progress.
type SitemapProgress struct {
	Fetched int `json:"fetched"`
}

// ResourceProgress describes link or image checking progress. Total is nil
// until the corresponding checking phase starts.
type ResourceProgress struct {
	Checked int  `json:"checked"`
	Total   *int `json:"total"`
}

// Progress is an immutable snapshot of an audit's progress.
type Progress struct {
	Phase      Phase            `json:"phase"`
	CurrentURL string           `json:"current_url,omitempty"`
	Pages      PageProgress     `json:"pages"`
	Sitemaps   SitemapProgress  `json:"sitemaps"`
	Links      ResourceProgress `json:"links"`
	Images     ResourceProgress `json:"images"`
}
