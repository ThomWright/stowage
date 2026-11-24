//! Parsing and validating `stowage.toml` against the `features` fixture workspace, whose members
//! are `api`, `domain` and `shared`, none of which has a bin target. Its external crates are
//! `dual`, `serde` and `sqlx-core`.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test by panicking"
)]

use std::path::PathBuf;
use std::sync::LazyLock;

use stowage_rules::{Config, ConfigError, ConfigWarning, Graph};

static GRAPH: LazyLock<Graph> = LazyLock::new(|| {
    let manifest: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "features",
        "Cargo.toml",
    ]
    .iter()
    .collect();
    Graph::load_from(manifest).expect("fixture loads")
});

fn warnings(toml: &str) -> Vec<ConfigWarning> {
    Config::parse(toml, &GRAPH)
        .expect("config is valid")
        .warnings()
        .to_vec()
}

fn errors(toml: &str) -> Vec<ConfigError> {
    Config::parse(toml, &GRAPH)
        .expect_err("config is invalid")
        .into_iter()
        .collect()
}

/// The message of the single schema error in `toml`.
fn schema_error(toml: &str) -> String {
    match errors(toml).as_slice() {
        [ConfigError::Schema(error)] => error.to_string(),
        other => panic!("expected one schema error, got {other:?}"),
    }
}

fn s(value: &str) -> String {
    value.to_owned()
}

#[test]
fn valid_config_has_no_warnings() {
    let toml = r#"
        [groups]
        domain = { name = "domain", workspace = true }
        api = { name = "api", workspace = true }
        sql = { name = "sqlx-core" }
        amqp = { name = "lapin" }

        [[forbid]]
        name = "domain-avoids-infrastructure"
        from = "domain"
        to = ["sql", "amqp"]
        reach = "transitive"
        kinds = ["normal", "build"]
        reason = "The domain is tested without infrastructure."

        [[forbid]]
        name = "nothing-depends-on-api"
        from = "workspace"
        to = "api"
        reason = "The API is an entrypoint."

        [[forbid]]
        name = "libraries-avoid-sql"
        from = { not = ["api", "domain"] }
        to = "sql"
        reason = "Only the API and the domain's adapters talk to the database."
    "#;
    assert_eq!(warnings(toml), []);
}

#[test]
fn config_without_rules_is_warned_about() {
    assert_eq!(warnings(""), [ConfigWarning::NoRules]);
    assert_eq!(
        warnings(
            r#"
            [groups]
            domain = { name = "domain", workspace = true }
            "#
        ),
        [ConfigWarning::NoRules]
    );
}

// Schema errors: the config isn't TOML, or doesn't have the structure of a config. These come
// one at a time, with a location.

#[test]
fn schema_errors_point_at_the_problem() {
    insta::assert_snapshot!(schema_error("[groups"), @"
    TOML parse error at line 1, column 8
      |
    1 | [groups
      |        ^
    unclosed table, expected `]`
    ");
    insta::assert_snapshot!(schema_error("[rules]"), @"
    TOML parse error at line 1, column 2
      |
    1 | [rules]
      |  ^^^^^
    unknown field `rules`, expected `groups` or `forbid`
    ");
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
        reason = "r"
        level = "warn"
    "#), @r#"
    TOML parse error at line 7, column 9
      |
    7 |         level = "warn"
      |         ^^^^^
    unknown field `level`, expected one of `name`, `from`, `to`, `reach`, `kinds`, `reason`
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
    "#), @"
    TOML parse error at line 2, column 9
      |
    2 |         [[forbid]]
      |         ^^^^^^^^^^
    missing field `reason`
    ");
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
        reach = "indirect"
        reason = "r"
    "#), @r#"
    TOML parse error at line 6, column 17
      |
    6 |         reach = "indirect"
      |                 ^^^^^^^^^^
    unknown variant `indirect`, expected `direct` or `transitive`
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
        kinds = ["normal", "test"]
        reason = "r"
    "#), @r#"
    TOML parse error at line 6, column 28
      |
    6 |         kinds = ["normal", "test"]
      |                            ^^^^^^
    unknown variant `test`, expected one of `normal`, `build`, `dev`
    "#);
}

#[test]
fn group_reference_schema_errors_point_at_the_problem() {
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = { all = ["workspace", "domain"] }
        to = "external"
        reason = "r"
    "#), @r#"
    TOML parse error at line 4, column 16
      |
    4 |         from = { all = ["workspace", "domain"] }
      |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    the only table allowed here is `{ not = ... }`; combinators such as `any` and `all` aren't supported
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = { not = "api", unless = "domain" }
        to = "external"
        reason = "r"
    "#), @r#"
    TOML parse error at line 4, column 16
      |
    4 |         from = { not = "api", unless = "domain" }
      |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    the only table allowed here is `{ not = ... }`; combinators such as `any` and `all` aren't supported
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = {}
        to = "external"
        reason = "r"
    "#), @"
    TOML parse error at line 4, column 16
      |
    4 |         from = {}
      |                ^^
    the only table allowed here is `{ not = ... }`; combinators such as `any` and `all` aren't supported
    ");
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = { not = { not = "api" } }
        to = "external"
        reason = "r"
    "#), @r#"
    TOML parse error at line 4, column 24
      |
    4 |         from = { not = { not = "api" } }
      |                        ^^^^^^^^^^^^^^^
    `not` takes a group name or a list of them
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = { not = ["api", 5] }
        to = "external"
        reason = "r"
    "#), @r#"
    TOML parse error at line 4, column 32
      |
    4 |         from = { not = ["api", 5] }
      |                                ^
    invalid type: integer `5`, expected a group name
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = { not = "shared" }
        reason = "r"
    "#), @r#"
    TOML parse error at line 5, column 14
      |
    5 |         to = { not = "shared" }
      |              ^^^^^^^^^^^^^^^^^^
    `to` takes a group name or a list of them; negating it, as in `{ not = ... }`, would make an allow-list, and allow-lists aren't supported yet
    "#);
    insta::assert_snapshot!(schema_error(r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = ["external", 5]
        reason = "r"
    "#), @r#"
    TOML parse error at line 5, column 27
      |
    5 |         to = ["external", 5]
      |                           ^
    invalid type: integer `5`, expected a group name
    "#);
}

#[test]
fn group_schema_errors_point_at_the_problem() {
    insta::assert_snapshot!(schema_error(r#"[groups]
        domain = "orders-domain""#), @r#"
    TOML parse error at line 2, column 18
      |
    2 |         domain = "orders-domain"
      |                  ^^^^^^^^^^^^^^^
    invalid type: string "orders-domain", expected a table of conditions, such as `{ name = "*-tests" }`
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        domain = { name = "orders-domain", tag = "domain" }"#), @r#"
    TOML parse error at line 2, column 44
      |
    2 |         domain = { name = "orders-domain", tag = "domain" }
      |                                            ^^^
    unknown field `tag`, expected one of `name`, `workspace`, `bin`
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        a = { workspace = "yes" }"#), @r#"
    TOML parse error at line 2, column 27
      |
    2 |         a = { workspace = "yes" }
      |                           ^^^^^
    invalid type: string "yes", expected a boolean
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        a = { name = ["x", 5] }"#), @r#"
    TOML parse error at line 2, column 28
      |
    2 |         a = { name = ["x", 5] }
      |                            ^
    invalid type: integer `5`, expected a glob string
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        a = { name = ["x", ["y"]] }"#), @r#"
    TOML parse error at line 2, column 28
      |
    2 |         a = { name = ["x", ["y"]] }
      |                            ^^^^^
    invalid type: sequence, expected a glob string
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        a = { name = { regex = "^svc-.*$" } }"#), @r#"
    TOML parse error at line 2, column 22
      |
    2 |         a = { name = { regex = "^svc-.*$" } }
      |                      ^^^^^^^^^^^^^^^^^^^^^^
    table matchers like `{ regex = "..." }` aren't supported; use a glob string
    "#);
    insta::assert_snapshot!(schema_error(r#"[groups]
        a = { name = ["serde", { regex = "^serde_.*" }] }"#), @r#"
    TOML parse error at line 2, column 32
      |
    2 |         a = { name = ["serde", { regex = "^serde_.*" }] }
      |                                ^^^^^^^^^^^^^^^^^^^^^^^
    table matchers like `{ regex = "..." }` aren't supported; use a glob string
    "#);
}

#[test]
fn schema_errors_have_a_span() {
    let toml = "[groups]\na = { tag = \"x\" }\n";
    let errors = errors(toml);
    let [ConfigError::Schema(error)] = errors.as_slice() else {
        panic!("expected one schema error");
    };
    assert_eq!(error.span().map(|span| &toml[span]), Some("tag"));
}

// Group errors.

#[test]
fn group_names_may_only_use_letters_digits_dashes_and_underscores() {
    assert_eq!(
        errors(
            r#"
            [groups]
            "!domain" = { name = "domain" }
            "" = { name = "domain" }
            "#
        ),
        [
            ConfigError::InvalidGroupName { group: s("") },
            ConfigError::InvalidGroupName {
                group: s("!domain")
            },
        ]
    );
}

#[test]
fn built_in_group_names_are_reserved() {
    assert_eq!(
        errors(
            r#"
            [groups]
            external = { workspace = false }
            workspace = { name = "*" }
            "#
        ),
        [
            ConfigError::ReservedGroupName {
                group: s("external")
            },
            ConfigError::ReservedGroupName {
                group: s("workspace")
            },
        ]
    );
}

#[test]
fn patterns_may_only_use_crate_name_characters_and_wildcards() {
    assert_eq!(
        errors(
            r#"
            [groups]
            braces = { name = "{serde,serde_json}" }
            brackets = { name = "serde[_-]json" }
            empty = { name = "" }
            list = { name = ["x y", "", "ok"] }
            "#
        ),
        [
            ConfigError::InvalidPattern {
                group: s("braces"),
                pattern: s("{serde,serde_json}"),
                character: '{'
            },
            ConfigError::InvalidPattern {
                group: s("brackets"),
                pattern: s("serde[_-]json"),
                character: '['
            },
            ConfigError::EmptyPattern { group: s("empty") },
            ConfigError::InvalidPattern {
                group: s("list"),
                pattern: s("x y"),
                character: ' '
            },
            ConfigError::EmptyPattern { group: s("list") },
        ]
    );
}

#[test]
fn groups_that_would_match_nothing_or_everything_are_rejected() {
    assert_eq!(
        errors(
            "
            [groups]
            everything = {}
            none = { name = [] }
            "
        ),
        [
            ConfigError::GroupHasNoConditions {
                group: s("everything")
            },
            ConfigError::EmptyNameList { group: s("none") },
        ]
    );
}

#[test]
fn groups_of_members_must_match_a_member() {
    assert_eq!(
        errors(
            r#"
            [groups]
            # Unused, but selects members.
            renamed = { name = "orders-domain", workspace = true }
            # No member has a bin target.
            entrypoints = { workspace = true, bin = true }
            # Used as a source, but only matches an external crate.
            serialisation = { name = "serde" }

            [[forbid]]
            name = "r"
            from = "serialisation"
            to = "entrypoints"
            reason = "r"
            "#
        ),
        [
            ConfigError::GroupMatchesNoMembers {
                group: s("entrypoints")
            },
            ConfigError::GroupMatchesNoMembers {
                group: s("renamed")
            },
            ConfigError::GroupMatchesNoMembers {
                group: s("serialisation")
            },
        ]
    );
}

#[test]
fn groups_may_match_several_members() {
    let toml = r#"
        [groups]
        core = { name = ["domain", "shared"], workspace = true }
        everything = { name = "*", workspace = true }
        sql = { name = ["sqlx-core", "tokio-postgres"] }

        [[forbid]]
        name = "r"
        from = ["core", "everything"]
        to = "sql"
        reason = "r"
    "#;
    assert_eq!(warnings(toml), []);
}

#[test]
fn every_name_pattern_in_a_group_of_members_must_match_a_member() {
    assert_eq!(
        errors(
            r#"
            [groups]
            # A typo.
            core = { name = ["domain", "sahred"], workspace = true }
            # `dual` is an external crate, so it's never in this group.
            mixed = { name = ["domain", "dual"], workspace = true }
            # Used as a source.
            sources = { name = ["api", "ap?-*"] }

            [[forbid]]
            name = "r"
            from = "sources"
            to = "external"
            reason = "r"
            "#
        ),
        [
            ConfigError::PatternMatchesNoMembers {
                group: s("core"),
                pattern: s("sahred")
            },
            ConfigError::PatternMatchesNoMembers {
                group: s("mixed"),
                pattern: s("dual")
            },
            ConfigError::PatternMatchesNoMembers {
                group: s("sources"),
                pattern: s("ap?-*")
            },
        ]
    );
}

#[test]
fn groups_of_external_crates_may_match_nothing() {
    let toml = r#"
        [groups]
        amqp = { name = "lapin" }
        unused = { name = "tonic" }

        [[forbid]]
        name = "r"
        from = "workspace"
        to = "amqp"
        reason = "Forbidding it now stops it being added later."
    "#;
    assert_eq!(warnings(toml), []);
}

// Rule errors.

#[test]
fn rule_names_may_only_use_letters_digits_dashes_and_underscores() {
    assert_eq!(
        errors(
            r#"
            [[forbid]]
            name = "no sql"
            from = "workspace"
            to = "external"
            reason = "r"
            "#
        ),
        [ConfigError::InvalidRuleName { rule: s("no sql") }]
    );
}

#[test]
fn rule_names_must_be_unique() {
    let rule = r#"
        [[forbid]]
        name = "r"
        from = "workspace"
        to = "external"
        reason = "r"
    "#;
    assert_eq!(
        errors(&rule.repeat(3)),
        [
            ConfigError::DuplicateRuleName { rule: s("r") },
            ConfigError::DuplicateRuleName { rule: s("r") },
        ]
    );
}

#[test]
fn rules_must_refer_to_defined_groups() {
    assert_eq!(
        errors(
            r#"
            [[forbid]]
            name = "a"
            from = "domian"
            to = ["external", "sql"]
            reason = "r"

            [[forbid]]
            name = "b"
            from = { not = "missing" }
            to = ""
            reason = "r"
            "#
        ),
        [
            ConfigError::UnknownGroup {
                rule: s("a"),
                group: s("domian")
            },
            ConfigError::UnknownGroup {
                rule: s("a"),
                group: s("sql")
            },
            ConfigError::UnknownGroup {
                rule: s("b"),
                group: s("missing")
            },
            ConfigError::UnknownGroup {
                rule: s("b"),
                group: s("")
            },
        ]
    );
}

#[test]
fn prefix_negation_points_to_not() {
    assert_eq!(
        errors(
            r#"
            [[forbid]]
            name = "r"
            from = "!api"
            to = ["external", "!domain"]
            reason = "r"
            "#
        ),
        [
            ConfigError::PrefixNegation {
                rule: s("r"),
                key: "from",
                group: s("api")
            },
            ConfigError::PrefixNegation {
                rule: s("r"),
                key: "to",
                group: s("domain")
            },
        ]
    );
}

#[test]
fn rules_must_not_have_empty_lists_or_reasons() {
    assert_eq!(
        errors(
            r#"
            [[forbid]]
            name = "a"
            from = []
            to = []
            kinds = []
            reason = ""

            [[forbid]]
            name = "b"
            from = "workspace"
            to = "external"
            reason = "  "

            [[forbid]]
            name = "c"
            from = { not = [] }
            to = "external"
            reason = "r"
            "#
        ),
        [
            ConfigError::EmptyGroupList {
                rule: s("a"),
                key: "from"
            },
            ConfigError::EmptyGroupList {
                rule: s("a"),
                key: "to"
            },
            ConfigError::EmptyKinds { rule: s("a") },
            ConfigError::EmptyReason { rule: s("a") },
            ConfigError::EmptyReason { rule: s("b") },
            ConfigError::EmptyGroupList {
                rule: s("c"),
                key: "from"
            },
        ]
    );
}

#[test]
fn rules_using_an_invalid_group_get_no_extra_error() {
    assert_eq!(
        errors(
            r#"
            [groups]
            bad = { name = "x y" }

            [[forbid]]
            name = "r"
            from = "workspace"
            to = "bad"
            reason = "r"
            "#
        ),
        [ConfigError::InvalidPattern {
            group: s("bad"),
            pattern: s("x y"),
            character: ' '
        }]
    );
}

/// Group errors come first, in group name order, then rule errors in file order.
#[test]
fn every_error_is_reported() {
    assert_eq!(
        errors(
            r#"
            [groups]
            z = {}
            m = { name = "missing", workspace = true }
            a = { name = "" }

            [[forbid]]
            name = "second"
            from = "missing"
            to = "external"
            reason = ""

            [[forbid]]
            name = "first"
            from = "workspace"
            to = "external"
            kinds = []
            reason = "r"
            "#
        ),
        [
            ConfigError::EmptyPattern { group: s("a") },
            ConfigError::GroupMatchesNoMembers { group: s("m") },
            ConfigError::GroupHasNoConditions { group: s("z") },
            ConfigError::UnknownGroup {
                rule: s("second"),
                group: s("missing")
            },
            ConfigError::EmptyReason { rule: s("second") },
            ConfigError::EmptyKinds { rule: s("first") },
        ]
    );
}

// Warnings.

#[test]
fn negated_group_matching_nothing_is_warned_about() {
    let toml = r#"
        [groups]
        harnesses = { name = "*-tests" }

        [[forbid]]
        name = "r"
        from = { not = "harnesses" }
        to = "workspace"
        reason = "r"
    "#;
    assert_eq!(
        warnings(toml),
        [ConfigWarning::NegatedGroupMatchesNothing {
            group: s("harnesses")
        }]
    );
}

#[test]
fn group_also_used_positively_is_not_warned_about() {
    // `to` groups may match nothing, so this group is fine.
    let toml = r#"
        [groups]
        harnesses = { name = "*-tests" }

        [[forbid]]
        name = "r"
        from = { not = "harnesses" }
        to = "harnesses"
        reason = "r"
    "#;
    assert_eq!(warnings(toml), []);
}

#[test]
fn messages_name_the_group_or_rule_and_the_problem() {
    let invalid = Config::parse(
        r#"
        [groups]
        "g h" = { bin = true }
        workspace = { bin = true }
        d = { name = "x y" }
        e = { name = "" }
        f = { name = [] }
        g = {}
        renamed = { name = "orders-domain", workspace = true }
        typo = { name = ["domain", "sahred"], workspace = true }

        [[forbid]]
        name = "no sql"
        from = []
        to = "missing"
        kinds = []
        reason = " "

        [[forbid]]
        name = "no sql"
        from = "workspace"
        to = "external"
        reason = "r"

        [[forbid]]
        name = "negated"
        from = "!api"
        to = "external"
        reason = "r"
        "#,
        &GRAPH,
    )
    .expect_err("config is invalid");
    insta::assert_snapshot!(invalid.to_string(), @r#"
    group `d`: name pattern `x y` contains ` `; patterns may only contain letters, digits, `-`, `_`, `*` and `?`
    group `e`: name pattern is empty
    group `f`: `name` is an empty list, so the group would match nothing
    group `g` has no conditions, so it would match every package; use the built-in `workspace` or `external` group instead
    group name `g h` may only contain letters, digits, `-` and `_`
    group `renamed` matches no workspace member, so rules using it would check nothing; was a crate renamed?
    group `typo`: name pattern `sahred` matches no workspace member in the group; is it misspelt?
    `workspace` is a built-in group and can't be redefined
    rule name `no sql` may only contain letters, digits, `-` and `_`
    rule `no sql`: `from` is an empty list, so the rule would check nothing
    rule `no sql` refers to unknown group `missing`
    rule `no sql`: `kinds` is empty, so the rule would check nothing
    rule `no sql` has an empty `reason`; say why the dependency is forbidden
    rule name `no sql` may only contain letters, digits, `-` and `_`
    rule name `no sql` is used by more than one rule
    rule `negated`: `from` refers to `!api`, but a `!` prefix doesn't negate a group; write `{ not = "api" }` instead (only `from` supports negation)
    "#);

    let negated = r#"
        [groups]
        harnesses = { name = "*-tests" }

        [[forbid]]
        name = "r"
        from = { not = "harnesses" }
        to = "workspace"
        reason = "r"
    "#;
    let messages: Vec<String> = [warnings(negated), warnings("")]
        .concat()
        .iter()
        .map(ToString::to_string)
        .collect();
    insta::assert_snapshot!(messages.join("\n"), @"
    group `harnesses` matches no package, so negating it in `from` excludes nothing; was a crate renamed?
    the config has no `[[forbid]]` rules, so it checks nothing
    ");
}
