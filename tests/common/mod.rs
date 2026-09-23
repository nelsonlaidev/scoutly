use std::path::{Path, PathBuf};

pub(crate) mod server;

pub(crate) const AUDITED_AT_PLACEHOLDER: &str = "{{AUDITED_AT}}";
pub(crate) const ORIGIN_PLACEHOLDER: &str = "{{ORIGIN}}";

pub(crate) fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn fixture(relative: &str) -> PathBuf {
    repository_root().join("tests/fixtures").join(relative)
}
