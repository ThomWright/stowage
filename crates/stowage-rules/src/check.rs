//! Checking a config's rules against a workspace's dependency graph.

use std::collections::BTreeMap;

use crate::config::ConfigWarning;
use crate::graph::{EdgeKind, Graph, Package, PackageId};
use crate::group::Groups;
use crate::rule::{PathKind, Reach, Rule, Sources};

/// The result of checking a config against a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    violations: Vec<Violation>,
    warnings: Vec<ConfigWarning>,
}

/// A dependency that a rule forbids: a source crate reaching a target crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    rule: String,
    reason: String,
    source: String,
    target: String,
    target_group: String,
    kind: PathKind,
}

impl Report {
    /// Every violation, ordered by rule (in the order the rules are written), then source crate,
    /// then target crate.
    pub fn violations(&self) -> &[Violation] {
        &self.violations
    }

    /// Things in the config that are probably mistakes.
    pub fn warnings(&self) -> &[ConfigWarning] {
        &self.warnings
    }
}

impl Violation {
    /// The name of the rule.
    pub fn rule(&self) -> &str {
        &self.rule
    }

    /// Why the rule forbids the dependency.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The name of the workspace member that has the forbidden dependency.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The name of the crate it reaches.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The first of the rule's `to` groups that the target is in.
    pub fn target_group(&self) -> &str {
        &self.target_group
    }

    /// The kind of the path from the source to the target. When several paths count, this is
    /// the most serious: normal, then build, then dev.
    pub fn kind(&self) -> PathKind {
        self.kind
    }
}

/// Checks every rule, as described on [`crate::Config::check`].
pub(crate) fn check(
    groups: &Groups,
    rules: &[Rule],
    warnings: &[ConfigWarning],
    graph: &Graph,
) -> Report {
    let violations = rules
        .iter()
        .filter(|rule| rule.reach == Reach::Direct)
        .flat_map(|rule| direct_violations(groups, rule, graph))
        .collect();
    Report {
        violations,
        warnings: warnings.to_vec(),
    }
}

/// The violations of a direct rule: each source member's edges to a target in `to`, of a kind
/// in `kinds`. Ordered by source, then target.
fn direct_violations(groups: &Groups, rule: &Rule, graph: &Graph) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut resolutions: Vec<_> = graph
        .resolutions()
        .iter()
        // There's a resolution for each workspace member, and only for them, so only members are
        // ever sources.
        .filter(|resolution| is_source(groups, &rule.from, graph.package(resolution.source)))
        .collect();
    // Package IDs are ordered by name, then version.
    resolutions.sort_by_key(|resolution| resolution.source);
    for resolution in resolutions {
        // For each target, its group and the most serious kind found so far. `PathKind` is
        // ordered from most to least serious.
        let mut targets: BTreeMap<PackageId, (&str, PathKind)> = BTreeMap::new();
        for edge in &resolution.edges {
            if edge.from.package != resolution.source {
                continue;
            }
            let kind = path_kind(edge.kind);
            if !rule.kinds.contains(&kind) {
                continue;
            }
            let Some(group) = target_group(groups, &rule.to, graph.package(edge.to.package)) else {
                continue;
            };
            targets
                .entry(edge.to.package)
                .and_modify(|(_, found)| *found = (*found).min(kind))
                .or_insert((group, kind));
        }
        let source = graph.package(resolution.source);
        violations.extend(
            targets
                .into_iter()
                .map(|(target, (group, kind))| Violation {
                    rule: rule.name.clone(),
                    reason: rule.reason.clone(),
                    source: source.name.clone(),
                    target: graph.package(target).name.clone(),
                    target_group: group.to_owned(),
                    kind,
                }),
        );
    }
    violations
}

/// Whether the workspace member `package` is one of the rule's source crates, as described on
/// [`Sources`].
fn is_source(groups: &Groups, sources: &Sources, package: &Package) -> bool {
    let in_any = sources
        .groups
        .iter()
        .any(|name| groups.get(name).is_some_and(|group| group.matches(package)));
    in_any != sources.negated
}

/// The first of `to` that `package` is in.
fn target_group<'a>(groups: &Groups, to: &'a [String], package: &Package) -> Option<&'a str> {
    to.iter()
        .find(|name| groups.get(name).is_some_and(|group| group.matches(package)))
        .map(String::as_str)
}

/// For a direct rule, the path is a single edge, so its kind is the edge's kind.
fn path_kind(kind: EdgeKind) -> PathKind {
    match kind {
        EdgeKind::Normal => PathKind::Normal,
        EdgeKind::Build => PathKind::Build,
        EdgeKind::Dev => PathKind::Dev,
    }
}
