//! Checking direct rules against fixture workspaces. `orders` and `orders-broken` are the design
//! doc's worked example; `features` covers how a source crate's features and build dependencies
//! count. Each fixture's README has a diagram.

#![expect(
    clippy::expect_used,
    reason = "test helpers fail the test by panicking"
)]

mod common;

use std::sync::LazyLock;

use stowage_rules::{Config, Graph, PathKind};

static ORDERS: LazyLock<Graph> = LazyLock::new(|| common::fixture("orders"));
static ORDERS_BROKEN: LazyLock<Graph> = LazyLock::new(|| common::fixture("orders-broken"));
static FEATURES: LazyLock<Graph> = LazyLock::new(|| common::fixture("features"));

/// The worked example's direct rules (all but `domain-is-independent-of-infrastructure`).
const WORKED_EXAMPLE: &str = r#"
    [groups]
    domain = { name = "orders-domain", workspace = true }
    entrypoints = { workspace = true, bin = true }
    test-harnesses = { name = "*-tests", workspace = true }
    serialisation = { name = ["serde", "serde_json"] }

    [[forbid]]
    name = "domain-is-the-bottom-layer"
    from = "domain"
    to = "workspace"
    reason = "Every other crate builds on the domain."

    [[forbid]]
    name = "only-test-harnesses-depend-on-entrypoints"
    from = { not = "test-harnesses" }
    to = "entrypoints"
    reason = "Code another crate needs belongs in a library."

    [[forbid]]
    name = "nothing-depends-on-test-harnesses"
    from = "workspace"
    to = "test-harnesses"
    reason = "Test harnesses must never reach a deployable."

    [[forbid]]
    name = "domain-does-not-serialise"
    from = "domain"
    to = "serialisation"
    reason = "A serialised form is a contract owned by another crate."
"#;

/// A violation as (rule, source, target, target group, path kind).
type Found = (String, String, String, String, PathKind);

fn violations(toml: &str, graph: &Graph) -> Vec<Found> {
    let config = Config::parse(toml, graph).expect("config is valid");
    config
        .check(graph)
        .violations()
        .iter()
        .map(|v| {
            (
                v.rule().to_owned(),
                v.source().to_owned(),
                v.target().to_owned(),
                v.target_group().to_owned(),
                v.kind(),
            )
        })
        .collect()
}

fn found(rule: &str, source: &str, target: &str, group: &str, kind: PathKind) -> Found {
    (
        rule.to_owned(),
        source.to_owned(),
        target.to_owned(),
        group.to_owned(),
        kind,
    )
}

#[test]
fn worked_example_passes_on_a_workspace_that_follows_it() {
    // Includes `orders-tests → orders-api`, which the negated `from` of
    // `only-test-harnesses-depend-on-entrypoints` allows.
    assert_eq!(violations(WORKED_EXAMPLE, &ORDERS), []);
}

/// Violations are ordered by rule (as written), then source, then target.
#[test]
fn worked_example_catches_each_direct_violation() {
    assert_eq!(
        violations(WORKED_EXAMPLE, &ORDERS_BROKEN),
        [
            found(
                "domain-is-the-bottom-layer",
                "orders-domain",
                "orders-ids",
                "workspace",
                PathKind::Normal
            ),
            found(
                "only-test-harnesses-depend-on-entrypoints",
                "orders-api",
                "orders-worker",
                "entrypoints",
                PathKind::Normal
            ),
            found(
                "nothing-depends-on-test-harnesses",
                "orders-worker",
                "orders-tests",
                "test-harnesses",
                PathKind::Dev
            ),
            // Only `orders-domain`'s own `serde` feature turns this dependency on.
            found(
                "domain-does-not-serialise",
                "orders-domain",
                "serde",
                "serialisation",
                PathKind::Normal
            ),
        ]
    );
}

#[test]
fn only_the_rules_kinds_count() {
    let toml = r#"
        [groups]
        test-harnesses = { name = "*-tests", workspace = true }

        [[forbid]]
        name = "r"
        from = "workspace"
        to = "test-harnesses"
        kinds = ["normal", "build"]
        reason = "r"
    "#;
    assert_eq!(violations(toml, &ORDERS_BROKEN), []);
}

#[test]
fn a_target_reached_by_several_edges_is_reported_once_with_the_most_serious_kind() {
    // `domain` uses `dual` both as a normal dependency and as a build dependency.
    let rule = |kinds: &str| {
        format!(
            r#"
            [groups]
            domain = {{ name = "domain", workspace = true }}
            dual = {{ name = "dual" }}

            [[forbid]]
            name = "r"
            from = "domain"
            to = "dual"
            kinds = {kinds}
            reason = "r"
            "#
        )
    };
    assert_eq!(
        violations(&rule(r#"["normal", "build", "dev"]"#), &FEATURES),
        [found("r", "domain", "dual", "dual", PathKind::Normal)]
    );
    assert_eq!(
        violations(&rule(r#"["build"]"#), &FEATURES),
        [found("r", "domain", "dual", "dual", PathKind::Build)]
    );
    assert_eq!(violations(&rule(r#"["dev"]"#), &FEATURES), []);
}

#[test]
fn a_target_in_several_groups_is_labelled_with_the_first() {
    let toml = r#"
        [groups]
        domain = { name = "domain", workspace = true }
        serialisation = { name = "serde" }

        [[forbid]]
        name = "r"
        from = "domain"
        to = ["serialisation", "external"]
        reason = "r"
    "#;
    assert_eq!(
        violations(toml, &FEATURES),
        [
            found("r", "domain", "dual", "external", PathKind::Normal),
            found("r", "domain", "serde", "serialisation", PathKind::Normal),
        ]
    );
}

#[test]
fn only_workspace_members_are_sources() {
    // `dual → sqlx-core` is an edge from an external crate, so it isn't checked. `shared` reaches
    // `sqlx-core` through its own optional `sqlx` feature, which counts when it's the source.
    let toml = r#"
        [groups]
        sql = { name = "sqlx-core" }

        [[forbid]]
        name = "r"
        from = "workspace"
        to = "sql"
        reason = "r"
    "#;
    assert_eq!(
        violations(toml, &FEATURES),
        [found("r", "shared", "sqlx-core", "sql", PathKind::Normal)]
    );
}

#[test]
fn the_report_includes_the_configs_warnings() {
    let config = Config::parse("", &ORDERS).expect("config is valid");
    assert_eq!(config.check(&ORDERS).warnings(), config.warnings());
}

#[test]
fn violations_carry_the_rules_reason() {
    let config = Config::parse(WORKED_EXAMPLE, &ORDERS_BROKEN).expect("config is valid");
    let report = config.check(&ORDERS_BROKEN);
    let reasons: Vec<(&str, &str)> = report
        .violations()
        .iter()
        .map(|v| (v.rule(), v.reason()))
        .collect();
    assert_eq!(
        reasons[0],
        (
            "domain-is-the-bottom-layer",
            "Every other crate builds on the domain."
        )
    );
}

#[test]
fn violations_are_ordered_by_source_then_target() {
    let toml = r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
        reason = "r"
    "#;
    assert_eq!(
        violations(toml, &FEATURES),
        [
            found("r", "domain", "dual", "external", PathKind::Normal),
            found("r", "domain", "serde", "external", PathKind::Normal),
            found("r", "shared", "sqlx-core", "external", PathKind::Normal),
        ]
    );
}
