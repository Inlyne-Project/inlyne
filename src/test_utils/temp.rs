use std::path::PathBuf;

use tempfile::{Builder, NamedTempFile, TempDir};

const TEST_PREFIX: &str = "inlyne-tests-";

pub fn dir() -> (TempDir, PathBuf) {
    let dir = Builder::new().prefix(TEST_PREFIX).tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

pub fn file_with_suffix(suffix: &str) -> (NamedTempFile, PathBuf) {
    let file = Builder::new()
        .prefix(TEST_PREFIX)
        .suffix(suffix)
        .tempfile()
        .unwrap();
    let path = file.path().canonicalize().unwrap();
    (file, path)
}
