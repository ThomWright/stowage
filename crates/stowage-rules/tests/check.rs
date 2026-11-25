//! Checking rules against fixture workspaces, and rendering the report. `orders` and
//! `orders-broken` are the design doc's worked example; `features` covers how a source crate's
//! features and build dependencies count; `versions` has two versions of one crate. Each
//! fixture's README has a diagram.

#![expect(
    clippy::expect_used,
    reason = "test helpers fail the test by panicking"
)]

mod common;

use std::sync::LazyLock;

use stowage_rules::{Config, Graph, Hop, PathKind};

static ORDERS: LazyLock<Graph> = LazyLock::new(|| common::fixture("orders"));
static ORDERS_BROKEN: LazyLock<Graph> = LazyLock::new(|| common::fixture("orders-broken"));
static FEATURES: LazyLock<Graph> = LazyLock::new(|| common::fixture("features"));
static VERSIONS: LazyLock<Graph> = LazyLock::new(|| common::fixture("versions"));

/// The design doc's worked example, which `orders` follows and `orders-broken` breaks.
const WORKED_EXAMPLE: &str = include_str!("fixtures/orders/stowage.toml");

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

fn report(toml: &str, graph: &Graph) -> String {
    Config::parse(toml, graph)
        .expect("config is valid")
        .check(graph)
        .to_string()
}

/// Each violation's (source, path), with each hop as `name@version`.
fn paths(toml: &str, graph: &Graph) -> Vec<(String, Vec<String>)> {
    let config = Config::parse(toml, graph).expect("config is valid");
    config
        .check(graph)
        .violations()
        .iter()
        .map(|v| {
            let path = v
                .path()
                .iter()
                .map(|hop| format!("{}@{}", hop.name(), hop.version()))
                .collect();
            (v.source().to_owned(), path)
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
fn worked_example_catches_each_violation() {
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
            // Through `db-pool`, which is not itself in any of the rule's groups.
            found(
                "domain-is-independent-of-infrastructure",
                "orders-domain",
                "sqlx-core",
                "sql",
                PathKind::Normal
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
fn only_the_rules_kinds_are_violations() {
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

#[test]
fn a_direct_rule_ignores_dependencies_of_dependencies() {
    // `orders-domain` reaches `sqlx-core` only through `db-pool`.
    let toml = r#"
        [groups]
        domain = { name = "orders-domain", workspace = true }
        sql = { name = "sqlx-core" }

        [[forbid]]
        name = "r"
        from = "domain"
        to = "sql"
        reason = "r"
    "#;
    assert_eq!(violations(toml, &ORDERS_BROKEN), []);
}

#[test]
fn a_transitive_rule_only_checks_paths_from_its_sources() {
    // `orders-api` and `orders-worker` reach `sqlx-core` through `orders-db`, but only the domain
    // is a source.
    let toml = r#"
        [groups]
        domain = { name = "orders-domain", workspace = true }
        sql = { name = "sqlx-core" }

        [[forbid]]
        name = "r"
        from = "domain"
        to = "sql"
        reach = "transitive"
        reason = "r"
    "#;
    assert_eq!(violations(toml, &ORDERS), []);
}

#[test]
fn a_transitive_path_through_a_build_dependency_is_a_build_path() {
    // `domain` reaches `sqlx-core` only through `dual`'s `sqlx` feature, which its build
    // dependency on `dual` turns on, so only on the host.
    let rule = |kinds: &str| {
        format!(
            r#"
            [groups]
            domain = {{ name = "domain", workspace = true }}
            sql = {{ name = "sqlx-core" }}

            [[forbid]]
            name = "r"
            from = "domain"
            to = "sql"
            reach = "transitive"
            kinds = {kinds}
            reason = "r"
            "#
        )
    };
    assert_eq!(
        violations(&rule(r#"["normal", "build", "dev"]"#), &FEATURES),
        [found("r", "domain", "sqlx-core", "sql", PathKind::Build)]
    );
    assert_eq!(violations(&rule(r#"["normal"]"#), &FEATURES), []);
}

#[test]
fn violations_carry_a_shortest_path_from_the_source_to_the_target() {
    let config = Config::parse(WORKED_EXAMPLE, &ORDERS_BROKEN).expect("config is valid");
    let report = config.check(&ORDERS_BROKEN);
    let paths: Vec<(&str, Vec<&str>)> = report
        .violations()
        .iter()
        .map(|v| (v.rule(), v.path().iter().map(Hop::name).collect()))
        .collect();
    assert_eq!(
        paths,
        [
            ("domain-is-the-bottom-layer", vec!["orders-ids"]),
            (
                "only-test-harnesses-depend-on-entrypoints",
                vec!["orders-worker"]
            ),
            ("nothing-depends-on-test-harnesses", vec!["orders-tests"]),
            (
                "domain-is-independent-of-infrastructure",
                vec!["db-pool", "sqlx-core"]
            ),
            ("domain-does-not-serialise", vec!["serde"]),
        ]
    );
}

/// `app` reaches `codec` 1.0.0 directly and 2.0.0 through `codec-compat`, and each is a
/// violation of its own.
const CODEC: &str = r#"
    [groups]
    codec = { name = "codec" }

    [[forbid]]
    name = "no-codec"
    from = "workspace"
    to = "codec"
    reach = "transitive"
    reason = "Encoding belongs at the edges."
"#;

#[test]
fn each_version_of_a_target_is_a_separate_violation() {
    let s = |hops: &[&str]| hops.iter().map(|&hop| hop.to_owned()).collect::<Vec<_>>();
    assert_eq!(
        paths(CODEC, &VERSIONS),
        [
            ("app".to_owned(), s(&["codec@1.0.0"])),
            ("app".to_owned(), s(&["codec-compat@0.1.0", "codec@2.0.0"])),
            ("lib".to_owned(), s(&["codec-compat@0.1.0", "codec@2.0.0"])),
        ]
    );
}

// Rendering. These snapshot the report people read; the tests above check behaviour.

#[test]
fn the_report_shows_each_violation_with_its_path_and_reason() {
    insta::assert_snapshot!(report(WORKED_EXAMPLE, &ORDERS_BROKEN), @r#"
    error[domain-is-the-bottom-layer]: orders-domain depends on orders-ids (workspace)
      orders-domain → orders-ids
      reason: Every other crate builds on the domain.

    error[only-test-harnesses-depend-on-entrypoints]: orders-api depends on orders-worker (entrypoints)
      orders-api → orders-worker
      reason: Code another crate needs belongs in a library.

    error[nothing-depends-on-test-harnesses]: orders-worker depends on orders-tests (test-harnesses)
      orders-worker → orders-tests (dev)
      reason: Test harnesses bring in mocks and fixtures, which must never end up in a deployable.

    error[domain-is-independent-of-infrastructure]: orders-domain depends on sqlx-core (sql)
      orders-domain → db-pool → sqlx-core
      reason: Keeping transport and storage out of the domain means its rules can be tested without infrastructure.

    error[domain-does-not-serialise]: orders-domain depends on serde (serialisation)
      orders-domain → serde
      reason: A serialised form is a contract owned by another crate.

    5 violations of 5 rules
    "#);
}

#[test]
fn build_paths_are_labelled() {
    let toml = r#"
        [groups]
        domain = { name = "domain", workspace = true }
        sql = { name = "sqlx-core" }

        [[forbid]]
        name = "domain-avoids-sql"
        from = "domain"
        to = "sql"
        reach = "transitive"
        reason = "The domain is tested without a database."
    "#;
    insta::assert_snapshot!(report(toml, &FEATURES), @r#"
    error[domain-avoids-sql]: domain depends on sqlx-core (sql)
      domain → dual → sqlx-core (build)
      reason: The domain is tested without a database.

    1 violation of 1 rule
    "#);
}

#[test]
fn crate_names_with_several_versions_are_shown_with_their_version() {
    insta::assert_snapshot!(report(CODEC, &VERSIONS), @r#"
    error[no-codec]: app depends on codec@1.0.0 (codec)
      app → codec@1.0.0
      reason: Encoding belongs at the edges.

    error[no-codec]: app depends on codec@2.0.0 (codec)
      app → codec-compat → codec@2.0.0
      reason: Encoding belongs at the edges.

    error[no-codec]: lib depends on codec@2.0.0 (codec)
      lib → codec-compat → codec@2.0.0
      reason: Encoding belongs at the edges.

    3 violations of 1 rule
    "#);
}

#[test]
fn warnings_come_before_violations_one_per_line() {
    // `fakes` and `harnesses` match nothing, so negating them excludes nothing, which is warned
    // about for each.
    let toml = r#"
        [groups]
        fakes = { name = "*-fake" }
        harnesses = { name = "*-harness" }
        test-harnesses = { name = "*-tests", workspace = true }

        [[forbid]]
        name = "nothing-depends-on-test-harnesses"
        from = { not = ["fakes", "harnesses"] }
        to = "test-harnesses"
        reason = "Test harnesses bring in mocks and fixtures, which must never end up in a deployable."
    "#;
    insta::assert_snapshot!(report(toml, &ORDERS_BROKEN), @r#"
    warning: group `fakes` matches no package, so negating it in `from` excludes nothing; was a crate renamed?
    warning: group `harnesses` matches no package, so negating it in `from` excludes nothing; was a crate renamed?

    error[nothing-depends-on-test-harnesses]: orders-worker depends on orders-tests (test-harnesses)
      orders-worker → orders-tests (dev)
      reason: Test harnesses bring in mocks and fixtures, which must never end up in a deployable.

    1 violation of 1 rule
    "#);
}

#[test]
fn a_report_without_violations_shows_only_its_warnings() {
    insta::assert_snapshot!(report("", &ORDERS), @"warning: the config has no `[[forbid]]` rules, so it checks nothing");
    assert_eq!(
        report(WORKED_EXAMPLE, &ORDERS),
        "",
        "a report with no violations or warnings should be empty"
    );
}
