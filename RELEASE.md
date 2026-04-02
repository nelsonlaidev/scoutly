# Release Guide

This project uses a tag-driven release flow.

## What Gets Published

- GitHub Release artifacts via `cargo-dist`
- Homebrew formula updates to `nelsonlaidev/homebrew-tap`
- npm package updates under `@nelsonlaidev`

The repository does not currently automate `cargo publish` to crates.io.

## Required Secrets

- `GITHUB_TOKEN`: provided by GitHub Actions
- `HOMEBREW_TAP_TOKEN`: push access to `nelsonlaidev/homebrew-tap`
- `NPM_TOKEN`: publish access to the `@nelsonlaidev` npm scope
- `CODECOV_TOKEN`: optional but recommended for coverage uploads

## Supported Release Targets

Release artifacts are intentionally limited to mainstream targets that are practical to support:

- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

## Pre-release Checklist

1. Confirm `main` is green in CI.
2. Update `Cargo.toml` version.
3. Add a new section to `CHANGELOG.md`.
4. Run:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

5. Verify installer and packaging config changes if release infrastructure was touched.

## Release Steps

1. Commit the version bump and changelog update.
2. Create an annotated tag that matches the Cargo version:

```bash
git tag -a v0.2.1 -m "Release v0.2.1"
```

3. Push the branch and tag:

```bash
git push origin main
git push origin v0.2.1
```

4. Wait for the `Release` workflow to finish.
5. Verify:
   - GitHub Release exists and contains archives/installers
   - Homebrew tap was updated
   - npm package was published

## After Release

- Smoke-test at least one install path:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/nelsonlaidev/scoutly/releases/download/v0.2.1/scoutly-installer.sh | sh
```

- Validate the binary:

```bash
scoutly --help
```

## Troubleshooting

- If tag and `Cargo.toml` version do not match, `cargo-dist` planning may fail.
- If Homebrew publish fails, confirm `HOMEBREW_TAP_TOKEN` can push to the tap repo.
- If npm publish fails, confirm the package version is new and `NPM_TOKEN` can publish to `@nelsonlaidev`.
