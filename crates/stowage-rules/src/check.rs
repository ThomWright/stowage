//! Checking a config's rules against a workspace's dependency graph.

#[cfg(test)]
mod tests;

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::config::ConfigWarning;
use crate::graph::{Edge, EdgeKind, Graph, Node, Package, PackageId, Platform, Resolution};
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

    /// The kind of the path from the source to the target. When the source reaches the target
    /// by forbidden paths of several kinds, this is the most serious: normal, then build, then
    /// dev.
    pub fn kind(&self) -> PathKind {
        self.kind
    }
}

/// A path of dependencies from a source crate to a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Path {
    /// In order from the source crate. Never empty.
    pub(crate) edges: Vec<Edge>,
    /// The kind of the path's least serious edge: any build edge makes it a build path, and a dev
    /// first edge makes it a dev path.
    pub(crate) kind: PathKind,
}

/// The path to report for each target that the source crate of `resolution` reaches.
///
/// The search starts from the source crate on the target platform, and follows its own
/// dependencies for a direct reach, or dependencies through any number of crates for a transitive
/// one.
///
/// - Dev edges are followed only on the first hop.
/// - Each path stops at the first target it reaches, so a target's own dependencies aren't
///   searched.
/// - The source crate is never a target, even if `is_target` says it is.
/// - Only paths whose kind is in `kinds` are reported. For each target, the path reported is the
///   shortest of the most serious kind. Of equally short paths, the one whose sequence of edges
///   comes first, in the order of [`Edge`], is reported.
fn search(
    resolution: &Resolution,
    is_target: impl Fn(PackageId) -> bool,
    kinds: &BTreeSet<PathKind>,
    reach: Reach,
) -> BTreeMap<PackageId, Path> {
    let mut dependencies: BTreeMap<Node, Vec<Edge>> = BTreeMap::new();
    // The edges are a sorted set, so each node's edges are in order too.
    for edge in &resolution.edges {
        dependencies.entry(edge.from).or_default().push(*edge);
    }

    // A breadth-first search over (node, path kind) states, so that a node reached by a less
    // serious path is still searched through by a more serious one. States are expanded in the
    // order they're found, and each node's edges followed in order, so paths are found in order
    // of length, then of their edges. The first path found to each state is the one to keep.
    let start = Node {
        package: resolution.source,
        platform: Platform::Target,
    };
    let mut seen: BTreeSet<(Node, PathKind)> = BTreeSet::new();
    let mut queue: VecDeque<(Node, Option<Path>)> = VecDeque::from([(start, None)]);
    let mut reported: BTreeMap<PackageId, Path> = BTreeMap::new();
    while let Some((node, path)) = queue.pop_front() {
        let first_hop = path.is_none();
        for edge in dependencies.get(&node).into_iter().flatten() {
            if edge.kind == EdgeKind::Dev && !first_hop {
                continue;
            }
            // Anything reached through the source again is also reached from the start, by a
            // shorter path of a kind at least as serious.
            if edge.to.package == resolution.source {
                continue;
            }
            let path = match &path {
                None => Path {
                    edges: vec![*edge],
                    kind: path_kind(edge.kind),
                },
                Some(path) => Path {
                    edges: [path.edges.as_slice(), &[*edge]].concat(),
                    kind: path.kind.max(path_kind(edge.kind)),
                },
            };
            if !seen.insert((edge.to, path.kind)) {
                continue;
            }
            if is_target(edge.to.package) {
                if kinds.contains(&path.kind) {
                    report(&mut reported, edge.to.package, path);
                }
            } else if reach == Reach::Transitive {
                queue.push_back((edge.to, Some(path)));
            }
        }
    }
    reported
}

/// Keeps `path` as the path reported for `target` if there's none yet, or if it's more serious
/// than the one reported so far. [`search`] finds paths in order of length, then of their edges,
/// so a later path of the same kind is never the one to report.
fn report(reported: &mut BTreeMap<PackageId, Path>, target: PackageId, path: Path) {
    match reported.entry(target) {
        Entry::Vacant(entry) => {
            entry.insert(path);
        }
        Entry::Occupied(mut entry) => {
            if path.kind < entry.get().kind {
                entry.insert(path);
            }
        }
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
        .flat_map(|rule| violations(groups, rule, graph))
        .collect();
    Report {
        violations,
        warnings: warnings.to_vec(),
    }
}

/// The violations of a rule, from each of its source members, ordered by source, then target.
fn violations(groups: &Groups, rule: &Rule, graph: &Graph) -> Vec<Violation> {
    let mut resolutions: Vec<_> = graph
        .resolutions()
        .iter()
        // There's a resolution for each workspace member, and only for them, so only members are
        // ever sources.
        .filter(|resolution| is_source(groups, &rule.from, graph.package(resolution.source)))
        .collect();
    // Package IDs are ordered by name, then version.
    resolutions.sort_by_key(|resolution| resolution.source);
    let group_of = |target: PackageId| target_group(groups, &rule.to, graph.package(target));
    let mut violations = Vec::new();
    for resolution in resolutions {
        let source = graph.package(resolution.source);
        let paths = search(
            resolution,
            |package| group_of(package).is_some(),
            &rule.kinds,
            rule.reach,
        );
        for (target, path) in paths {
            // Every reported target passed `group_of`.
            let Some(group) = group_of(target) else {
                continue;
            };
            violations.push(Violation {
                rule: rule.name.clone(),
                reason: rule.reason.clone(),
                source: source.name.clone(),
                target: graph.package(target).name.clone(),
                target_group: group.to_owned(),
                kind: path.kind,
            });
        }
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

/// The kind of a path made of just one edge of `kind`.
fn path_kind(kind: EdgeKind) -> PathKind {
    match kind {
        EdgeKind::Normal => PathKind::Normal,
        EdgeKind::Build => PathKind::Build,
        EdgeKind::Dev => PathKind::Dev,
    }
}
