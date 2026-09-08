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
	got, err := applyRulePolicy(context.Background(), issues, nil, false)
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

	got, err := applyRulePolicy(context.Background(), issues, rules, false)
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

	got, err := applyRulePolicy(context.Background(), issues, rules, true)
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
	issues, err := applyRulePolicy(ctx, []Issue{{Code: IssueMissingTitle}}, nil, false)
	if !errors.Is(err, context.Canceled) || issues != nil {
		t.Fatalf("applyRulePolicy() = %#v, %v", issues, err)
	}
}
