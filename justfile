default:
    @just --list

run *args:
    @cargo run --locked -- {{ args }}

build:
    @cargo build --locked

check:
    @cargo check --locked --all-targets --all-features

fmt:
    @cargo fmt --all

lint:
    @cargo clippy --locked --all-targets --all-features -- -D warnings

docs:
    @RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features

changelog-prerelease version:
    @git cliff --unreleased --tag {{version}} --prepend CHANGELOG.md

changelog-stable version:
    #!/usr/bin/env bash
    set -euo pipefail
    previous="$(git describe --tags --abbrev=0 --match 'v[0-9]*' --exclude 'v*-*')"
    git cliff "${previous}..HEAD" --tag "{{version}}" --ignore-tags 'v[0-9]+\.[0-9]+\.[0-9]+-(alpha|beta|rc)\..*' --prepend CHANGELOG.md

test:
    @cargo test --locked

test-integration:
    @cargo test --locked --test integration

test-e2e:
    @cargo test --locked --test e2e

test-cover:
    @cargo llvm-cov --locked --all-features --workspace --lcov --output-path lcov.info

audit:
    @cargo audit --deny warnings

dist-check:
    @dist generate --check
    @dist plan --output-format=json

clean:
    @cargo clean
