//! Loading a workspace's dependency graph.

use std::error::Error;
use std::path::PathBuf;

use stowage_rules::Graph;

#[test]
fn a_workspace_that_cargo_cant_read_fails_to_load() {
    let missing: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "missing", "Cargo.toml"]
        .iter()
        .collect();

    let error = Graph::load_from(missing).expect_err("a missing manifest fails to load");

    assert!(
        error.source().is_some(),
        "the error should keep what cargo reported as its source"
    );
}
