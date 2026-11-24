//! Helpers shared by the integration tests.

use std::path::PathBuf;

use stowage_rules::Graph;

/// Loads the fixture workspace in `tests/fixtures/<name>`.
///
/// # Panics
///
/// If the fixture doesn't load.
pub(crate) fn fixture(name: &str) -> Graph {
    let manifest: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        name,
        "Cargo.toml",
    ]
    .iter()
    .collect();
    #[expect(
        clippy::expect_used,
        reason = "a fixture that doesn't load fails the test"
    )]
    Graph::load_from(manifest).expect("fixture loads")
}
