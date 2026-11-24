//! Forbid rules: dependencies that crates in one set of groups may not have on crates in another.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::config::ConfigError;
use crate::de::{self, Expected, Strings};
use crate::group::is_valid_name;

/// A rule as written in a `[[forbid]]` table.
///
/// Deserialising checks only the shape of the rule. [`Rule::new`] checks that it makes sense.
#[derive(Debug, Deserialize)]
#[serde(
    deny_unknown_fields,
    expecting = "a table with `name`, `from`, `to` and `reason`"
)]
pub(crate) struct RuleDef {
    name: String,
    from: Sources,
    #[serde(deserialize_with = "targets")]
    to: Vec<String>,
    #[serde(default)]
    reach: Reach,
    #[serde(default = "all_kinds")]
    kinds: BTreeSet<PathKind>,
    reason: String,
}

/// No crate in `from` may depend on a crate in `to`.
#[derive(Debug)]
pub(crate) struct Rule {
    /// Identifies the rule in errors.
    pub(crate) name: String,
    /// The source crates, which are always workspace members.
    pub(crate) from: Sources,
    /// The groups of forbidden targets: a package in any of them is forbidden. Each group is kept
    /// separate, so a violation can name the group it reached.
    pub(crate) to: Vec<String>,
    pub(crate) reach: Reach,
    /// The path kinds that count as a violation.
    pub(crate) kinds: BTreeSet<PathKind>,
    /// Why the dependency is forbidden, shown with every violation.
    pub(crate) reason: String,
}

/// The source crates of a rule: the workspace members in any of `groups`, or, if `negated`, the
/// workspace members in none of them.
///
/// Written as `"group"`, `["a", "b"]`, `{ not = "group" }` or `{ not = ["a", "b"] }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sources {
    pub(crate) groups: Vec<String>,
    pub(crate) negated: bool,
}

/// How far a rule looks from each source crate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Reach {
    /// Only the source crate's own dependencies.
    #[default]
    Direct,
    /// Anything the source crate depends on, through any number of crates.
    Transitive,
}

/// How a target is built for the source crate, judged by the path from one to the other.
///
/// Ordered from most to least serious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    /// Every edge on the path is a normal dependency, so the target is linked into the source
    /// crate.
    Normal,
    /// The path doesn't start with a dev edge, and has at least one build edge, so the target
    /// only runs at build time.
    Build,
    /// The path starts with a dev edge, so the target only exists in the source crate's tests,
    /// examples and benches.
    Dev,
}

fn all_kinds() -> BTreeSet<PathKind> {
    BTreeSet::from([PathKind::Normal, PathKind::Build, PathKind::Dev])
}

const GROUPS: Expected = Expected {
    item: "a group name",
    value: "a group name or a list of them",
    table: "`not` takes a group name or a list of them",
};

fn targets<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    de::strings(
        deserializer,
        Expected {
            table: "`to` takes a group name or a list of them; negating it, as in `{ not = ... }`, would make an allow-list, and allow-lists aren't supported yet",
            ..GROUPS
        },
    )
}

impl<'de> Deserialize<'de> for Sources {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SourcesVisitor)
    }
}

struct SourcesVisitor;

impl SourcesVisitor {
    fn plain(groups: Vec<String>) -> Sources {
        Sources {
            groups,
            negated: false,
        }
    }
}

impl<'de> Visitor<'de> for SourcesVisitor {
    type Value = Sources;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a group name, a list of them, or `{ not = ... }`")
    }

    fn visit_str<E: Error>(self, group: &str) -> Result<Sources, E> {
        Ok(Self::plain(vec![group.to_owned()]))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Sources, A::Error> {
        let groups = de::strings(serde::de::value::SeqAccessDeserializer::new(seq), GROUPS)?;
        Ok(Self::plain(groups))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Sources, A::Error> {
        const ONLY_NOT: &str = "the only table allowed here is `{ not = ... }`; combinators such as `any` and `all` aren't supported";
        match map.next_key::<String>()?.as_deref() {
            Some("not") => {}
            Some(_) | None => return Err(Error::custom(ONLY_NOT)),
        }
        let groups = map.next_value_seed(Strings(GROUPS))?;
        if map.next_key::<String>()?.is_some() {
            return Err(Error::custom(ONLY_NOT));
        }
        Ok(Sources {
            groups,
            negated: true,
        })
    }
}

impl RuleDef {
    pub(crate) fn from(&self) -> &Sources {
        &self.from
    }

    pub(crate) fn to(&self) -> &[String] {
        &self.to
    }
}

impl Rule {
    /// Validates a rule.
    ///
    /// `is_defined` says whether a group name was defined, even if its definition was invalid,
    /// so that an invalid group doesn't also cause an unknown-group error. `seen` holds the names
    /// of the rules before this one, and this rule's name is added to it.
    ///
    /// # Errors
    ///
    /// Returns every problem found.
    pub(crate) fn new(
        def: RuleDef,
        is_defined: impl Fn(&str) -> bool,
        seen: &mut BTreeSet<String>,
    ) -> Result<Self, Vec<ConfigError>> {
        let RuleDef {
            name,
            from,
            to,
            reach,
            kinds,
            reason,
        } = def;
        let rule = || name.clone();
        let mut errors = Vec::new();
        if !is_valid_name(&name) {
            errors.push(ConfigError::InvalidRuleName { rule: rule() });
        }
        if !seen.insert(name.clone()) {
            errors.push(ConfigError::DuplicateRuleName { rule: rule() });
        }
        for (key, groups) in [("from", &from.groups), ("to", &to)] {
            if groups.is_empty() {
                errors.push(ConfigError::EmptyGroupList { rule: rule(), key });
            }
            for group in groups {
                // Group names can't contain `!`, so this is someone negating the way other tools
                // do, and deserves a pointer to `not`.
                if let Some(negated) = group.strip_prefix('!') {
                    errors.push(ConfigError::PrefixNegation {
                        rule: rule(),
                        key,
                        group: negated.to_owned(),
                    });
                } else if !is_defined(group) {
                    errors.push(ConfigError::UnknownGroup {
                        rule: rule(),
                        group: group.clone(),
                    });
                }
            }
        }
        if kinds.is_empty() {
            errors.push(ConfigError::EmptyKinds { rule: rule() });
        }
        if reason.trim().is_empty() {
            errors.push(ConfigError::EmptyReason { rule: rule() });
        }
        if errors.is_empty() {
            Ok(Self {
                name,
                from,
                to,
                reach,
                kinds,
                reason,
            })
        } else {
            Err(errors)
        }
    }
}
