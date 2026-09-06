package audit

import (
	"fmt"
	"strings"
	"unicode/utf8"

	"github.com/nelsonlaidev/scoutly/internal/page"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

type lengthRule struct {
	minLength      int
	maxLength      int
	missingCode    IssueCode
	missingMessage string
	tooShortCode   IssueCode
	tooShortLabel  string
	tooLongCode    IssueCode
	tooLongLabel   string
}

var titleRule = lengthRule{
	minLength:      50,
	maxLength:      60,
	missingCode:    IssueMissingTitle,
	missingMessage: "Page is missing a title tag",
	tooShortCode:   IssueTitleTooShort,
	tooShortLabel:  "Title is too short",
	tooLongCode:    IssueTitleTooLong,
	tooLongLabel:   "Title is too long",
}

var metaDescriptionRule = lengthRule{
	minLength:      150,
	maxLength:      160,
	missingCode:    IssueMissingMetaDescription,
	missingMessage: "Page is missing a meta description",
	tooShortCode:   IssueMetaDescriptionTooShort,
	tooShortLabel:  "Meta description is too short",
	tooLongCode:    IssueMetaDescriptionTooLong,
	tooLongLabel:   "Meta description is too long",
}

func analyzePage(input page.Page, target IssueTarget) []Issue {
	if !page.IsHTMLContentType(input.ContentType) {
		return []Issue{}
	}

	issues := make([]Issue, 0)
	issues = append(issues, validateLength(input.Title, titleRule)...)
	issues = append(issues, validateLength(input.Description, metaDescriptionRule)...)
	issues = append(issues, validateH1Headings(input.Headings.H1)...)
	issues = append(issues, validateImages(input.ImageAltTexts)...)
	issues = append(issues, validateThinContent(input)...)
	issues = append(issues, validateOpenGraph(input.OpenGraph)...)
	for index := range issues {
		issues[index].Target = target
	}
	return issues
}

func validateLength(value *string, rule lengthRule) []Issue {
	if value == nil || strings.TrimSpace(*value) == "" {
		return []Issue{{
			Severity: SeverityError,
			Code:     rule.missingCode,
			Message:  rule.missingMessage,
		}}
	}

	length := utf8.RuneCountInString(strings.TrimSpace(*value))
	if length < rule.minLength {
		return []Issue{{
			Severity: SeverityWarning,
			Code:     rule.tooShortCode,
			Message: fmt.Sprintf(
				"%s (%d chars, recommended: %d-%d)",
				rule.tooShortLabel,
				length,
				rule.minLength,
				rule.maxLength,
			),
		}}
	}
	if length > rule.maxLength {
		return []Issue{{
			Severity: SeverityWarning,
			Code:     rule.tooLongCode,
			Message: fmt.Sprintf(
				"%s (%d chars, recommended: %d-%d)",
				rule.tooLongLabel,
				length,
				rule.minLength,
				rule.maxLength,
			),
		}}
	}
	return []Issue{}
}

func validateH1Headings(headings []string) []Issue {
	switch {
	case len(headings) == 0:
		return []Issue{{
			Severity: SeverityWarning,
			Code:     IssueMissingH1,
			Message:  "Page is missing an H1 tag",
		}}
	case len(headings) > 1:
		return []Issue{{
			Severity: SeverityWarning,
			Code:     IssueMultipleH1,
			Message:  fmt.Sprintf("Page has multiple H1 tags (%d)", len(headings)),
		}}
	default:
		return []Issue{}
	}
}

func validateImages(altTexts []*string) []Issue {
	missingAltCount := 0
	for _, alt := range altTexts {
		if alt == nil {
			missingAltCount++
		}
	}
	if missingAltCount == 0 {
		return []Issue{}
	}

	return []Issue{{
		Severity: SeverityWarning,
		Code:     IssueMissingImageAlt,
		Message:  fmt.Sprintf("%d image(s) missing alt text", missingAltCount),
	}}
}

func validateThinContent(input page.Page) []Issue {
	contentIndicators := len(input.Headings.H1) + len(input.Links) + len(input.Images)
	if contentIndicators >= 5 {
		return []Issue{}
	}

	return []Issue{{
		Severity: SeverityWarning,
		Code:     IssueThinContent,
		Message:  "Page may have thin content (few elements found)",
	}}
}

func validateOpenGraph(openGraph page.OpenGraph) []Issue {
	fields := []struct {
		value   string
		present bool
		code    IssueCode
		tag     string
	}{
		{value: dereferenceString(openGraph.Title), present: openGraph.Title != nil, code: IssueMissingOGTitle, tag: "og:title"},
		{value: dereferenceString(openGraph.Description), present: openGraph.Description != nil, code: IssueMissingOGDescription, tag: "og:description"},
		{value: urlutil.String(openGraph.Image), present: openGraph.Image != nil, code: IssueMissingOGImage, tag: "og:image"},
		{value: urlutil.String(openGraph.URL), present: openGraph.URL != nil, code: IssueMissingOGURL, tag: "og:url"},
		{value: dereferenceString(openGraph.Type), present: openGraph.Type != nil, code: IssueMissingOGType, tag: "og:type"},
	}

	issues := make([]Issue, 0, len(fields))
	for _, field := range fields {
		if field.present && strings.TrimSpace(field.value) != "" {
			continue
		}
		issues = append(issues, Issue{
			Severity: SeverityInfo,
			Code:     field.code,
			Message:  "Page is missing " + field.tag + " tag",
		})
	}
	return issues
}

func dereferenceString(value *string) string {
	if value == nil {
		return ""
	}
	return *value
}
