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

- `aarch64-apple-darwin`
- `aarch64-unknown-linux-gnu`
- `aarch64-pc-windows-msvc`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `x86_64-pc-windows-msvc`

## Release Steps

1. Run tests

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

2. Update the version in `Cargo.toml` (e.g., from `0.2.0` to `0.2.1`).

3. Create an annotated tag with release notes

```bash
git tag v0.2.1 -m "Highlights:
- Added GitHub OAuth
- Improved search ranking
- Breaking: renamed config file from scoutly.json to scoutly.toml"
```

3. Push the branch and tag

```bash
git push origin main
git push origin v0.2.1
```

4. Generate release notes

```bash
git cliff -o CHANGELOG.md
```

5. Commit the changelog and version changes

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "chore(release): prepare v0.2.1"
```

5. Push the branch and the tag

```bash
git push
git push --tags
```

6. Wait for the `Release` workflow to finish.
7. Verify:
   - GitHub Release exists and contains archives/installers
   - Homebrew tap was updated
   - npm package was published
