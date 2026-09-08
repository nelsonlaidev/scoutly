package tui

import (
	"errors"
	"maps"
	"math"
	"strconv"
	"strings"
	"time"

	"charm.land/bubbles/v2/key"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"

	"github.com/nelsonlaidev/scoutly/audit"
	"github.com/nelsonlaidev/scoutly/internal/urlutil"
)

const (
	maxTimeoutMilliseconds = math.MaxInt64 / int64(time.Millisecond)
	setupGridColumns       = 2
	runButtonLabel         = "Run audit"
)

type fieldID string

const (
	fieldURL                 fieldID = "url"
	fieldMaxDepth            fieldID = "max_depth"
	fieldMaxPages            fieldID = "max_pages"
	fieldMaxSitemapDocuments fieldID = "max_sitemap_documents"
	fieldTimeout             fieldID = "timeout"
	fieldMaxRedirects        fieldID = "max_redirects"
	fieldConcurrency         fieldID = "concurrency"
	fieldUserAgent           fieldID = "user_agent"
	fieldRateLimit           fieldID = "rate_limit"
	fieldKeepFragments       fieldID = "keep_fragments"
	fieldIgnoreRedirects     fieldID = "ignore_redirects"
	fieldRespectRobots       fieldID = "respect_robots"
	fieldSitemaps            fieldID = "sitemaps"
	fieldImages              fieldID = "images"
	fieldRun                 fieldID = "run"
)

type setupValues struct {
	url                 string
	maxDepth            string
	maxPages            string
	maxSitemapDocuments string
	timeout             string
	maxRedirects        string
	concurrency         string
	userAgent           string
	rateLimit           string
	keepFragments       bool
	ignoreRedirects     bool
	respectRobots       bool
	sitemaps            bool
	images              bool
	rules               *audit.Rules
}

type setupLayout struct {
	grid     huh.Layout
	runGroup *huh.Group
	runTheme *setupRunTheme
}

func (layout *setupLayout) View(form *huh.Form) string {
	return layout.grid.View(form)
}

func (layout *setupLayout) GroupWidth(
	form *huh.Form,
	group *huh.Group,
	width int,
) int {
	if group == layout.runGroup {
		layout.runTheme.width = width
		return width
	}
	return layout.grid.GroupWidth(form, group, width)
}

func newSetupForm(values *setupValues, formTheme *setupTheme) (*huh.Form, []fieldID) {
	input := func(id fieldID, title, placeholder string, value *string) *huh.Input {
		field := huh.NewInput().
			Key(string(id)).
			Title(title).
			Value(value).
			Validate(validateSetupInput(values, id))
		if placeholder != "" {
			field.Placeholder(placeholder)
		}
		return field
	}
	confirm := func(id fieldID, title string, value *bool) *huh.Confirm {
		// Huh's non-inline layout inserts a blank row. Pair an inline layout
		// with a newline-only description to keep options below the title.
		return huh.NewConfirm().
			Key(string(id)).
			Title(title).
			Description("\n").
			Inline(true).
			WithButtonAlignment(lipgloss.Left).
			Value(value)
	}

	keyMap := huh.NewDefaultKeyMap()
	keyMap.Confirm.Toggle = key.NewBinding(
		key.WithKeys(" ", "space", "h", "l", "left", "right"),
		key.WithHelp("space", "toggle"),
	)

	runTheme := &setupRunTheme{setup: formTheme, width: defaultWidth}
	runField := huh.NewConfirm().
		Key(string(fieldRun)).
		Affirmative(runButtonLabel).
		Negative("")
	runField.WithTheme(runTheme)

	fields := []huh.Field{
		input(fieldURL, "Website URL", "https://example.com", &values.url),
		input(fieldMaxDepth, "Maximum depth", "", &values.maxDepth),
		input(fieldMaxPages, "Maximum pages", "", &values.maxPages),
		input(
			fieldMaxSitemapDocuments,
			"Maximum sitemap documents",
			"",
			&values.maxSitemapDocuments,
		),
		input(fieldTimeout, "Timeout (ms)", "", &values.timeout),
		input(
			fieldMaxRedirects,
			"Maximum redirects",
			"",
			&values.maxRedirects,
		),
		input(fieldConcurrency, "Concurrency", "", &values.concurrency),
		input(fieldUserAgent, "User agent", "", &values.userAgent),
		input(
			fieldRateLimit,
			"Rate limit (requests/sec)",
			"disabled",
			&values.rateLimit,
		),
		confirm(
			fieldKeepFragments,
			"Keep URL fragments",
			&values.keepFragments,
		),
		confirm(
			fieldIgnoreRedirects,
			"Ignore redirect issues",
			&values.ignoreRedirects,
		),
		confirm(
			fieldRespectRobots,
			"Respect robots.txt",
			&values.respectRobots,
		),
		confirm(fieldSitemaps, "Discover XML sitemaps", &values.sitemaps),
		confirm(fieldImages, "Check discovered images", &values.images),
		runField,
	}

	groups := make([]*huh.Group, 0, len(fields))
	fieldOrder := make([]fieldID, 0, len(fields))
	var runGroup *huh.Group
	for _, field := range fields {
		group := huh.NewGroup(field)
		groups = append(groups, group)
		if field.GetKey() == string(fieldRun) {
			runGroup = group
		}
		fieldOrder = append(fieldOrder, fieldID(field.GetKey()))
	}
	rows := (len(groups) + setupGridColumns - 1) / setupGridColumns
	layout := &setupLayout{
		grid:     huh.LayoutGrid(rows, setupGridColumns),
		runGroup: runGroup,
		runTheme: runTheme,
	}
	form := huh.NewForm(groups...).
		WithLayout(layout).
		WithKeyMap(keyMap).
		WithShowHelp(false).
		WithShowErrors(false).
		WithTheme(formTheme)
	return form, fieldOrder
}

func validateSetupInput(values *setupValues, id fieldID) func(string) error {
	return func(value string) error {
		candidate := *values
		switch id {
		case fieldURL:
			candidate.url = value
		case fieldMaxDepth:
			candidate.maxDepth = value
		case fieldMaxPages:
			candidate.maxPages = value
		case fieldMaxSitemapDocuments:
			candidate.maxSitemapDocuments = value
		case fieldTimeout:
			candidate.timeout = value
		case fieldMaxRedirects:
			candidate.maxRedirects = value
		case fieldConcurrency:
			candidate.concurrency = value
		case fieldUserAgent:
			candidate.userAgent = value
		case fieldRateLimit:
			candidate.rateLimit = value
		}

		_, _, errorsByField := parseSetupValues(candidate)
		message := errorsByField[id]
		if message == "" {
			return nil
		}
		return errors.New(message)
	}
}

func parseSetupValues(values setupValues) (string, audit.Options, map[fieldID]string) {
	errorsByField := make(map[fieldID]string)
	options := audit.Options{
		KeepFragments:   values.keepFragments,
		IgnoreRedirects: values.ignoreRedirects,
		RespectRobots:   values.respectRobots,
		Sitemaps:        values.sitemaps,
		Images:          values.images,
		UserAgent:       values.userAgent,
	}
	if values.rules != nil {
		options.Rules = maps.Clone(*values.rules)
	}

	target := strings.TrimSpace(values.url)
	if _, err := urlutil.ParseTarget(target); err != nil {
		errorsByField[fieldURL] = "Enter a valid HTTP or HTTPS URL"
	}

	parseInt := func(id fieldID, value string, destination *int) {
		parsed, err := strconv.Atoi(strings.TrimSpace(value))
		if err != nil {
			errorsByField[id] = "Enter a whole number"
			return
		}
		*destination = parsed
	}
	parseInt(fieldMaxDepth, values.maxDepth, &options.MaxDepth)
	parseInt(fieldMaxPages, values.maxPages, &options.MaxPages)
	parseInt(
		fieldMaxSitemapDocuments,
		values.maxSitemapDocuments,
		&options.MaxSitemapDocuments,
	)
	parseInt(fieldMaxRedirects, values.maxRedirects, &options.MaxRedirects)
	parseInt(fieldConcurrency, values.concurrency, &options.Concurrency)

	timeoutMilliseconds, err := strconv.ParseInt(strings.TrimSpace(values.timeout), 10, 64)
	switch {
	case err != nil:
		errorsByField[fieldTimeout] = "Enter a whole number"
	case timeoutMilliseconds <= 0:
		errorsByField[fieldTimeout] = "Must be greater than 0"
	case timeoutMilliseconds > maxTimeoutMilliseconds:
		errorsByField[fieldTimeout] = "Value is too large"
	default:
		options.Timeout = time.Duration(timeoutMilliseconds) * time.Millisecond
	}

	rateLimit := strings.TrimSpace(values.rateLimit)
	if rateLimit != "" {
		options.RateLimit, err = strconv.ParseFloat(rateLimit, 64)
		if err != nil {
			errorsByField[fieldRateLimit] = "Enter a number or leave blank"
		}
	}

	if validationErr := options.Validate(); validationErr != nil {
		if fieldErr, ok := errors.AsType[*audit.ValidationError](validationErr); ok {
			for _, item := range fieldErr.Fields {
				id := fieldID(item.Field)
				if _, exists := errorsByField[id]; !exists {
					errorsByField[id] = titleCase(item.Message)
				}
			}
		}
	}

	return target, options, errorsByField
}
