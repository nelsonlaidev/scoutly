# Repository Guidelines

## Project Structure & Module Organization

- `src/lib.rs` exposes the asynchronous audit library and stable caller-facing types. Implementation modules under `src/` own crawling, transport, parsing, configuration, rules, progress, and report construction.
- `src/main.rs` and the binary-only `src/cli*.rs`, `src/output.rs`, and `src/tui/` modules own the executable, stream routing, signal handling, report formatting, and terminal interface.
- Unit tests live beside the Rust code in `#[cfg(test)]` modules. Library integration tests live in `tests/integration/`, executable end-to-end tests in `tests/e2e/`, shared test support in `tests/common/`, and deterministic fixtures in `tests/fixtures/`. Shared unit-test HTTP support remains in `src/test_server.rs` when crate-private access is required.
- `Cargo.toml`, `Cargo.lock`, and `rust-toolchain.toml` define the package and toolchain requirements. Development recipes live in `justfile`, and CI/release automation in `.github/workflows/`.
- `npm/` contains the npm CLI wrapper. Its install script selects a cargo-dist archive for the current platform, verifies it against `sha256.sum`, and installs the downloaded binary.

## Build, Test, and Development Commands

- `just build` compiles the library and CLI with the locked dependency graph.
- `just run https://example.com --progress never` runs the crawler locally.
- `just test` runs the locked Rust test suite.
- `just test-integration` runs the public library integration suite.
- `just test-e2e` runs the executable end-to-end suite.
- `just test-cover` writes `lcov.info` through `cargo llvm-cov`.
- `just fmt` applies rustfmt to all targets.
- `just lint` runs Clippy for all targets and features with warnings denied.
- `just docs` builds library documentation with warnings denied.
- `npm --prefix npm ci --ignore-scripts && npm --prefix npm test` checks the npm installer without downloading a release binary.
- `npm pack ./npm --dry-run` verifies the files included in the published npm package.
- The equivalent raw Cargo commands are acceptable when `just` is unavailable.

## Coding Style & Naming Conventions

- Let rustfmt determine source formatting and import layout.
- Use `snake_case` for modules, files, functions, and local variables; `PascalCase` for types and traits; and `SCREAMING_SNAKE_CASE` for constants.
- Put stable caller-facing types and behavior behind `src/lib.rs`; keep executable, output, and TUI concerns private to the binary crate.
- Use async cancellation through future dropping and Tokio primitives, preserve error sources with `thiserror`, and avoid printing or logging from library code.
- Prefer small, focused functions and the standard library. Add dependencies only when their production value justifies the maintenance cost.

## Testing Guidelines

- Add or update unit tests in the closest matching module. Use `tests/` when the public library, CLI process, or package boundary is what the test needs to exercise.
- Update `npm/install.test.js` whenever release archive names or supported Node.js platform mappings change.
- Prefer deterministic table-driven tests, loopback HTTP servers, `tests/fixtures/`, and the existing test server support. Do not depend on public websites.
- `tests/fixtures/` covers CLI, configuration, report, progress, HTTP request, HTML parsing, and TUI behavior. Tests may update fixtures deliberately as the product changes, but unintentional output or ordering drift must fail.
- The end-to-end audit test normalizes only `audited_at` values, the test server's dynamically allocated origin (including its port), and spinner frames or elapsed timing when a terminal renderer includes them. The non-terminal progress fixture has no spinner or timing data, so only the dynamic origin is replaced; request count and order are never normalized.
- Library integration tests and CLI end-to-end tests share these fixtures. Ratatui screens use semantic interaction tests plus fixed-size Insta snapshots; ANSI styling and widget padding are not part of the byte-level fixture.
- Keep tests isolated from process-wide state, shared working directories, and mutable fixtures so the Rust test harness can run them concurrently.
- CI enforces rustfmt, Clippy, docs, the declared MSRV, native Linux/macOS/Windows tests, `cargo audit`, stress tests, cargo-dist planning, crate/npm package validation, and Rust coverage uploaded to Codecov.

## Commit & Pull Request Guidelines

- Match the repository’s Conventional Commit style: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Keep commits scoped and imperative, e.g. `fix: preserve machine-readable JSON output`.
- PRs should summarize user-visible changes, link related issues, list verification commands, and include sample CLI output when flags or reports change.

## Releases

- Scoutly is tag-driven: pushing a new `v*` tag triggers the cargo-dist Release workflow, whose custom jobs publish the Homebrew cask, npm package, and crate. Only release when the user explicitly asks.
- Before tagging, confirm all of: (1) `main` is current — `git switch main && git pull --ff-only origin main`; (2) the CI run for `HEAD` passed — `gh run list --workflow "Continuous Integration" --commit "$(git rev-parse HEAD)" --limit 1 --json status,conclusion` shows success; (3) the tree is clean — `git status --porcelain` is empty.
- Create a new SemVer tag (`vX.Y.Z` or `vX.Y.Z-beta.N`) that does not exist on the remote, then push only that tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.

## Configuration & Security Tips

- Validate config discovery with `scoutly.config.*`, `scoutly.*`, and `.scoutly.*` JSON, YAML, or TOML fixtures in an isolated temporary directory.
- Keep HTTP behavior deterministic and bounded: preserve request cancellation, response-size limits, redirect limits, robots.txt handling, and configured concurrency/rate limits.
- Do not commit secrets or real crawl credentials; use local fixtures and loopback test servers for repeatable regression tests.
- Keep npm publishing on GitHub Actions trusted publishing. Do not add a long-lived npm token when OIDC is available, and do not bypass release checksum verification in the npm installer.
- Use Semantic Versioning release tags: `vX.Y.Z` for stable releases and `vX.Y.Z-beta.N` for prereleases. Prereleases publish to the npm `beta` dist-tag and the Homebrew `scoutly@beta` Cask without replacing stable channels.
