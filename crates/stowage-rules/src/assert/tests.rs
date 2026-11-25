//! Tests of checking a workspace, against the fixtures' `stowage.toml` files.

use std::error::Error as _;

use super::{Failure, check_workspace};
use crate::graph::fixtures::{fixture, fixture_manifest};
use crate::{ConfigWarning, Graph};

/// `message`, with this crate's directory replaced, so snapshots don't depend on where the
/// repository is checked out.
fn redacted(message: &str) -> String {
    message.replace(env!("CARGO_MANIFEST_DIR"), "[crate]")
}

#[test]
fn a_workspace_that_follows_its_rules_passes() {
    let graph = Ok(fixture("orders"));

    let report = check_workspace(&graph).expect("the workspace should pass");

    assert_eq!(report.violations(), []);
    assert_eq!(report.warnings(), []);
}

#[test]
fn warnings_alone_dont_fail_the_check() {
    let graph = Ok(fixture("kinds"));

    let report = check_workspace(&graph).expect("the workspace should pass");

    assert_eq!(report.warnings(), [ConfigWarning::NoRules]);
}

#[test]
fn a_broken_rule_fails_the_check_with_the_report() {
    let graph = Ok(fixture("orders-broken"));

    let failure = check_workspace(&graph).expect_err("the workspace should fail");

    let Failure::Violations(report) = &failure else {
        panic!("expected violations, got {failure:?}");
    };
    assert_eq!(
        report.violations().len(),
        5,
        "orders-broken should break each of the worked example's five rules once"
    );
    assert_eq!(failure.to_string(), report.to_string());
}

#[test]
fn a_missing_config_fails_the_check() {
    let graph = Ok(fixture("features"));

    let failure = check_workspace(&graph).expect_err("the workspace should fail");

    assert!(
        matches!(failure, Failure::MissingConfig { .. }),
        "expected a missing config, got {failure:?}"
    );
    insta::assert_snapshot!(
        redacted(&failure.to_string()),
        @"no stowage.toml at [crate]/tests/fixtures/features"
    );
}

#[test]
fn an_invalid_config_fails_the_check_with_every_error() {
    let graph = Ok(fixture("bin-only"));

    let failure = check_workspace(&graph).expect_err("the workspace should fail");

    assert!(
        matches!(failure, Failure::InvalidConfig { .. }),
        "expected an invalid config, got {failure:?}"
    );
    insta::assert_snapshot!(redacted(&failure.to_string()), @"
    stowage.toml at [crate]/tests/fixtures/bin-only is invalid:
    rule name `no tools` may only contain letters, digits, `-` and `_`
    rule `no tools` refers to unknown group `tool`
    rule `no tools` has an empty `reason`; say why the dependency is forbidden
    ");
}

#[test]
fn a_workspace_that_cant_be_loaded_fails_the_check_with_cargos_error() {
    let missing = fixture_manifest("missing");
    let graph = Graph::load_from(&missing);

    let failure = check_workspace(&graph).expect_err("the workspace should fail");

    let Failure::Load(error) = &failure else {
        panic!("expected a load failure, got {failure:?}");
    };
    let cause = error
        .source()
        .expect("the load error has a source")
        .to_string();
    let message = failure.to_string();
    assert!(
        message.starts_with("failed to load the workspace's dependency graph: "),
        "the message should say what failed: {message}"
    );
    assert!(
        message.contains(&cause),
        "the message should include cargo's error, {cause:?}: {message}"
    );
}
