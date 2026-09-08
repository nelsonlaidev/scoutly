# Repository Guidelines

## Project Structure & Module Organization

- `cmd/scoutly/` contains the executable entry point, CLI flags, signal handling, and CLI/TUI mode selection.
- `audit/` is the public library package. It owns audit orchestration, options, progress events, report models, and report construction.
- `internal/` contains implementation packages for crawling, fetching, resource checking, page parsing, configuration, robots.txt, sitemaps, CLI output, and the TUI. Keep implementation-only packages internal unless callers need a stable public API.
- Tests live beside the code as `*_test.go`. Package fixtures belong in a nearby `testdata/` directory, and shared test-only helpers belong in `internal/testutil/`.
- `go.mod` and `go.sum` define the module and toolchain requirements. Development recipes live in `justfile`, lint configuration in `.golangci.yml`, and CI/release automation in `.github/workflows/`.
- `npm/` contains the npm CLI wrapper. Its install script selects a GoReleaser archive for the current platform, verifies its checksum, and installs the downloaded binary.

## Build, Test, and Development Commands

- `just build` compiles the CLI to `./scoutly` using `go build ./cmd/scoutly`.
- `just run https://example.com --progress never` runs the crawler locally.
- `just test` runs `go test -race -count=1 ./...` across every package.
- `just test-cover` writes an atomic coverage profile to `coverage.out` and prints the function summary.
- `just fmt` applies `gofmt` and the formatters configured in `.golangci.yml`.
- `just lint` runs `go vet ./...` and golangci-lint.
- `just tidy` updates `go.mod` and `go.sum`; review both files after running it.
- `npm --prefix npm ci --ignore-scripts && npm --prefix npm test` checks the npm installer without downloading a release binary.
- `npm pack ./npm --dry-run` verifies the files included in the published npm package.
- The equivalent raw Go commands are acceptable when `just` is unavailable.

## Coding Style & Naming Conventions

- Let `gofmt` and the configured goimports formatter determine source and import formatting.
- Use short lowercase package names, `snake_case.go` file names where multiple words are needed, `PascalCase` for exported identifiers, and `camelCase` for unexported identifiers. Preserve conventional initialisms such as `URL`, `HTTP`, and `CLI`.
- Put stable caller-facing types and behavior in `audit`; keep executable, output, TUI, and implementation concerns in `cmd/scoutly` or the relevant `internal` package.
- Pass `context.Context` as the first parameter for cancellable work, propagate cancellation, wrap errors with operation context using `%w`, and avoid logging from library packages.
- Prefer small, focused functions and the standard library. Add dependencies only when their production value justifies the maintenance cost.

## Testing Guidelines

- Add or update tests in the closest matching `*_test.go` file. Use external test packages only when the public API boundary is what the test needs to exercise.
- Update `npm/install.test.js` whenever release archive names or supported Node.js platform mappings change.
- Prefer deterministic table-driven tests, `httptest` servers, package-local `testdata/` fixtures, and the helpers in `internal/testutil/`. Do not depend on public websites.
- Call `t.Parallel()` only when the test does not mutate process-wide state, environment, working directories, or shared fixtures.
- CI enforces module tidiness, formatting, `go vet`, golangci-lint, `govulncheck`, cross-platform build/test coverage, npm package validation, and a Linux race-enabled coverage run uploaded to Codecov.

## Commit & Pull Request Guidelines

- Match the repository’s Conventional Commit style: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Keep commits scoped and imperative, e.g. `fix: preserve machine-readable JSON output`.
- PRs should summarize user-visible changes, link related issues, list verification commands, and include sample CLI output when flags or reports change.

## Releases

- Scoutly is tag-driven: pushing a new `v*` tag triggers the Release workflow (GoReleaser creates the GitHub Release and Homebrew cask, then the npm job publishes). Only release when the user explicitly asks.
- Before tagging, confirm all of: (1) `main` is current — `git switch main && git pull --ff-only origin main`; (2) the CI run for `HEAD` passed — `gh run list --workflow "Continuous Integration" --commit "$(git rev-parse HEAD)" --limit 1 --json status,conclusion` shows success; (3) the tree is clean — `git status --porcelain` is empty.
- Create a new SemVer tag (`vX.Y.Z` or `vX.Y.Z-beta.N`) that does not exist on the remote, then push only that tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.

## Configuration & Security Tips

- Validate config discovery with `scoutly.config.*`, `scoutly.*`, and `.scoutly.*` JSON, YAML, or TOML fixtures in an isolated temporary directory.
- Keep HTTP behavior deterministic and bounded: preserve request cancellation, response-size limits, redirect limits, robots.txt handling, and configured concurrency/rate limits.
- Do not commit secrets or real crawl credentials; use local fixtures and `httptest` servers for repeatable regression tests.
- Keep npm publishing on GitHub Actions trusted publishing. Do not add a long-lived npm token when OIDC is available, and do not bypass release checksum verification in the npm installer.
- Use Semantic Versioning release tags: `vX.Y.Z` for stable releases and `vX.Y.Z-beta.N` for prereleases. Prereleases publish to the npm `beta` dist-tag and the Homebrew `scoutly@beta` Cask without replacing stable channels.
