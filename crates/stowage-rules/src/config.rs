//! A Stowage config (`stowage.toml`): its groups and rules, validated against a workspace.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use serde::Deserialize;

use crate::graph::Graph;
use crate::group::{BUILT_IN, GroupDef, Groups};
use crate::rule::{Rule, RuleDef};

/// The groups and rules of a config, validated against a workspace.
#[derive(Debug)]
pub struct Config {
    #[expect(dead_code, reason = "nothing reads groups until the checker exists")]
    groups: Groups,
    #[expect(dead_code, reason = "nothing reads rules until the checker exists")]
    rules: Vec<Rule>,
    warnings: Vec<ConfigWarning>,
}

/// A config file as written, before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigDef {
    #[serde(default)]
    groups: BTreeMap<String, GroupDef>,
    #[serde(default)]
    forbid: Vec<RuleDef>,
}

/// Every problem found in a config, one per line when displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigErrors(Vec<ConfigError>);

/// A problem with a config that stops it being used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The config isn't valid TOML, or doesn't have the structure of a config: an unknown key, a
    /// value of the wrong type, or a missing required key.
    #[error(transparent)]
    Schema(SchemaError),

    /// A group name contains something other than letters, digits, `-` and `_`.
    #[error("group name `{group}` may only contain letters, digits, `-` and `_`")]
    InvalidGroupName {
        /// The group's name.
        group: String,
    },

    /// A group has the name of a built-in group.
    #[error("`{group}` is a built-in group and can't be redefined")]
    ReservedGroupName {
        /// The group's name.
        group: String,
    },

    /// A name pattern contains a character that can't appear in a crate name and isn't a
    /// wildcard.
    #[error(
        "group `{group}`: name pattern `{pattern}` contains `{character}`; patterns may only contain letters, digits, `-`, `_`, `*` and `?`"
    )]
    InvalidPattern {
        /// The group's name.
        group: String,
        /// The pattern as written.
        pattern: String,
        /// The first character not allowed.
        character: char,
    },

    /// A name pattern is an empty string.
    #[error("group `{group}`: name pattern is empty")]
    EmptyPattern {
        /// The group's name.
        group: String,
    },

    /// A group's `name` is an empty list, so the group would match nothing.
    #[error("group `{group}`: `name` is an empty list, so the group would match nothing")]
    EmptyNameList {
        /// The group's name.
        group: String,
    },

    /// A group has no conditions, so it would match every package.
    #[error(
        "group `{group}` has no conditions, so it would match every package; use the built-in `workspace` or `external` group instead"
    )]
    GroupHasNoConditions {
        /// The group's name.
        group: String,
    },

    /// A group that selects workspace members matches none. A group selects members if it has
    /// `workspace = true`, or a rule uses it in `from`.
    #[error(
        "group `{group}` matches no workspace member, so rules using it would check nothing; was a crate renamed?"
    )]
    GroupMatchesNoMembers {
        /// The group's name.
        group: String,
    },

    /// A name pattern in a group that selects workspace members matches none of the members that
    /// meet the group's other conditions, so it has no effect. It's most likely misspelt. Reported
    /// only when the group as a whole matches at least one member.
    #[error(
        "group `{group}`: name pattern `{pattern}` matches no workspace member in the group; is it misspelt?"
    )]
    PatternMatchesNoMembers {
        /// The group's name.
        group: String,
        /// The pattern as written.
        pattern: String,
    },

    /// A rule name contains something other than letters, digits, `-` and `_`.
    #[error("rule name `{rule}` may only contain letters, digits, `-` and `_`")]
    InvalidRuleName {
        /// The rule's name.
        rule: String,
    },

    /// Two rules have the same name. Reported for each rule after the first.
    #[error("rule name `{rule}` is used by more than one rule")]
    DuplicateRuleName {
        /// The rule's name.
        rule: String,
    },

    /// A rule refers to a group that isn't defined.
    #[error("rule `{rule}` refers to unknown group `{group}`")]
    UnknownGroup {
        /// The rule's name.
        rule: String,
        /// The group name as referred to.
        group: String,
    },

    /// A rule refers to `!group`, as other tools write negation. Stowage negates with
    /// `{ not = ... }`, and only in `from`.
    #[error(
        "rule `{rule}`: `{key}` refers to `!{group}`, but a `!` prefix doesn't negate a group; write `{{ not = \"{group}\" }}` instead (only `from` supports negation)"
    )]
    PrefixNegation {
        /// The rule's name.
        rule: String,
        /// `from` or `to`.
        key: &'static str,
        /// The group name after the `!`.
        group: String,
    },

    /// A rule's `from` or `to` is an empty list, so the rule would check nothing.
    #[error("rule `{rule}`: `{key}` is an empty list, so the rule would check nothing")]
    EmptyGroupList {
        /// The rule's name.
        rule: String,
        /// `from` or `to`.
        key: &'static str,
    },

    /// A rule's `kinds` is empty, so the rule would check nothing.
    #[error("rule `{rule}`: `kinds` is empty, so the rule would check nothing")]
    EmptyKinds {
        /// The rule's name.
        rule: String,
    },

    /// A rule's `reason` is empty or only whitespace.
    #[error("rule `{rule}` has an empty `reason`; say why the dependency is forbidden")]
    EmptyReason {
        /// The rule's name.
        rule: String,
    },
}

/// The config isn't valid TOML, or doesn't have the structure of a config.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(transparent)]
pub struct SchemaError(toml::de::Error);

/// Something in a config that is probably a mistake, but doesn't stop it being used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigWarning {
    /// A group is used only inside `from = { not = ... }` and matches no package, so negating it
    /// excludes nothing.
    #[error(
        "group `{group}` matches no package, so negating it in `from` excludes nothing; was a crate renamed?"
    )]
    NegatedGroupMatchesNothing {
        /// The group's name.
        group: String,
    },

    /// The config has no rules, so it checks nothing.
    #[error("the config has no `[[forbid]]` rules, so it checks nothing")]
    NoRules,
}

impl Config {
    /// Parses a config and validates it against `graph`.
    ///
    /// # Errors
    ///
    /// If the config isn't valid TOML or doesn't have the structure of a config, returns that one
    /// error. Otherwise returns every problem found: group errors first, ordered by group name,
    /// then rule errors in the order the rules are written.
    pub fn parse(source: &str, graph: &Graph) -> Result<Self, ConfigErrors> {
        let ConfigDef { groups, forbid } = toml::from_str(source)
            .map_err(|error| ConfigErrors(vec![ConfigError::Schema(SchemaError(error))]))?;

        // Invalid groups still count as defined, so rules using them don't get a second error.
        let defined: BTreeSet<String> = groups
            .keys()
            .cloned()
            .chain(BUILT_IN.map(str::to_owned))
            .collect();
        let references = References::of(&forbid);
        let (groups, mut errors) = Groups::new(groups);
        errors.extend(member_errors(&groups, &references, graph));
        // Stable, so each group's own errors stay in order.
        errors.sort_by(|a, b| a.group().cmp(&b.group()));

        let mut rules = Vec::new();
        let mut seen = BTreeSet::new();
        for def in forbid {
            match Rule::new(def, |group| defined.contains(group), &mut seen) {
                Ok(rule) => rules.push(rule),
                Err(mut rule_errors) => errors.append(&mut rule_errors),
            }
        }
        if !errors.is_empty() {
            return Err(ConfigErrors(errors));
        }

        let warnings = warnings(&groups, &rules, &references, graph);
        Ok(Self {
            groups,
            rules,
            warnings,
        })
    }

    /// Things in the config that are probably mistakes.
    pub fn warnings(&self) -> &[ConfigWarning] {
        &self.warnings
    }
}

/// The group names used by a config's rules.
struct References {
    /// Used in a `from` that isn't negated.
    sources: BTreeSet<String>,
    /// Used anywhere other than a negated `from`.
    positive: BTreeSet<String>,
    /// Used in a negated `from`.
    negated: BTreeSet<String>,
}

impl References {
    fn of(rules: &[RuleDef]) -> Self {
        let mut references = Self {
            sources: BTreeSet::new(),
            positive: BTreeSet::new(),
            negated: BTreeSet::new(),
        };
        for rule in rules {
            let from = rule.from();
            if from.negated {
                references.negated.extend(from.groups.iter().cloned());
            } else {
                references.sources.extend(from.groups.iter().cloned());
                references.positive.extend(from.groups.iter().cloned());
            }
            references.positive.extend(rule.to().iter().cloned());
        }
        references
    }
}

/// Errors for groups that select workspace members but match none, or have name patterns that
/// match none, as described on [`ConfigError::GroupMatchesNoMembers`] and
/// [`ConfigError::PatternMatchesNoMembers`].
fn member_errors(groups: &Groups, references: &References, graph: &Graph) -> Vec<ConfigError> {
    let members: Vec<_> = graph
        .packages()
        .iter()
        .filter(|package| package.workspace_member)
        .collect();
    let mut errors = Vec::new();
    for (name, group) in groups.iter() {
        if !(group.selects_members() || references.sources.contains(name)) {
            continue;
        }
        if members.iter().any(|member| group.matches(member)) {
            errors.extend(
                group
                    .unmatched_patterns(&members)
                    .into_iter()
                    .map(|pattern| ConfigError::PatternMatchesNoMembers {
                        group: name.to_owned(),
                        pattern: pattern.to_owned(),
                    }),
            );
        } else {
            errors.push(ConfigError::GroupMatchesNoMembers {
                group: name.to_owned(),
            });
        }
    }
    errors
}

fn warnings(
    groups: &Groups,
    rules: &[Rule],
    references: &References,
    graph: &Graph,
) -> Vec<ConfigWarning> {
    let mut warnings: Vec<ConfigWarning> = groups
        .iter()
        .filter(|(name, _)| {
            references.negated.contains(*name) && !references.positive.contains(*name)
        })
        .filter(|(_, group)| {
            !graph
                .packages()
                .iter()
                .any(|package| group.matches(package))
        })
        .map(|(name, _)| ConfigWarning::NegatedGroupMatchesNothing {
            group: name.to_owned(),
        })
        .collect();
    if rules.is_empty() {
        warnings.push(ConfigWarning::NoRules);
    }
    warnings
}

impl ConfigError {
    /// The group the error is about, if it's about one.
    fn group(&self) -> Option<&str> {
        match self {
            Self::InvalidGroupName { group }
            | Self::ReservedGroupName { group }
            | Self::InvalidPattern { group, .. }
            | Self::EmptyPattern { group }
            | Self::EmptyNameList { group }
            | Self::GroupHasNoConditions { group }
            | Self::GroupMatchesNoMembers { group }
            | Self::PatternMatchesNoMembers { group, .. } => Some(group),
            Self::Schema(_)
            | Self::InvalidRuleName { .. }
            | Self::DuplicateRuleName { .. }
            | Self::UnknownGroup { .. }
            | Self::PrefixNegation { .. }
            | Self::EmptyGroupList { .. }
            | Self::EmptyKinds { .. }
            | Self::EmptyReason { .. } => None,
        }
    }
}

impl SchemaError {
    /// Where the problem is in the config, as a byte range, if it's known.
    pub fn span(&self) -> Option<Range<usize>> {
        self.0.span()
    }
}

impl ConfigErrors {
    /// Every problem found.
    pub fn iter(&self) -> std::slice::Iter<'_, ConfigError> {
        self.0.iter()
    }
}

impl IntoIterator for ConfigErrors {
    type Item = ConfigError;
    type IntoIter = std::vec::IntoIter<ConfigError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ConfigErrors {
    type Item = &'a ConfigError;
    type IntoIter = std::slice::Iter<'a, ConfigError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl std::error::Error for ConfigErrors {}

impl fmt::Display for ConfigErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            write!(formatter, "{error}")?;
        }
        Ok(())
    }
}
