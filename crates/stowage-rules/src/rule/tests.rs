use std::collections::BTreeSet;

use super::{PathKind, Reach, Rule, RuleDef, Sources};

fn rule(toml: &str) -> Rule {
    let def: RuleDef = toml::from_str(toml).expect("rule has the right shape");
    Rule::new(def, |_| true, &mut BTreeSet::new()).expect("rule is valid")
}

fn sources(groups: &[&str], negated: bool) -> Sources {
    Sources {
        groups: groups.iter().map(|&group| group.to_owned()).collect(),
        negated,
    }
}

#[test]
fn name_and_reason_are_kept() {
    let rule = rule(
        r#"
        name = "domain-is-pure"
        from = "a"
        to = "b"
        reason = "The domain is tested without infrastructure."
        "#,
    );
    assert_eq!(rule.name, "domain-is-pure");
    assert_eq!(rule.reason, "The domain is tested without infrastructure.");
}

#[test]
fn reach_and_kinds_have_defaults() {
    let rule = rule(
        r#"
        name = "r"
        from = "a"
        to = "b"
        reason = "r"
        "#,
    );
    assert_eq!(rule.reach, Reach::Direct);
    assert_eq!(
        rule.kinds,
        BTreeSet::from([PathKind::Normal, PathKind::Build, PathKind::Dev])
    );
}

#[test]
fn reach_and_kinds_can_be_set() {
    let rule = rule(
        r#"
        name = "r"
        from = "a"
        to = "b"
        reach = "transitive"
        kinds = ["dev", "normal", "dev"]
        reason = "r"
        "#,
    );
    assert_eq!(rule.reach, Reach::Transitive);
    assert_eq!(
        rule.kinds,
        BTreeSet::from([PathKind::Normal, PathKind::Dev])
    );
}

#[test]
fn sources_may_be_one_group_or_a_list() {
    let one = rule(
        r#"
        name = "r"
        from = "domain"
        to = "b"
        reason = "r"
        "#,
    );
    assert_eq!(one.from, sources(&["domain"], false));
    let list = rule(
        r#"
        name = "r"
        from = ["domain", "shared"]
        to = "b"
        reason = "r"
        "#,
    );
    assert_eq!(list.from, sources(&["domain", "shared"], false));
}

#[test]
fn sources_may_be_negated() {
    let one = rule(
        r#"
        name = "r"
        from = { not = "harnesses" }
        to = "b"
        reason = "r"
        "#,
    );
    assert_eq!(one.from, sources(&["harnesses"], true));
    let list = rule(
        r#"
        name = "r"
        from = { not = ["api", "harnesses"] }
        to = "b"
        reason = "r"
        "#,
    );
    assert_eq!(list.from, sources(&["api", "harnesses"], true));
}

#[test]
fn targets_keep_each_group() {
    let rule = rule(
        r#"
        name = "r"
        from = "a"
        to = ["sql", "amqp"]
        reason = "r"
        "#,
    );
    assert_eq!(rule.to, ["sql", "amqp"]);
}
