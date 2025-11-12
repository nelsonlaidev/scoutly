# Scoutly

A fast, lightweight CLI website crawler and SEO analyzer built with Rust. Scoutly is inspired by Scrutiny and helps you analyze websites for broken links, SEO issues, and overall site health.

## Features

- **Website Crawling**: Recursively crawl websites with configurable depth limits
- **Link Checking**: Validate all internal and external links, detect broken links (404s, 500s)
- **SEO Analysis**:
  - Check for missing or poorly optimized title tags
  - Validate meta descriptions
  - Detect missing or multiple H1 tags
  - Find images without alt text
  - Identify thin content
- **Configuration Files**: Support for JSON, TOML, and YAML configuration files with automatic detection
- **Flexible Reporting**: Output results in human-readable text or JSON format
- **Fast & Concurrent**: Built with Tokio for async I/O and parallel link checking
- **robots.txt Support**: Respects robots.txt rules by default

## Prerequisites

- **Rust** (1.91 or later) - [Install Rust](https://www.rust-lang.org/tools/install)
- **Cargo** (comes with Rust)

### Optional Development Tools

- **Lefthook** - Git hooks manager for running linters and formatters automatically
  ```bash
  # macOS
  brew install lefthook

  # After installation, initialize hooks
  lefthook install
  ```

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/nelsonlaidev/scoutly.git
cd scoutly

# Build the project
cargo build --release

# The binary will be at target/release/scoutly
```

## Usage

### Basic Usage

```bash
# Crawl a website with default settings (depth: 5, max pages: 200)
scoutly https://example.com

# Specify custom depth and page limits
scoutly https://example.com --depth 3 --max-pages 100

# Enable verbose output to see progress
scoutly https://example.com --verbose
```

### Advanced Options

```bash
# Follow external links (by default, only internal links are followed)
scoutly https://example.com --external

# Ignore redirect issues in the report
scoutly https://example.com --ignore-redirects

# Treat URLs with fragment identifiers (#) as unique links
scoutly https://example.com --keep-fragments

# Output results in JSON format
scoutly https://example.com --output json

# Save report to a file
scoutly https://example.com --save report.json

# Combine options
scoutly https://example.com --depth 4 --max-pages 200 --verbose --ignore-redirects --save report.json
```

### Configuration Files

Scoutly supports configuration files in JSON, TOML, or YAML format. Configuration files allow you to set default values for options without having to specify them on the command line every time.

#### Default Configuration Paths

Scoutly automatically looks for configuration files in the following locations (in order of priority):

1. **Current directory:**
   - `scoutly.json`
   - `scoutly.toml`
   - `scoutly.yaml`
   - `scoutly.yml`

2. **User config directory:**
   - Linux/macOS: `~/.config/scoutly/config.{json,toml,yaml,yml}`
   - Windows: `%APPDATA%\scoutly\config.{json,toml,yaml,yml}`

#### Example Configuration Files

All configuration fields are optional. You can provide only the fields you want to customize.

**JSON** (`scoutly.json`):
```json
{
  "depth": 10,
  "max_pages": 500,
  "output": "json",
  "external": true,
  "verbose": true,
  "ignore_redirects": false,
  "keep_fragments": false,
  "rate_limit": 2.0,
  "concurrency": 10,
  "respect_robots_txt": true
}
```

**TOML** (`scoutly.toml`):
```toml
depth = 10
max_pages = 500
output = "json"
external = true
verbose = true
ignore_redirects = false
keep_fragments = false
rate_limit = 2.0
concurrency = 10
respect_robots_txt = true
```

**YAML** (`scoutly.yaml`):
```yaml
depth: 10
max_pages: 500
output: json
external: true
verbose: true
ignore_redirects: false
keep_fragments: false
rate_limit: 2.0
concurrency: 10
respect_robots_txt: true
```

#### Using a Custom Config File

You can specify a custom configuration file path using the `--config` option:

```bash
scoutly https://example.com --config ./my-config.json
```

#### Configuration Priority

Command-line arguments always take precedence over configuration file values. For example:

```bash
# If scoutly.json sets depth to 10, this command will use depth 15
scoutly https://example.com --depth 15
```

This allows you to set sensible defaults in your config file while still being able to override them when needed.

### Command Line Options

```
Usage: scoutly [OPTIONS] <URL>

Arguments:
  <URL>  The URL to start crawling from

Options:
  -d, --depth <DEPTH>              Maximum crawl depth (default: 5)
  -m, --max-pages <MAX_PAGES>      Maximum number of pages to crawl (default: 200)
  -o, --output <OUTPUT>            Output format: text or json [default: text]
  -s, --save <SAVE>                Save report to file
  -e, --external                   Follow external links
  -v, --verbose                    Verbose output
      --ignore-redirects           Ignore redirect issues in the report
      --keep-fragments             Treat URLs with fragment identifiers (#) as unique links
  -r, --rate-limit <RATE_LIMIT>    Rate limit for requests per second
  -c, --concurrency <CONCURRENCY>  Number of concurrent requests (default: 5)
      --respect-robots-txt         Respect robots.txt rules (default: true)
      --config <CONFIG>            Path to configuration file (JSON, TOML, or YAML)
  -h, --help                       Print help
```

## Example Output

### Text Report

```
================================================================================
Scoutly - Crawl Report
================================================================================

Start URL: https://example.com
Timestamp: 2025-11-03T16:05:29.911833+00:00

Summary
  Total Pages Crawled: 15
  Total Links Found:   127
  Broken Links:        2
  Errors:              3
  Warnings:            8
  Info:                5

Pages with Issues

  URL: https://example.com/about
    Status: 200
    Depth:  1
    Title:  About Us
    Issues:
      [WARN ] Page is missing a meta description
      [WARN ] 3 image(s) missing alt text

  URL: https://example.com/contact
    Status: 200
    Depth:  1
    Title:  Contact
    Issues:
      [ERROR] Broken link: https://example.com/old-page (HTTP 404)
```

### JSON Report

Use `--output json` to get machine-readable output suitable for integration with other tools or CI/CD pipelines.

## How It Works

1. **Crawling**: Starting from the provided URL, Scoutly fetches each page and extracts all links from various HTML elements (anchor tags, iframes, media elements, embeds, etc.)
2. **Link Discovery**: Internal links (same domain) are queued for crawling based on depth limits
3. **Link Validation**: All discovered links are checked asynchronously for HTTP status codes
4. **SEO Analysis**: Each page is analyzed for common SEO issues
5. **Report Generation**: Results are compiled into a comprehensive report

### Link Extraction

Scoutly extracts links from multiple HTML elements:

- `<a href>` - Standard hyperlinks
- `<iframe src>` - Embedded content
- `<video src>` and `<source src>` - Video content
- `<audio src>` - Audio content
- `<embed src>` - Embedded plugins
- `<object data>` - Embedded objects

## SEO Checks Performed

- **Title Tag**

  - Missing title
  - Title too short (< 50 characters, recommended: 50-60)
  - Title too long (> 60 characters, recommended: 50-60)

- **Meta Description**

  - Missing meta description
  - Description too short (< 150 characters, recommended: 150-160)
  - Description too long (> 160 characters, recommended: 150-160)

- **Headings**

  - Missing H1 tag
  - Multiple H1 tags

- **Images**

  - Missing alt attributes

- **Content**

  - Thin content detection (checks if page has fewer than 5 content indicators)

- **Links**
  - Broken links (4xx and 5xx status codes)
  - Redirect detection (3xx status codes)

## Performance

- Asynchronous I/O for fast crawling
- Concurrent link checking
- Configurable limits to prevent excessive resource usage
- Typical crawl speed: 10-20 pages per second (depending on target site and network)

## Limitations (Basic Version)

- No JavaScript rendering (only parses initial HTML)
- Basic content analysis (no detailed text analysis)
- No authentication support
- No sitemap generation (planned for future versions)

## Future Enhancements

- JavaScript rendering with headless browser support
- Sitemap generation (XML)
- Authentication support
- More advanced SEO checks (keyword density, structured data)
- Progress bar for long-running crawls
- HTML validation
- Accessibility checks (WCAG compliance)
- PDF and document crawling

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Author

- [@nelsonlaidev](https://github.com/nelsonlaidev)

## Donation

If you find this project helpful, consider supporting me by [sponsoring the project](https://github.com/sponsors/nelsonlaidev).

## License

This project is open source and available under the [MIT License](LICENSE).

---

<p align="center">
Made with ❤️ in Hong Kong
</p>
