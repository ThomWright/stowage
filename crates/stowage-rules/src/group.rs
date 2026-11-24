//! Groups: named selectors over the packages in a graph.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside the tests uses groups until rules exist"
    )
)]

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use wildmatch::WildMatch;

use crate::graph::Package;

const BUILT_IN: [&str; 2] = ["workspace", "external"];

/// A group as written in the config's `[groups]` table, such as
/// `harnesses = { name = "*-tests", workspace = true }`.
///
/// Deserialising checks only the shape of the definition. [`Groups::new`] checks that it makes
/// sense.
#[derive(Debug, Deserialize)]
#[serde(
    deny_unknown_fields,
    expecting = "a table of conditions, such as `{ name = \"*-tests\" }`"
)]
pub(crate) struct GroupDef {
    name: Option<NameList>,
    workspace: Option<bool>,
    bin: Option<bool>,
}

/// The value of a group's `name` key: one pattern, or a list of them.
#[derive(Debug)]
struct NameList(Vec<String>);

/// A named selector over packages. A package is in the group if it meets all of the group's
/// conditions.
#[derive(Debug)]
pub(crate) struct Group {
    /// The package's name matches at least one of these.
    names: Option<Vec<NamePattern>>,
    /// The package is, or isn't, a workspace member.
    workspace: Option<bool>,
    /// The package has, or doesn't have, a bin target.
    bin: Option<bool>,
}

/// A glob over crate names: `*` matches any run of characters, and `?` matches exactly one.
///
/// Names are compared the way crates.io decides whether two names are the same crate: ignoring
/// case, and treating `-` and `_` as the same. So `serde_json` matches `serde-json`, and
/// `*-tests` matches `orders_tests`.
#[derive(Debug)]
struct NamePattern(WildMatch);

/// The groups defined in a config's `[groups]` table, plus the built-in groups: `workspace`
/// (every workspace member) and `external` (every other package).
#[derive(Debug)]
pub(crate) struct Groups(BTreeMap<String, Group>);

/// A group definition that has the right shape but doesn't make sense.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GroupError {
    #[error("group name `{group}` may only contain letters, digits, `-` and `_`")]
    InvalidName { group: String },

    #[error("`{group}` is a built-in group and can't be redefined")]
    ReservedName { group: String },

    #[error(
        "group `{group}`: name pattern `{pattern}` contains `{character}`; patterns may only contain letters, digits, `-`, `_`, `*` and `?`"
    )]
    InvalidPattern {
        group: String,
        pattern: String,
        character: char,
    },

    #[error("group `{group}`: name pattern is empty")]
    EmptyPattern { group: String },

    #[error("group `{group}`: `name` is an empty list, so the group would match nothing")]
    EmptyNameList { group: String },

    #[error(
        "group `{group}` has no conditions, so it would match every package; use the built-in `workspace` or `external` group instead"
    )]
    NoConditions { group: String },
}

impl Groups {
    /// Validates the definitions from a `[groups]` table, keyed by group name.
    ///
    /// # Errors
    ///
    /// Returns every problem found, ordered by group name.
    pub(crate) fn new(defs: BTreeMap<String, GroupDef>) -> Result<Self, Vec<GroupError>> {
        let mut groups = BTreeMap::from([
            (BUILT_IN[0].to_owned(), Group::members(true)),
            (BUILT_IN[1].to_owned(), Group::members(false)),
        ]);
        let mut errors = Vec::new();
        for (name, def) in defs {
            match Group::new(&name, def) {
                Ok(group) => {
                    groups.insert(name, group);
                }
                Err(mut group_errors) => errors.append(&mut group_errors),
            }
        }
        if errors.is_empty() {
            Ok(Self(groups))
        } else {
            Err(errors)
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Group> {
        self.0.get(name)
    }
}

impl Group {
    /// Selects workspace members, or every package that isn't one.
    fn members(workspace: bool) -> Self {
        Self {
            names: None,
            workspace: Some(workspace),
            bin: None,
        }
    }

    fn new(name: &str, def: GroupDef) -> Result<Self, Vec<GroupError>> {
        let group = || name.to_owned();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(vec![GroupError::InvalidName { group: group() }]);
        }
        if BUILT_IN.contains(&name) {
            return Err(vec![GroupError::ReservedName { group: group() }]);
        }
        let GroupDef {
            name: names,
            workspace,
            bin,
        } = def;
        if names.is_none() && workspace.is_none() && bin.is_none() {
            return Err(vec![GroupError::NoConditions { group: group() }]);
        }
        let names = names
            .map(|NameList(patterns)| {
                if patterns.is_empty() {
                    return Err(vec![GroupError::EmptyNameList { group: group() }]);
                }
                let mut parsed = Vec::new();
                let mut errors = Vec::new();
                for pattern in &patterns {
                    match NamePattern::parse(name, pattern) {
                        Ok(pattern) => parsed.push(pattern),
                        Err(error) => errors.push(error),
                    }
                }
                if errors.is_empty() {
                    Ok(parsed)
                } else {
                    Err(errors)
                }
            })
            .transpose()?;
        Ok(Self {
            names,
            workspace,
            bin,
        })
    }

    /// Whether `package` meets all of this group's conditions.
    pub(crate) fn matches(&self, package: &Package) -> bool {
        self.names.as_ref().is_none_or(|names| {
            let name = normalise(&package.name);
            names.iter().any(|pattern| pattern.0.matches(&name))
        }) && self
            .workspace
            .is_none_or(|workspace| workspace == package.workspace_member)
            && self.bin.is_none_or(|bin| bin == package.has_bin)
    }
}

// Hand-written visitors rather than an untagged enum, so that a table gets an error saying table
// matchers aren't supported, instead of "data did not match any variant of untagged enum".

impl<'de> Deserialize<'de> for NameList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NameListVisitor;

        impl<'de> Visitor<'de> for NameListVisitor {
            type Value = NameList;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a glob string or a list of them")
            }

            fn visit_str<E: de::Error>(self, pattern: &str) -> Result<NameList, E> {
                Ok(NameList(vec![pattern.to_owned()]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<NameList, A::Error> {
                let mut patterns = Vec::new();
                while let Some(ListedName(pattern)) = seq.next_element()? {
                    patterns.push(pattern);
                }
                Ok(NameList(patterns))
            }

            fn visit_map<A: MapAccess<'de>>(self, _map: A) -> Result<NameList, A::Error> {
                Err(unsupported_matcher())
            }
        }

        deserializer.deserialize_any(NameListVisitor)
    }
}

/// One pattern inside a `name` list.
struct ListedName(String);

impl<'de> Deserialize<'de> for ListedName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ListedNameVisitor;

        impl<'de> Visitor<'de> for ListedNameVisitor {
            type Value = ListedName;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a glob string")
            }

            fn visit_str<E: de::Error>(self, pattern: &str) -> Result<ListedName, E> {
                Ok(ListedName(pattern.to_owned()))
            }

            fn visit_map<A: MapAccess<'de>>(self, _map: A) -> Result<ListedName, A::Error> {
                Err(unsupported_matcher())
            }
        }

        deserializer.deserialize_any(ListedNameVisitor)
    }
}

fn unsupported_matcher<E: de::Error>() -> E {
    E::custom("table matchers like `{ regex = \"...\" }` aren't supported; use a glob string")
}

impl NamePattern {
    /// Rejects characters that can't appear in a crate name, so that syntax such as `[abc]` or
    /// `{a,b}` fails loudly instead of silently matching nothing. It also means adding that syntax
    /// later can't change what an existing pattern matches.
    fn parse(group: &str, pattern: &str) -> Result<Self, GroupError> {
        if pattern.is_empty() {
            return Err(GroupError::EmptyPattern {
                group: group.to_owned(),
            });
        }
        if let Some(character) = pattern
            .chars()
            .find(|&c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '*' | '?')))
        {
            return Err(GroupError::InvalidPattern {
                group: group.to_owned(),
                pattern: pattern.to_owned(),
                character,
            });
        }
        Ok(Self(WildMatch::new(&normalise(pattern))))
    }
}

/// The form of a crate name used for comparison, as described on [`NamePattern`].
fn normalise(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}
