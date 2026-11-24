use std::collections::BTreeMap;

use super::{GroupDef, GroupError, Groups};
use crate::graph::Package;
use crate::graph::fixtures::fixture;

fn parse(toml: &str) -> Result<Groups, Vec<GroupError>> {
    let defs: BTreeMap<String, GroupDef> =
        toml::from_str(toml).expect("groups have the right shape");
    Groups::new(defs)
}

/// The deserialisation error for groups of the wrong shape.
fn shape_error(toml: &str) -> String {
    toml::from_str::<BTreeMap<String, GroupDef>>(toml)
        .expect_err("groups have the wrong shape")
        .to_string()
}

fn errors(toml: &str) -> Vec<GroupError> {
    parse(toml).expect_err("groups are invalid")
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
        .expect("groups are valid")
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
    let groups = parse("").expect("no groups is valid");
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

fn invalid_pattern(group: &str, pattern: &str, character: char) -> GroupError {
    GroupError::InvalidPattern {
        group: group.to_owned(),
        pattern: pattern.to_owned(),
        character,
    }
}

#[test]
fn patterns_may_only_use_crate_name_characters_and_wildcards() {
    assert_eq!(
        errors(
            r#"
            braces = { name = "{serde,serde_json}" }
            brackets = { name = "serde[_-]json" }
            empty = { name = "" }
            space = { name = "serde json" }
            "#
        ),
        [
            invalid_pattern("braces", "{serde,serde_json}", '{'),
            invalid_pattern("brackets", "serde[_-]json", '['),
            GroupError::EmptyPattern {
                group: "empty".to_owned()
            },
            invalid_pattern("space", "serde json", ' '),
        ]
    );
}

#[test]
fn groups_that_would_match_nothing_or_everything_are_rejected() {
    assert_eq!(
        errors(
            "
            everything = {}
            none = { name = [] }
            "
        ),
        [
            GroupError::NoConditions {
                group: "everything".to_owned()
            },
            GroupError::EmptyNameList {
                group: "none".to_owned()
            },
        ]
    );
}

#[test]
fn built_in_group_names_are_reserved() {
    assert_eq!(
        errors(
            r#"
            external = { workspace = false }
            workspace = { name = "orders-*" }
            "#
        ),
        [
            GroupError::ReservedName {
                group: "external".to_owned()
            },
            GroupError::ReservedName {
                group: "workspace".to_owned()
            },
        ]
    );
}

#[test]
fn group_names_may_only_use_letters_digits_dashes_and_underscores() {
    assert_eq!(
        errors(r#""!domain" = { name = "orders-domain" }"#),
        [GroupError::InvalidName {
            group: "!domain".to_owned()
        }]
    );
}

#[test]
fn every_problem_in_a_group_is_reported() {
    assert_eq!(
        errors(r#"a = { name = ["x y", "", "a[b]"] }"#),
        [
            invalid_pattern("a", "x y", ' '),
            GroupError::EmptyPattern {
                group: "a".to_owned()
            },
            invalid_pattern("a", "a[b]", '['),
        ]
    );
}

/// Groups are reported in name order, because TOML tables are read sorted by key.
#[test]
fn error_messages_name_the_group_and_problem() {
    let messages: Vec<String> = errors(
        r#"
        d = { name = "x y" }
        e = { name = [] }
        f = {}
        "g h" = { bin = true }
        i = { name = "" }
        workspace = { bin = true }
        "#,
    )
    .iter()
    .map(ToString::to_string)
    .collect();
    insta::assert_snapshot!(messages.join("\n"), @r#"
    group `d`: name pattern `x y` contains ` `; patterns may only contain letters, digits, `-`, `_`, `*` and `?`
    group `e`: `name` is an empty list, so the group would match nothing
    group `f` has no conditions, so it would match every package; use the built-in `workspace` or `external` group instead
    group name `g h` may only contain letters, digits, `-` and `_`
    group `i`: name pattern is empty
    `workspace` is a built-in group and can't be redefined
    "#);
}

#[test]
fn unknown_keys_are_rejected() {
    insta::assert_snapshot!(shape_error(r#"domain = { name = "orders-domain", tag = "domain" }"#), @r#"
    TOML parse error at line 1, column 36
      |
    1 | domain = { name = "orders-domain", tag = "domain" }
      |                                    ^^^
    unknown field `tag`, expected one of `name`, `workspace`, `bin`
    "#);
}

#[test]
fn groups_must_be_tables() {
    insta::assert_snapshot!(shape_error(r#"domain = "orders-domain""#), @r#"
    TOML parse error at line 1, column 10
      |
    1 | domain = "orders-domain"
      |          ^^^^^^^^^^^^^^^
    invalid type: string "orders-domain", expected a table of conditions, such as `{ name = "*-tests" }`
    "#);
}

#[test]
fn flags_must_be_booleans() {
    insta::assert_snapshot!(shape_error(r#"a = { workspace = "yes" }"#), @r#"
    TOML parse error at line 1, column 19
      |
    1 | a = { workspace = "yes" }
      |                   ^^^^^
    invalid type: string "yes", expected a boolean
    "#);
    insta::assert_snapshot!(shape_error("a = { bin = 1 }"), @r#"
    TOML parse error at line 1, column 13
      |
    1 | a = { bin = 1 }
      |             ^
    invalid type: integer `1`, expected a boolean
    "#);
}

#[test]
fn name_must_be_a_glob_string_or_a_list_of_them() {
    insta::assert_snapshot!(shape_error("a = { name = 5 }"), @r#"
    TOML parse error at line 1, column 14
      |
    1 | a = { name = 5 }
      |              ^
    invalid type: integer `5`, expected a glob string or a list of them
    "#);
    insta::assert_snapshot!(shape_error(r#"a = { name = ["x", 5] }"#), @r#"
    TOML parse error at line 1, column 20
      |
    1 | a = { name = ["x", 5] }
      |                    ^
    invalid type: integer `5`, expected a glob string
    "#);
    insta::assert_snapshot!(shape_error(r#"a = { name = ["x", ["y"]] }"#), @r#"
    TOML parse error at line 1, column 20
      |
    1 | a = { name = ["x", ["y"]] }
      |                    ^^^^^
    invalid type: sequence, expected a glob string
    "#);
}

#[test]
fn table_matchers_are_not_supported() {
    insta::assert_snapshot!(shape_error(r#"a = { name = { regex = "^svc-.*$" } }"#), @r#"
    TOML parse error at line 1, column 14
      |
    1 | a = { name = { regex = "^svc-.*$" } }
      |              ^^^^^^^^^^^^^^^^^^^^^^
    table matchers like `{ regex = "..." }` aren't supported; use a glob string
    "#);
    insta::assert_snapshot!(shape_error(r#"a = { name = ["serde", { regex = "^serde_.*" }] }"#), @r#"
    TOML parse error at line 1, column 24
      |
    1 | a = { name = ["serde", { regex = "^serde_.*" }] }
      |                        ^^^^^^^^^^^^^^^^^^^^^^^
    table matchers like `{ regex = "..." }` aren't supported; use a glob string
    "#);
}
