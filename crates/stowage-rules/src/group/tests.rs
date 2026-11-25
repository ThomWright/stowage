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

/// A workspace member with only a lib target.
fn member(name: &str) -> Package {
    Package {
        name: name.to_owned(),
        version: "0.1.0".to_owned(),
        workspace_member: true,
        has_bin: false,
        proc_macro: false,
    }
}

/// A crate from outside the workspace, with only a lib target.
fn external(name: &str) -> Package {
    Package {
        workspace_member: false,
        ..member(name)
    }
}

impl Package {
    fn with_bin(self) -> Self {
        Self {
            has_bin: true,
            ..self
        }
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
    assert!(matches(groups, "exact", &member("orders-domain")));
    assert!(!matches(groups, "exact", &member("orders-domain-x")));
    assert!(matches(groups, "suffix", &member("orders-tests")));
    assert!(!matches(groups, "suffix", &member("tests-orders")));
    assert!(matches(groups, "single", &external("tokio-postgres")));
    assert!(!matches(groups, "single", &external("tokio-postgre")));
}

#[test]
fn name_list_matches_any_pattern() {
    let groups = r#"sql = { name = ["sqlx-core", "tokio-postgres"] }"#;
    assert!(matches(groups, "sql", &external("sqlx-core")));
    assert!(matches(groups, "sql", &external("tokio-postgres")));
    assert!(!matches(groups, "sql", &external("diesel")));
}

#[test]
fn names_match_regardless_of_dash_underscore_and_case() {
    let groups = r#"
        json = { name = "serde_json" }
        tests = { name = "*-tests" }
    "#;
    assert!(matches(groups, "json", &external("serde-json")));
    assert!(matches(groups, "json", &external("Serde_JSON")));
    assert!(matches(groups, "tests", &member("orders_tests")));
}

#[test]
fn conditions_are_all_required() {
    let groups = r#"
        harnesses = { name = "*-tests", workspace = true }
        entrypoints = { workspace = true, bin = true }
        libraries = { bin = false }
    "#;
    assert!(matches(groups, "harnesses", &member("orders-tests")));
    assert!(!matches(groups, "harnesses", &external("orders-tests")));
    assert!(matches(
        groups,
        "entrypoints",
        &member("orders-api").with_bin()
    ));
    assert!(!matches(groups, "entrypoints", &member("orders-api")));
    assert!(!matches(
        groups,
        "entrypoints",
        &external("some-cli").with_bin()
    ));
    assert!(matches(groups, "libraries", &member("orders-domain")));
    assert!(!matches(
        groups,
        "libraries",
        &member("orders-api").with_bin()
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
    let cli = member("orders-cli").with_bin();
    let domain = member("orders-domain");
    let group = groups.get("cli").expect("group exists");
    assert_eq!(
        group.unmatched_patterns(&[&cli, &domain]),
        ["orders-domain", "orders-typo"]
    );
}
