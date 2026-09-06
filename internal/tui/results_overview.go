package tui

import (
	"fmt"
	"strconv"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/nelsonlaidev/scoutly/audit"
)

type overviewMetricDefinition struct {
	label string
	tab   resultTab
}

var overviewMetrics = []overviewMetricDefinition{
	{label: "Pages", tab: tabPages},
	{label: "Issues", tab: tabIssues},
	{label: "Links", tab: tabLinks},
	{label: "Images", tab: tabImages},
}

func renderOverview(report *audit.Report, width int, theme theme) string {
	if report == nil {
		return theme.errorText.Render("Report unavailable.")
	}
	summary := report.Summary
	metricGap := " "
	metricWidths := overviewMetricWidths(width)
	metricViews := make([]string, 0, len(overviewMetrics)*2-1)
	for index, metric := range overviewMetrics {
		if index > 0 {
			metricViews = append(metricViews, metricGap)
		}
		metricViews = append(
			metricViews,
			overviewMetric(
				metric.label,
				overviewMetricValue(summary, metric.tab),
				metricWidths[index],
				theme,
			),
		)
	}
	metrics := lipgloss.JoinHorizontal(lipgloss.Top, metricViews...)
	detailLines := []string{
		theme.muted.Render("Issue severity"),
		fmt.Sprintf("  Error: %d", summary.Issues.Error),
		fmt.Sprintf("  Warning: %d", summary.Issues.Warning),
		fmt.Sprintf("  Info: %d", summary.Issues.Info),
		"",
		theme.muted.Render("Link summary"),
		fmt.Sprintf("  Checked: %d", summary.Links.Checked),
		fmt.Sprintf("  Broken: %d", summary.Links.Broken),
		fmt.Sprintf("  Blocked: %d", summary.Links.Blocked),
		fmt.Sprintf("  Redirected: %d", summary.Links.Redirected),
		"",
		theme.muted.Render("Image summary"),
		fmt.Sprintf("  Checked: %d", summary.Images.Checked),
		fmt.Sprintf("  Broken: %d", summary.Images.Broken),
		fmt.Sprintf("  Blocked: %d", summary.Images.Blocked),
		fmt.Sprintf("  Invalid: %d", summary.Images.Invalid),
		fmt.Sprintf("  Redirected: %d", summary.Images.Redirected),
		"",
		theme.muted.Render("Audited " + report.URL),
		theme.muted.Render(
			"Completed " + report.AuditedAt.Format("2006-01-02 15:04:05 MST"),
		),
	}
	details := theme.metricBox.Width(width).Render(strings.Join(detailLines, "\n"))
	return metrics + "\n" + details
}

func overviewMetricWidths(width int) []int {
	gapWidth := lipgloss.Width(" ")
	metricsWidth := max(
		len(overviewMetrics),
		width-(len(overviewMetrics)-1)*gapWidth,
	)
	metricWidth := metricsWidth / len(overviewMetrics)
	extraColumns := metricsWidth % len(overviewMetrics)
	widths := make([]int, len(overviewMetrics))
	for index := range widths {
		widths[index] = metricWidth
		if index < extraColumns {
			widths[index]++
		}
	}
	return widths
}

func overviewMetricValue(summary audit.Summary, tab resultTab) int {
	switch tab {
	case tabPages:
		return summary.Pages
	case tabIssues:
		return summary.Issues.Total
	case tabLinks:
		return summary.Links.Total
	case tabImages:
		return summary.Images.Total
	default:
		return 0
	}
}

func overviewMetric(label string, value, width int, theme theme) string {
	return theme.metricBox.Width(width).Render(
		theme.metricLabel.Render(label+": ") +
			theme.metricValue.Render(strconv.Itoa(value)),
	)
}
