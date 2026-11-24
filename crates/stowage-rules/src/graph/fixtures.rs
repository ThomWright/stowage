//! Loading the fixture workspaces in `tests/fixtures`, for tests.

use std::path::PathBuf;

use guppy::MetadataCommand;

use super::Graph;

/// The root manifest of the fixture workspace called `name`.
pub(crate) fn fixture_manifest(name: &str) -> PathBuf {
    [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        name,
        "Cargo.toml",
    ]
    .iter()
    .collect()
}

/// Loads the fixture workspace called `name`, without touching the network or rewriting its
/// committed lockfile.
pub(crate) fn fixture(name: &str) -> Graph {
    let mut command = MetadataCommand::new();
    command
        .manifest_path(fixture_manifest(name))
        .other_options(["--locked".to_owned(), "--offline".to_owned()]);
    Graph::from_command(command).expect("fixture loads")
}
