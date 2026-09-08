# Scoutly

Scoutly audits a website by crawling its HTML pages, analyzing common SEO
problems, and checking discovered links and images.

Scoutly provides a script-friendly CLI, a keyboard-driven terminal interface,
and an importable Go library.

## Highlights

- Crawl same-origin HTML pages with configurable depth, page, concurrency, and
  request-rate limits.
- Find broken or redirected links and validate discovered images.
- Flag common SEO problems involving titles, meta descriptions, H1 headings,
  image alt text, thin content, and Open Graph metadata.
- Respect robots.txt rules and discover pages from XML and gzip sitemaps.
- Explore results interactively or emit text and machine-readable JSON reports.

## Installation

Install Scoutly on macOS or Linux with Homebrew:

```sh
brew install nelsonlaidev/tap/scoutly
```

Or install Scoutly on macOS, Linux, or Windows with npm and Node.js 22.14 or
newer:

```sh
npm install --global @nelsonlaidev/scoutly
```

Prebuilt archives for Linux, macOS, and Windows are available from the
[latest GitHub release](https://github.com/nelsonlaidev/scoutly/releases/latest).

If Go is already installed, you can instead build and install the CLI directly:

```sh
go install github.com/nelsonlaidev/scoutly/cmd/scoutly@latest
```

### Prerelease builds

Prereleases use Semantic Versioning tags such as `v0.5.0-beta.1`. Install the
latest prerelease through the opt-in beta channels:

```sh
brew install --cask nelsonlaidev/tap/scoutly@beta
npm install --global @nelsonlaidev/scoutly@beta
```

GitHub publishes prerelease archives under the exact version tag. Go users can
also install a specific prerelease directly:

```sh
go install github.com/nelsonlaidev/scoutly/cmd/scoutly@v0.5.0-beta.1
```

Stable installations are never advanced to a prerelease automatically.

## CLI

Audit a website by passing its URL:

```sh
scoutly https://example.com
```

Run Scoutly without a URL in an interactive terminal to open the full-screen
TUI:

```sh
scoutly
```

CLI flags and the discovered config file initialize the TUI fields:

```sh
scoutly --max-depth 2 --max-pages 100 --concurrency 10
```

When a URL is supplied, the final report is written to standard output, while
errors and optional progress remain on standard error.

```sh
scoutly https://example.com --max-depth 2 --max-pages 100
scoutly https://example.com --format json
scoutly https://example.com --progress never
```

Pressing `Ctrl+C` cancels every outstanding request, produces no partial
report, and exits with status 130. Inside the TUI, `Ctrl+C` cancels a running
audit without closing the interface; on other screens it exits normally.

Running without a URL when standard input or standard output is not attached
to a terminal fails instead of waiting for interactive input.

### TUI

The setup screen contains the target URL and every audit option in one
scrollable form. Use `Tab` or `Enter` to move forward, `Shift+Tab` to move back,
and `Space` to toggle settings. You can also click a field to focus it or click
a confirmation choice directly. Submit or click the final `Run audit` button
to start.

While an audit is running, Scoutly displays the active phase, elapsed time,
page and resource counters, the current URL, and the 200 most recent activity
entries. Scroll the recent activity pane with the mouse wheel; scrolling up
pauses automatic following until you return to the bottom. Cancelling or
failing an audit keeps the configured fields available for editing and
retrying.

Completed reports include five views:

- `1`–`5`: Overview, Issues, Pages, Links, and Images
- `/`: search the current result list
- `f`: cycle the current tab's filters
- `Tab`: switch between list and detail panes
- Arrow keys and Page Up/Down: navigate tabs, results, and details
- `Enter`: open or close full-page details at widths below 100 columns
- `n`: configure another audit while preserving the previous fields
- `q`: leave the TUI

You can also click a result tab or overview summary card, focus the search
field, cycle the current filter, select a result row, or focus a result pane.
The mouse wheel scrolls the result list or the overview and detail pane under
the pointer. At widths below 100 columns, clicking a row opens its details;
press `Esc` to return to the list.

At widths of 100 columns or more, result lists and details appear side by side.
Widths from 70 to 99 columns use one pane at a time. Smaller terminals show a
resize prompt without discarding the current state. Reports remain in the
alternate-screen TUI and are not written to standard output when the TUI exits.

### Configuration

Scoutly automatically discovers one of the following files in the current
directory:

- `scoutly.config.{json,yaml,yml,toml}`
- `scoutly.{json,yaml,yml,toml}`
- `.scoutly.{json,yaml,yml,toml}`

Configuration keys and rule names use snake case. CLI flags override file
values, and file values override defaults.

```toml
max_depth = 2
max_pages = 100
keep_fragments = false
ignore_redirects = false
rate_limit = 5
respect_robots = true
sitemaps = true
images = true
max_sitemap_documents = 1000
timeout = 30000
max_redirects = 10
user_agent = "scoutly/custom"
concurrency = 20
format = "text"
progress = "auto"

[rules]
title_too_short = "warning"
meta_description_too_short = "warning"
broken_link = "error"
```

`timeout` is expressed in milliseconds. `rate_limit = 0` disables global
request rate limiting. Supported progress modes are `auto`, `always`, and
`never`. Rule levels are `off`, `info`, `warning`, and `error`; omitted rules
keep their built-in level. Unknown rule names and invalid levels are rejected.

The following advisory rules are disabled by default to keep initial scans
focused on high-confidence findings:

- `title_too_short`
- `title_too_long`
- `meta_description_too_short`
- `meta_description_too_long`
- `thin_content`
- `missing_og_title`
- `missing_og_description`
- `missing_og_image`
- `missing_og_url`
- `missing_og_type`

The enabled defaults are:

- Error: `missing_title`, `missing_meta_description`, `page_crawl_failed`,
  `page_http_error`, `broken_link`, `invalid_image_url`, `broken_image`, and
  `invalid_image_content_type`
- Warning: `missing_image_alt`, `missing_h1`, `multiple_h1`,
  `link_check_blocked`, and `image_check_blocked`
- Info: `redirect` and `image_redirect`

Every name above can be used as a rule key. Reported issue codes remain
kebab-case for compatibility. Changing a rule only controls
issue reporting and severity; it does not skip crawling or resource checks.
`ignore_redirects` continues to suppress both `redirect` and `image_redirect`,
even when those rules are explicitly enabled.

Use an explicit file or disable discovery:

```sh
scoutly https://example.com --config ./audit.toml
scoutly https://example.com --no-config
```

Discovery fails when more than one conventional config file exists. JSON,
YAML, and TOML parsing is strict: unknown fields, duplicate fields, null
values, and multiple documents are rejected.

## Go library

Start with `DefaultOptions`, change the desired values, and pass a context to
`Audit`:

```go
package main

import (
	"context"
	"fmt"
	"log"

	"github.com/nelsonlaidev/scoutly/audit"
)

func main() {
	options := audit.DefaultOptions()
	options.MaxDepth = 2
	options.MaxPages = 100
	options.Rules["title_too_short"] = audit.RuleLevelWarning

	report, err := audit.Audit(
		context.Background(),
		"https://example.com",
		options,
		func(progress audit.Progress) error {
			fmt.Printf("%s: %s\n", progress.Phase, progress.CurrentURL)
			return nil
		},
	)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("%d issues\n", report.Summary.Issues.Total)
}
```

`Audit` returns a nil report on cancellation, invalid input, or progress
callback failure.

The JSON representation of `Report` uses stable snake_case field names.

## Audit behavior

- Crawling follows same-origin HTTP(S) links from `<a>` and `<iframe>` elements
  in deterministic breadth-first order. An initial redirect establishes the
  effective crawl origin, whose robots.txt policy and sitemaps are then used.
- External links are checked but never crawled.
- robots.txt is respected by default. A missing 4xx robots file allows
  crawling; failures that prevent determining the policy stop the audit with
  an error rather than producing an empty report.
- XML sitemap indexes, URL sets, namespaces, and gzip documents are supported.
  Sitemap-only pages are recorded at depth `-1`.
- Link and image requests are deduplicated by fragment-free URL, including
  resources that appear as both a link and an image.
- Image discovery covers `img src`, `img srcset`, `picture source srcset`, and
  `og:image`.
- Image checks inspect HTTP status and `Content-Type`; they do not decode image
  contents or inspect dimensions.

HTML responses are limited to 10 MiB, robots.txt to 512 KiB, and decompressed
sitemap documents to 50 MiB and 50,000 entries.

## Development

Development requires Go 1.26.6 or newer, [just](https://github.com/casey/just),
and golangci-lint v2. Clone the repository and run the project checks with:

```sh
git clone https://github.com/nelsonlaidev/scoutly.git
cd scoutly
go mod download
just build
just fmt
just lint
just test
just test-cover
just tidy
```

`just test` enables the race detector and disables cached test results. Tests
use local HTTP servers and do not depend on public websites. CI additionally
checks module tidiness, builds and tests on Linux, macOS, and Windows, runs
`govulncheck`, validates the npm package, and uploads coverage from the Linux
race-enabled test run.

When changing the npm installer, install its dependencies without running the
download hook, then test and inspect the package contents:

```sh
npm --prefix npm ci --ignore-scripts
npm --prefix npm test
npm pack ./npm --dry-run
```
