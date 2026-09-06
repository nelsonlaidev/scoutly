package audit

import "time"

// Severity classifies the impact of an issue.
type Severity string

const (
	SeverityError   Severity = "error"
	SeverityWarning Severity = "warning"
	SeverityInfo    Severity = "info"
)

// IssueCode is a stable machine-readable issue identifier.
type IssueCode string

const (
	IssueMissingTitle            IssueCode = "missing-title"
	IssueTitleTooShort           IssueCode = "title-too-short"
	IssueTitleTooLong            IssueCode = "title-too-long"
	IssueMissingMetaDescription  IssueCode = "missing-meta-description"
	IssueMetaDescriptionTooShort IssueCode = "meta-description-too-short"
	IssueMetaDescriptionTooLong  IssueCode = "meta-description-too-long"
	IssueMissingImageAlt         IssueCode = "missing-image-alt"
	IssueMissingH1               IssueCode = "missing-h1"
	IssueMultipleH1              IssueCode = "multiple-h1"
	IssueThinContent             IssueCode = "thin-content"
	IssueMissingOGTitle          IssueCode = "missing-og-title"
	IssueMissingOGDescription    IssueCode = "missing-og-description"
	IssueMissingOGImage          IssueCode = "missing-og-image"
	IssueMissingOGURL            IssueCode = "missing-og-url"
	IssueMissingOGType           IssueCode = "missing-og-type"
	IssuePageCrawlFailed         IssueCode = "page-crawl-failed"
	IssuePageHTTPError           IssueCode = "page-http-error"
	IssueBrokenLink              IssueCode = "broken-link"
	IssueLinkCheckBlocked        IssueCode = "link-check-blocked"
	IssueRedirect                IssueCode = "redirect"
	IssueInvalidImageURL         IssueCode = "invalid-image-url"
	IssueBrokenImage             IssueCode = "broken-image"
	IssueImageCheckBlocked       IssueCode = "image-check-blocked"
	IssueInvalidImageContentType IssueCode = "invalid-image-content-type"
	IssueImageRedirect           IssueCode = "image-redirect"
)

// TargetType identifies the kind of resource an issue applies to.
type TargetType string

const (
	TargetPage  TargetType = "page"
	TargetLink  TargetType = "link"
	TargetImage TargetType = "image"
)

// IssueTarget points to the resource affected by an issue.
type IssueTarget struct {
	Type TargetType `json:"type"`
	URL  string     `json:"url"`
}

// Issue is one actionable audit finding.
type Issue struct {
	Code     IssueCode   `json:"code"`
	Severity Severity    `json:"severity"`
	Message  string      `json:"message"`
	Target   IssueTarget `json:"target"`
}

// Headings contains the headings included in an audit report.
type Headings struct {
	H1 []string `json:"h1"`
}

// PageImage is an image included in a parsed page.
type PageImage struct {
	URL string  `json:"url"`
	Alt *string `json:"alt"`
}

// OpenGraph contains Open Graph metadata found on a page.
type OpenGraph struct {
	Title       *string `json:"title"`
	Description *string `json:"description"`
	Image       *string `json:"image"`
	URL         *string `json:"url"`
	Type        *string `json:"type"`
	SiteName    *string `json:"site_name"`
	Locale      *string `json:"locale"`
}

// Page is a crawled page included in the public report.
type Page struct {
	URL         string      `json:"url"`
	Depth       int         `json:"depth"`
	StatusCode  *int        `json:"status_code"`
	ContentType *string     `json:"content_type"`
	Title       *string     `json:"title"`
	Description *string     `json:"description"`
	Headings    Headings    `json:"headings"`
	Images      []PageImage `json:"images"`
	OpenGraph   OpenGraph   `json:"open_graph"`
}

// ResultKind identifies a link or image check result.
type ResultKind string

const (
	ResultResponse ResultKind = "response"
	ResultBlocked  ResultKind = "blocked"
	ResultFailed   ResultKind = "failed"
	ResultSkipped  ResultKind = "skipped"
	ResultInvalid  ResultKind = "invalid"
)

// FailureReason is a stable machine-readable request failure.
type FailureReason string

const (
	FailureRequestTimedOut     FailureReason = "request-timed-out"
	FailureConnectionFailed    FailureReason = "connection-failed"
	FailureRequestFailed       FailureReason = "request-failed"
	FailureAntiBotChallenge    FailureReason = "anti-bot-challenge"
	FailureUnsupportedProtocol FailureReason = "unsupported-protocol"
	FailureInvalidURL          FailureReason = "invalid-url"
)

// LinkResult describes the outcome of checking a link.
type LinkResult struct {
	Kind       ResultKind    `json:"kind"`
	StatusCode *int          `json:"status_code,omitempty"`
	FinalURL   *string       `json:"final_url,omitempty"`
	Reason     FailureReason `json:"reason,omitempty"`
}

// LinkOccurrence describes where a link was found.
type LinkOccurrence struct {
	PageURL     string `json:"page_url"`
	OriginalURL string `json:"original_url"`
	Element     string `json:"element"`
	Text        string `json:"text"`
}

// Link is one unique checked link and all of its occurrences.
type Link struct {
	URL     string           `json:"url"`
	Result  LinkResult       `json:"result"`
	FoundOn []LinkOccurrence `json:"found_on"`
}

// ImageResult describes the outcome of checking an image.
type ImageResult struct {
	Kind        ResultKind    `json:"kind"`
	StatusCode  *int          `json:"status_code,omitempty"`
	FinalURL    *string       `json:"final_url,omitempty"`
	ContentType *string       `json:"content_type,omitempty"`
	Reason      FailureReason `json:"reason,omitempty"`
}

// ImageOccurrence describes where an image reference was found.
type ImageOccurrence struct {
	PageURL     string  `json:"page_url"`
	OriginalURL string  `json:"original_url"`
	Element     string  `json:"element"`
	Attribute   string  `json:"attribute"`
	Descriptor  *string `json:"descriptor"`
	Alt         *string `json:"alt"`
}

// Image is one unique checked image and all of its occurrences.
type Image struct {
	URL     string            `json:"url"`
	Result  ImageResult       `json:"result"`
	FoundOn []ImageOccurrence `json:"found_on"`
}

// LinkSummary contains aggregate link counts.
type LinkSummary struct {
	Total      int `json:"total"`
	Checked    int `json:"checked"`
	Broken     int `json:"broken"`
	Blocked    int `json:"blocked"`
	Redirected int `json:"redirected"`
}

// ImageSummary contains aggregate image counts.
type ImageSummary struct {
	Total      int `json:"total"`
	Checked    int `json:"checked"`
	Broken     int `json:"broken"`
	Blocked    int `json:"blocked"`
	Redirected int `json:"redirected"`
	Invalid    int `json:"invalid"`
}

// IssueSummary contains aggregate issue counts.
type IssueSummary struct {
	Total   int `json:"total"`
	Error   int `json:"error"`
	Warning int `json:"warning"`
	Info    int `json:"info"`
}

// Summary contains aggregate audit counts.
type Summary struct {
	Pages  int          `json:"pages"`
	Links  LinkSummary  `json:"links"`
	Images ImageSummary `json:"images"`
	Issues IssueSummary `json:"issues"`
}

// Report is the complete result of an audit.
type Report struct {
	URL       string    `json:"url"`
	AuditedAt time.Time `json:"audited_at"`
	Summary   Summary   `json:"summary"`
	Issues    []Issue   `json:"issues"`
	Pages     []Page    `json:"pages"`
	Links     []Link    `json:"links"`
	Images    []Image   `json:"images"`
}
