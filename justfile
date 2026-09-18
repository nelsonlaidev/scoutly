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
