package audit

import (
	"reflect"
	"slices"
	"strings"
	"testing"

	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/testutil"
)

func TestAnalyzeLengthBoundaries(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		title       string
		description string
		want        []Issue
	}{
		{
			name:        "minimum boundaries",
			title:       strings.Repeat("T", 50),
			description: strings.Repeat("D", 150),
			want:        []Issue{},
		},
		{
			name:        "maximum boundaries",
			title:       strings.Repeat("T", 60),
			description: strings.Repeat("D", 160),
			want:        []Issue{},
		},
		{
			name:        "below minimum",
			title:       strings.Repeat("T", 49),
			description: strings.Repeat("D", 149),
			want: []Issue{
				{Severity: SeverityWarning, Code: IssueTitleTooShort, Message: "Title is too short (49 chars, recommended: 50-60)"},
				{Severity: SeverityWarning, Code: IssueMetaDescriptionTooShort, Message: "Meta description is too short (149 chars, recommended: 150-160)"},
			},
		},
		{
			name:        "above maximum",
			title:       strings.Repeat("T", 61),
			description: strings.Repeat("D", 161),
			want: []Issue{
				{Severity: SeverityWarning, Code: IssueTitleTooLong, Message: "Title is too long (61 chars, recommended: 50-60)"},
				{Severity: SeverityWarning, Code: IssueMetaDescriptionTooLong, Message: "Meta description is too long (161 chars, recommended: 150-160)"},
			},
		},
		{
			name:        "Unicode code points",
			title:       strings.Repeat("🚀", 50),
			description: strings.Repeat("界", 150),
			want:        []Issue{},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			input := completePage(t)
			input.Title = new(test.title)
			input.Description = new(test.description)
			if got := analyzePage(input, IssueTarget{}); !reflect.DeepEqual(got, test.want) {
				t.Errorf("analyzePage() = %#v, want %#v", got, test.want)
			}
		})
	}
}

func TestAnalyzeMissingMetadataOrder(t *testing.T) {
	t.Parallel()

	input := completePage(t)
	input.Title = new("   ")
	input.Description = nil
	input.Headings.H1 = []string{}
	input.Links = []page.Link{}
	input.Images = []page.Image{
		{Src: testutil.ParseURL(t, "https://example.com/one.png"), Alt: nil},
		{Src: testutil.ParseURL(t, "https://example.com/two.png"), Alt: nil},
		{Src: testutil.ParseURL(t, "https://example.com/decorative.png"), Alt: new("")},
	}
	input.ImageAltTexts = []*string{nil, nil, new("")}
	input.OpenGraph = page.OpenGraph{Description: new(" ")}

	want := []Issue{
		{Severity: SeverityError, Code: IssueMissingTitle, Message: "Page is missing a title tag"},
		{Severity: SeverityError, Code: IssueMissingMetaDescription, Message: "Page is missing a meta description"},
		{Severity: SeverityWarning, Code: IssueMissingH1, Message: "Page is missing an H1 tag"},
		{Severity: SeverityWarning, Code: IssueMissingImageAlt, Message: "2 image(s) missing alt text"},
		{Severity: SeverityWarning, Code: IssueThinContent, Message: "Page may have thin content (few elements found)"},
		{Severity: SeverityInfo, Code: IssueMissingOGTitle, Message: "Page is missing og:title tag"},
		{Severity: SeverityInfo, Code: IssueMissingOGDescription, Message: "Page is missing og:description tag"},
		{Severity: SeverityInfo, Code: IssueMissingOGImage, Message: "Page is missing og:image tag"},
		{Severity: SeverityInfo, Code: IssueMissingOGURL, Message: "Page is missing og:url tag"},
		{Severity: SeverityInfo, Code: IssueMissingOGType, Message: "Page is missing og:type tag"},
	}

	if got := analyzePage(input, IssueTarget{}); !reflect.DeepEqual(got, want) {
		t.Errorf("analyzePage() = %#v, want %#v", got, want)
	}
}

func TestAnalyzePageElements(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		mutate func(*page.Page)
		want   Issue
	}{
		{
			name: "multiple H1",
			mutate: func(input *page.Page) {
				input.Headings.H1 = []string{"First", "Second"}
			},
			want: Issue{Severity: SeverityWarning, Code: IssueMultipleH1, Message: "Page has multiple H1 tags (2)"},
		},
		{
			name: "srcset-only image without alt",
			mutate: func(input *page.Page) {
				input.ImageAltTexts = append(input.ImageAltTexts, nil)
			},
			want: Issue{Severity: SeverityWarning, Code: IssueMissingImageAlt, Message: "1 image(s) missing alt text"},
		},
		{
			name: "thin content",
			mutate: func(input *page.Page) {
				input.Headings.H1 = []string{"Heading"}
				input.Links = []page.Link{}
				input.Images = []page.Image{}
			},
			want: Issue{Severity: SeverityWarning, Code: IssueThinContent, Message: "Page may have thin content (few elements found)"},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			input := completePage(t)
			test.mutate(&input)
			issues := analyzePage(input, IssueTarget{})
			if !slices.Contains(issues, test.want) {
				t.Errorf("analyzePage() = %#v, want it to contain %#v", issues, test.want)
			}
		})
	}
}

func TestAnalyzeContentTypes(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		contentType string
		wantIssues  bool
	}{
		{name: "missing content type", contentType: "", wantIssues: true},
		{name: "HTML", contentType: "text/html; charset=utf-8", wantIssues: true},
		{name: "HTML case insensitive", contentType: "TEXT/HTML", wantIssues: true},
		{name: "XHTML", contentType: "application/xhtml+xml", wantIssues: true},
		{name: "PDF", contentType: "application/pdf", wantIssues: false},
		{name: "image", contentType: "image/png", wantIssues: false},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			input := completePage(t)
			input.ContentType = test.contentType
			input.Title = nil
			issues := analyzePage(input, IssueTarget{})
			if got := len(issues) > 0; got != test.wantIssues {
				t.Errorf("analyzePage() returned issues = %t, want %t; issues: %#v", got, test.wantIssues, issues)
			}
			if test.wantIssues && !slices.Contains(issues, Issue{
				Severity: SeverityError,
				Code:     IssueMissingTitle,
				Message:  "Page is missing a title tag",
			}) {
				t.Errorf("analyzePage() = %#v, want missing title issue", issues)
			}
		})
	}
}

func TestAnalyzeDoesNotMutatePage(t *testing.T) {
	t.Parallel()

	input := completePage(t)
	original := clonePage(input)

	_ = analyzePage(input, IssueTarget{})

	if !reflect.DeepEqual(input, original) {
		t.Errorf("analyzePage() mutated page:\ngot  %#v\nwant %#v", input, original)
	}
}

func completePage(t *testing.T) page.Page {
	t.Helper()
	return page.Page{
		ContentType: "text/html; charset=utf-8",
		Title:       new(strings.Repeat("T", 50)),
		Description: new(strings.Repeat("D", 150)),
		Headings:    page.Headings{H1: []string{"Main heading"}},
		Links: []page.Link{
			{Element: page.LinkElementAnchor, URL: testutil.ParseURL(t, "https://example.com/about"), Text: "About"},
			{Element: page.LinkElementAnchor, URL: testutil.ParseURL(t, "https://example.com/contact"), Text: "Contact"},
		},
		Images: []page.Image{
			{Src: testutil.ParseURL(t, "https://example.com/hero.png"), Alt: new("Hero")},
			{Src: testutil.ParseURL(t, "https://example.com/decorative.png"), Alt: new("")},
		},
		ImageAltTexts:   []*string{new("Hero"), new("")},
		ImageReferences: []page.ImageReference{},
		OpenGraph: page.OpenGraph{
			Title:       new("Example title"),
			Description: new("Example description"),
			Image:       testutil.ParseURL(t, "https://example.com/social.png"),
			URL:         testutil.ParseURL(t, "https://example.com/"),
			Type:        new("website"),
		},
	}
}

func clonePage(input page.Page) page.Page {
	result := input
	result.Headings.H1 = append(make([]string, 0, len(input.Headings.H1)), input.Headings.H1...)
	result.Links = append(make([]page.Link, 0, len(input.Links)), input.Links...)
	result.Images = append(make([]page.Image, 0, len(input.Images)), input.Images...)
	result.ImageAltTexts = append(make([]*string, 0, len(input.ImageAltTexts)), input.ImageAltTexts...)
	result.ImageReferences = append(
		make([]page.ImageReference, 0, len(input.ImageReferences)),
		input.ImageReferences...,
	)
	return result
}
