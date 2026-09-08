package audit

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestApplyRulePolicyUsesQuietDefaults(t *testing.T) {
	t.Parallel()

	issues := []Issue{
		{Code: IssueMissingTitle, Severity: SeverityInfo},
		{Code: IssueTitleTooShort, Severity: SeverityWarning},
		{Code: IssueTitleTooLong, Severity: SeverityWarning},
		{Code: IssueMetaDescriptionTooShort, Severity: SeverityWarning},
		{Code: IssueMetaDescriptionTooLong, Severity: SeverityWarning},
		{Code: IssueThinContent, Severity: SeverityWarning},
		{Code: IssueMissingOGTitle, Severity: SeverityInfo},
		{Code: IssueMissingOGDescription, Severity: SeverityInfo},
		{Code: IssueMissingOGImage, Severity: SeverityInfo},
		{Code: IssueMissingOGURL, Severity: SeverityInfo},
		{Code: IssueMissingOGType, Severity: SeverityInfo},
	}

	want := []Issue{{Code: IssueMissingTitle, Severity: SeverityError}}
	got, err := applyRulePolicy(context.Background(), issues, nil, false, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("applyRulePolicy() = %#v, want %#v", got, want)
	}
}

func TestApplyRulePolicyOverridesLevels(t *testing.T) {
	t.Parallel()

	issues := []Issue{
		{Code: IssueTitleTooShort, Severity: SeverityWarning},
		{Code: IssueMissingTitle, Severity: SeverityError},
		{Code: IssueBrokenLink, Severity: SeverityError},
	}
	rules := Rules{
		"title_too_short": RuleLevelWarning,
		"missing_title":   RuleLevelInfo,
		"broken_link":     RuleLevelOff,
	}
	want := []Issue{
		{Code: IssueTitleTooShort, Severity: SeverityWarning},
		{Code: IssueMissingTitle, Severity: SeverityInfo},
	}

	got, err := applyRulePolicy(context.Background(), issues, rules, false, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("applyRulePolicy() = %#v, want %#v", got, want)
	}
}

func TestApplyRulePolicyPreservesIgnoreRedirectsCompatibility(t *testing.T) {
	t.Parallel()

	issues := []Issue{
		{Code: IssueRedirect, Severity: SeverityInfo},
		{Code: IssueImageRedirect, Severity: SeverityInfo},
	}
	rules := Rules{
		"redirect":       RuleLevelError,
		"image_redirect": RuleLevelWarning,
	}

	got, err := applyRulePolicy(context.Background(), issues, rules, true, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 0 {
		t.Fatalf("applyRulePolicy() = %#v, want no redirect issues", got)
	}
}

func TestApplyRulePolicyStopsOnCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	issues, err := applyRulePolicy(ctx, []Issue{{Code: IssueMissingTitle}}, nil, false, nil)
	if !errors.Is(err, context.Canceled) || issues != nil {
		t.Fatalf("applyRulePolicy() = %#v, %v", issues, err)
	}
}

func TestApplyRulePolicyIgnoresTargetPrefixes(t *testing.T) {
	t.Parallel()
	ignores := []RuleIgnore{
		{URLPrefix: "https://EXAMPLE.com:443/legacy/", Rules: []string{"missing_title", "broken_link"}},
		{URLPrefix: "https://other.example/", Rules: []string{"broken_image"}},
	}
	for _, test := range []struct {
		url     string
		code    IssueCode
		ignored bool
	}{
		{"https://example.com/legacy", IssueMissingTitle, true},
		{"https://example.com/legacy/page?a=1#part", IssueBrokenLink, true},
		{"https://example.com/legacy%2Fpage", IssueBrokenLink, true},
		{"https://example.com/legacy-old", IssueBrokenLink, false},
		{"https://example.com/Legacy", IssueBrokenLink, false},
		{"http://example.com/legacy", IssueBrokenLink, false},
		{"https://example.com:8443/legacy", IssueBrokenLink, false},
		{"https://other.example/legacy", IssueBrokenLink, false},
		{"https://other.example", IssueBrokenImage, true},
		{"https://example.com/legacy", IssueMissingH1, false},
		{"%", IssueInvalidImageURL, false},
	} {
		t.Run(test.url+string(test.code), func(t *testing.T) {
			issues := []Issue{{Code: test.code, Target: IssueTarget{URL: test.url}}}
			got, err := applyRulePolicy(context.Background(), issues, nil, false, ignores)
			if err != nil || (len(got) == 0) != test.ignored {
				t.Fatalf("issues = %#v, error = %v, want ignored=%t", got, err, test.ignored)
			}
		})
	}
	issues := []Issue{
		{Code: IssueMissingTitle, Target: IssueTarget{URL: "https://elsewhere.example/"}},
		{Code: IssueRedirect, Target: IssueTarget{URL: "https://example.com/legacy"}},
	}
	got, err := applyRulePolicy(context.Background(), issues, Rules{"missing_title": RuleLevelOff, "redirect": RuleLevelError}, true, ignores)
	if err != nil || len(got) != 0 {
		t.Fatalf("ignores changed global policy: %#v, %v", got, err)
	}
}
