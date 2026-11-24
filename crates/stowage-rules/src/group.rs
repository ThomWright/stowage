//! Groups: named selectors over the packages in a graph.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use wildmatch::WildMatch;

use crate::config::ConfigError;
use crate::de::{self, Expected};
use crate::graph::Package;

/// The names of the built-in groups, which can't be redefined.
pub(crate) const BUILT_IN: [&str; 2] = [WORKSPACE, "external"];

/// The built-in group of every workspace member.
const WORKSPACE: &str = "workspace";

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
    #[serde(default, deserialize_with = "names")]
    name: Option<Vec<String>>,
    workspace: Option<bool>,
    bin: Option<bool>,
}

fn names<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<String>>, D::Error> {
    de::strings(
        deserializer,
        Expected {
            item: "a glob string",
            value: "a glob string or a list of them",
            table: "table matchers like `{ regex = \"...\" }` aren't supported; use a glob string",
        },
    )
    .map(Some)
}

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
struct NamePattern {
    /// The pattern as written.
    pattern: String,
    glob: WildMatch,
}

/// The groups defined in a config's `[groups]` table, plus the built-in groups: `workspace`
/// (every workspace member) and `external` (every other package).
#[derive(Debug)]
pub(crate) struct Groups(BTreeMap<String, Group>);

impl Groups {
    /// Validates the definitions from a `[groups]` table, keyed by group name.
    ///
    /// Returns the valid groups, plus an error for each problem in the others, ordered by group
    /// name.
    pub(crate) fn new(defs: BTreeMap<String, GroupDef>) -> (Self, Vec<ConfigError>) {
        let mut groups = BTreeMap::from([
            (WORKSPACE.to_owned(), Group::members(true)),
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
        (Self(groups), errors)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "nothing looks groups up until the checker exists")
    )]
    pub(crate) fn get(&self, name: &str) -> Option<&Group> {
        self.0.get(name)
    }

    /// Every group, ordered by name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &Group)> {
        self.0.iter().map(|(name, group)| (name.as_str(), group))
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

    fn new(name: &str, def: GroupDef) -> Result<Self, Vec<ConfigError>> {
        let group = || name.to_owned();
        if !is_valid_name(name) {
            return Err(vec![ConfigError::InvalidGroupName { group: group() }]);
        }
        if BUILT_IN.contains(&name) {
            return Err(vec![ConfigError::ReservedGroupName { group: group() }]);
        }
        let GroupDef {
            name: names,
            workspace,
            bin,
        } = def;
        if names.is_none() && workspace.is_none() && bin.is_none() {
            return Err(vec![ConfigError::GroupHasNoConditions { group: group() }]);
        }
        let names = names
            .map(|patterns| {
                if patterns.is_empty() {
                    return Err(vec![ConfigError::EmptyNameList { group: group() }]);
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
        self.names
            .as_ref()
            .is_none_or(|names| names.iter().any(|pattern| pattern.matches(package)))
            && self.matches_except_names(package)
    }

    fn matches_except_names(&self, package: &Package) -> bool {
        self.workspace
            .is_none_or(|workspace| workspace == package.workspace_member)
            && self.bin.is_none_or(|bin| bin == package.has_bin)
    }

    /// The group's name patterns, as written, that match none of `packages` that meet the group's
    /// other conditions. Such a pattern has no effect on the group.
    pub(crate) fn unmatched_patterns(&self, packages: &[&Package]) -> Vec<&str> {
        self.names
            .iter()
            .flatten()
            .filter(|pattern| {
                !packages
                    .iter()
                    .any(|package| pattern.matches(package) && self.matches_except_names(package))
            })
            .map(|pattern| pattern.pattern.as_str())
            .collect()
    }

    /// Whether every package in the group is a workspace member.
    pub(crate) fn selects_members(&self) -> bool {
        self.workspace == Some(true)
    }
}

/// Whether `name` is allowed as the name of a group or rule: letters, digits, `-` and `_`.
pub(crate) fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

impl NamePattern {
    /// Rejects characters that can't appear in a crate name, so that syntax such as `[abc]` or
    /// `{a,b}` fails loudly instead of silently matching nothing. It also means adding that syntax
    /// later can't change what an existing pattern matches.
    fn parse(group: &str, pattern: &str) -> Result<Self, ConfigError> {
        if pattern.is_empty() {
            return Err(ConfigError::EmptyPattern {
                group: group.to_owned(),
            });
        }
        if let Some(character) = pattern
            .chars()
            .find(|&c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '*' | '?')))
        {
            return Err(ConfigError::InvalidPattern {
                group: group.to_owned(),
                pattern: pattern.to_owned(),
                character,
            });
        }
        Ok(Self {
            pattern: pattern.to_owned(),
            glob: WildMatch::new(&normalise(pattern)),
        })
    }

    fn matches(&self, package: &Package) -> bool {
        self.glob.matches(&normalise(&package.name))
    }
}

/// The form of a crate name used for comparison, as described on [`NamePattern`].
fn normalise(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}
