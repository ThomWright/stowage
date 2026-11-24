use std::collections::BTreeMap;

use super::{GroupDef, Groups};
use crate::graph::Package;
use crate::graph::fixtures::fixture;

fn parse(toml: &str) -> Groups {
    let defs: BTreeMap<String, GroupDef> =
        toml::from_str(toml).expect("groups have the right shape");
    let (groups, errors) = Groups::new(defs);
    assert_eq!(errors, [], "groups are valid");
    groups
}

fn package(name: &str, workspace_member: bool, has_bin: bool) -> Package {
    Package {
        name: name.to_owned(),
        version: "0.1.0".to_owned(),
        workspace_member,
        has_bin,
        proc_macro: false,
    }
}

fn matches(toml: &str, group: &str, package: &Package) -> bool {
    parse(toml)
        .get(group)
        .expect("group exists")
        .matches(package)
}

#[test]
fn name_matches_globs_and_exact_names() {
    let groups = r#"
        exact = { name = "orders-domain" }
        suffix = { name = "*-tests" }
        single = { name = "tokio-postgre?" }
    "#;
    assert!(matches(
        groups,
        "exact",
        &package("orders-domain", true, false)
    ));
    assert!(!matches(
        groups,
        "exact",
        &package("orders-domain-x", true, false)
    ));
    assert!(matches(
        groups,
        "suffix",
        &package("orders-tests", true, false)
    ));
    assert!(!matches(
        groups,
        "suffix",
        &package("tests-orders", true, false)
    ));
    assert!(matches(
        groups,
        "single",
        &package("tokio-postgres", false, false)
    ));
    assert!(!matches(
        groups,
        "single",
        &package("tokio-postgre", false, false)
    ));
}

#[test]
fn name_list_matches_any_pattern() {
    let groups = r#"sql = { name = ["sqlx-core", "tokio-postgres"] }"#;
    assert!(matches(groups, "sql", &package("sqlx-core", false, false)));
    assert!(matches(
        groups,
        "sql",
        &package("tokio-postgres", false, false)
    ));
    assert!(!matches(groups, "sql", &package("diesel", false, false)));
}

#[test]
fn names_match_regardless_of_dash_underscore_and_case() {
    let groups = r#"
        json = { name = "serde_json" }
        tests = { name = "*-tests" }
    "#;
    assert!(matches(
        groups,
        "json",
        &package("serde-json", false, false)
    ));
    assert!(matches(
        groups,
        "json",
        &package("Serde_JSON", false, false)
    ));
    assert!(matches(
        groups,
        "tests",
        &package("orders_tests", true, false)
    ));
}

#[test]
fn conditions_are_all_required() {
    let groups = r#"
        harnesses = { name = "*-tests", workspace = true }
        entrypoints = { workspace = true, bin = true }
        libraries = { bin = false }
    "#;
    assert!(matches(
        groups,
        "harnesses",
        &package("orders-tests", true, false)
    ));
    assert!(!matches(
        groups,
        "harnesses",
        &package("orders-tests", false, false)
    ));
    assert!(matches(
        groups,
        "entrypoints",
        &package("orders-api", true, true)
    ));
    assert!(!matches(
        groups,
        "entrypoints",
        &package("orders-api", true, false)
    ));
    assert!(!matches(
        groups,
        "entrypoints",
        &package("some-cli", false, true)
    ));
    assert!(matches(
        groups,
        "libraries",
        &package("orders-domain", true, false)
    ));
    assert!(!matches(
        groups,
        "libraries",
        &package("orders-api", true, true)
    ));
}

#[test]
fn built_in_groups_split_members_from_external_crates() {
    let graph = fixture("features");
    let groups = parse("");
    let names = |group: &str| {
        let group = groups.get(group).expect("built-in group exists");
        graph
            .packages()
            .iter()
            .filter(|p| group.matches(p))
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
    };

    assert_eq!(names("workspace"), ["api", "domain", "shared"]);
    assert_eq!(names("external"), ["dual", "serde", "sqlx-core"]);
}

#[test]
fn patterns_only_count_packages_meeting_the_other_conditions() {
    let groups =
        parse(r#"cli = { name = ["orders-cli", "orders-domain", "orders-typo"], bin = true }"#);
    let cli = package("orders-cli", true, true);
    let domain = package("orders-domain", true, false);
    let group = groups.get("cli").expect("group exists");
    assert_eq!(
        group.unmatched_patterns(&[&cli, &domain]),
        ["orders-domain", "orders-typo"]
    );
}
