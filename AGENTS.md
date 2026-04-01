# Repository Guidelines

## Project Structure & Module Organization

- `src/` contains the Rust crate: `main.rs` is the CLI entry point, `lib.rs` orchestrates runs, and focused modules handle crawling, link checking, SEO analysis, reporting, config loading, robots rules, and HTTP behavior.
- `tests/` holds integration-style coverage. Use `tests/*_test.rs` for behavior suites, `tests/server/` for local Actix-based test servers, and `tests/static/` for HTML fixtures.
- `target/` is generated build output. CI and release automation live in `.github/workflows/`.

## Build, Test, and Development Commands

- `cargo build` compiles the debug binary.
- `cargo build --release` builds the distributable CLI at `target/release/scoutly`.
- `cargo run -- https://example.com --verbose` runs the crawler locally.
- `cargo test` runs the full test suite.
- `cargo fmt --all` formats Rust code.
- `cargo clippy --all-targets --all-features -- -D warnings` enforces lint-clean code.
- `cargo tarpaulin --out Xml --ignore-tests` generates the coverage report used in CI.
- Optional: `lefthook install` enables the pre-commit `fmt` and `clippy` hooks.

## Coding Style & Naming Conventions

- Follow `rustfmt` defaults: 4-space indentation and standard Rust formatting.
- Use `snake_case` for files, modules, functions, and tests; use `CamelCase` for types and enums.
- Keep CLI/output concerns in `cli`, `lib`, and `reporter`; keep crawl and network behavior in their dedicated modules.
- Prefer small, focused functions and return `anyhow::Result` for fallible top-level flows.

## Testing Guidelines

- Add or update tests in the closest matching `tests/<area>_test.rs` file.
- Prefer deterministic fixture-based coverage using `tests/static/` or the helpers in `tests/server/mod.rs`.
- CI enforces formatting, clippy, cross-platform tests, and Codecov thresholds of 95% project coverage (excluding `src/main.rs`).

## Commit & Pull Request Guidelines

- Match the repository’s Conventional Commit style: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Keep commits scoped and imperative, e.g. `fix: preserve machine-readable JSON output`.
- PRs should summarize user-visible changes, link related issues, list verification commands, and include sample CLI output when flags or reports change.

## Configuration & Security Tips

- Validate config handling with `scoutly.{json,toml,yaml}` or `~/.config/scoutly/config.*`.
- Do not commit secrets or real crawl credentials; use local fixtures for repeatable regression tests.
