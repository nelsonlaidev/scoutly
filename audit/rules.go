package audit

import (
	"context"
	"strings"
)

// RuleLevel controls whether an issue is reported and which severity it uses.
type RuleLevel string

const (
	RuleLevelOff     RuleLevel = "off"
	RuleLevelInfo    RuleLevel = "info"
	RuleLevelWarning RuleLevel = "warning"
	RuleLevelError   RuleLevel = "error"
)

// Rules overrides the built-in level of individual audit rules by snake_case name.
// Omitted rules keep their built-in level.
type Rules map[string]RuleLevel

var defaultRuleLevels = map[string]RuleLevel{
	ruleName(IssueMissingTitle):            RuleLevelError,
	ruleName(IssueTitleTooShort):           RuleLevelOff,
	ruleName(IssueTitleTooLong):            RuleLevelOff,
	ruleName(IssueMissingMetaDescription):  RuleLevelError,
	ruleName(IssueMetaDescriptionTooShort): RuleLevelOff,
	ruleName(IssueMetaDescriptionTooLong):  RuleLevelOff,
	ruleName(IssueMissingImageAlt):         RuleLevelWarning,
	ruleName(IssueMissingH1):               RuleLevelWarning,
	ruleName(IssueMultipleH1):              RuleLevelWarning,
	ruleName(IssueThinContent):             RuleLevelOff,
	ruleName(IssueMissingOGTitle):          RuleLevelOff,
	ruleName(IssueMissingOGDescription):    RuleLevelOff,
	ruleName(IssueMissingOGImage):          RuleLevelOff,
	ruleName(IssueMissingOGURL):            RuleLevelOff,
	ruleName(IssueMissingOGType):           RuleLevelOff,
	ruleName(IssuePageCrawlFailed):         RuleLevelError,
	ruleName(IssuePageHTTPError):           RuleLevelError,
	ruleName(IssueBrokenLink):              RuleLevelError,
	ruleName(IssueLinkCheckBlocked):        RuleLevelWarning,
	ruleName(IssueRedirect):                RuleLevelInfo,
	ruleName(IssueInvalidImageURL):         RuleLevelError,
	ruleName(IssueBrokenImage):             RuleLevelError,
	ruleName(IssueImageCheckBlocked):       RuleLevelWarning,
	ruleName(IssueInvalidImageContentType): RuleLevelError,
	ruleName(IssueImageRedirect):           RuleLevelInfo,
}

func applyRulePolicy(
	ctx context.Context,
	issues []Issue,
	overrides Rules,
	ignoreRedirects bool,
) ([]Issue, error) {
	result := make([]Issue, 0, len(issues))
	for _, issue := range issues {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		name := ruleName(issue.Code)
		level, known := defaultRuleLevels[name]
		if !known {
			result = append(result, issue)
			continue
		}

		if override, exists := overrides[name]; exists {
			level = override
		}
		if ignoreRedirects &&
			(issue.Code == IssueRedirect || issue.Code == IssueImageRedirect) {
			level = RuleLevelOff
		}
		if level == RuleLevelOff {
			continue
		}

		issue.Severity = Severity(level)
		result = append(result, issue)
	}
	return result, nil
}

func ruleName(code IssueCode) string {
	return strings.ReplaceAll(string(code), "-", "_")
}

func isValidRuleLevel(level RuleLevel) bool {
	switch level {
	case RuleLevelOff, RuleLevelInfo, RuleLevelWarning, RuleLevelError:
		return true
	default:
		return false
	}
}
