//! Tests of the path search, over resolutions built by hand.
//!
//! The example tests cover chosen cases, such as which of several paths is reported. The property
//! tests check what must hold for any well-formed resolution, as described on [`scenarios`].

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::sample::{select, subsequence};

use super::{Path, search};
use crate::graph::{Edge, EdgeKind, Node, PackageId, Platform, Resolution};
use crate::rule::{PathKind, Reach};

/// The index of every resolution's source crate.
const SOURCE: usize = 0;

const ALL_KINDS: [PathKind; 3] = [PathKind::Normal, PathKind::Build, PathKind::Dev];

fn id(index: usize) -> PackageId {
    PackageId::from_index(index)
}

fn on_target(index: usize) -> Node {
    Node {
        package: id(index),
        platform: Platform::Target,
    }
}

fn on_host(index: usize) -> Node {
    Node {
        package: id(index),
        platform: Platform::Host,
    }
}

fn normal(from: Node, to: Node) -> Edge {
    Edge {
        from,
        to,
        kind: EdgeKind::Normal,
    }
}

fn build(from: Node, to: Node) -> Edge {
    Edge {
        from,
        to,
        kind: EdgeKind::Build,
    }
}

fn dev(from: Node, to: Node) -> Edge {
    Edge {
        from,
        to,
        kind: EdgeKind::Dev,
    }
}

fn resolution(edges: impl IntoIterator<Item = Edge>) -> Resolution {
    Resolution {
        source: id(SOURCE),
        // The search doesn't read features.
        features: BTreeMap::new(),
        edges: edges.into_iter().collect(),
    }
}

/// Searches for paths of every kind.
fn reports(edges: &[Edge], targets: &[usize], reach: Reach) -> BTreeMap<PackageId, Path> {
    reports_of_kinds(edges, targets, &ALL_KINDS, reach)
}

fn reports_of_kinds(
    edges: &[Edge],
    targets: &[usize],
    kinds: &[PathKind],
    reach: Reach,
) -> BTreeMap<PackageId, Path> {
    let targets: BTreeSet<PackageId> = targets.iter().copied().map(id).collect();
    search(
        &resolution(edges.iter().copied()),
        |package| targets.contains(&package),
        &kinds.iter().copied().collect(),
        reach,
    )
}

fn path(kind: PathKind, edges: &[Edge]) -> Path {
    Path {
        edges: edges.to_vec(),
        kind,
    }
}

mod examples {
    use super::*;

    const S: usize = SOURCE;
    const A: usize = 1;
    const B: usize = 2;
    const C: usize = 3;
    const T: usize = 4;
    const U: usize = 5;

    #[test]
    fn the_shortest_path_is_reported() {
        let via_a_and_b = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(B)),
            normal(on_target(B), on_target(T)),
        ];
        let via_c = [
            normal(on_target(S), on_target(C)),
            normal(on_target(C), on_target(T)),
        ];
        let edges = [&via_a_and_b[..], &via_c[..]].concat();
        assert_eq!(
            reports(&edges, &[T], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &via_c))])
        );
    }

    #[test]
    fn of_equally_short_paths_the_first_in_package_order_is_reported() {
        let via_b = [
            normal(on_target(S), on_target(B)),
            normal(on_target(B), on_target(T)),
        ];
        let via_a = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(T)),
        ];
        let edges = [via_b, via_a].concat();
        assert_eq!(
            reports(&edges, &[T], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &via_a))])
        );
    }

    #[test]
    fn a_more_serious_kind_beats_a_shorter_path() {
        let build_path = [build(on_target(S), on_host(T))];
        let normal_path = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(B)),
            normal(on_target(B), on_target(T)),
        ];
        let edges = [&build_path[..], &normal_path[..]].concat();
        assert_eq!(
            reports(&edges, &[T], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &normal_path))])
        );
    }

    /// Such as a crate that's a dev-dependency of the source, and also a normal dependency of one
    /// of the source's other dependencies.
    #[test]
    fn a_crate_reached_first_by_a_less_serious_path_is_still_searched_through() {
        let dev_path = [
            dev(on_target(S), on_target(A)),
            normal(on_target(A), on_target(T)),
        ];
        let normal_path = [
            normal(on_target(S), on_target(B)),
            normal(on_target(B), on_target(A)),
            normal(on_target(A), on_target(T)),
        ];
        let edges = [&dev_path[..], &normal_path[..]].concat();
        assert_eq!(
            reports(&edges, &[T], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &normal_path))])
        );
    }

    #[test]
    fn only_paths_of_the_given_kinds_are_reported() {
        let build_path = [build(on_target(S), on_host(T))];
        let normal_path = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(T)),
        ];
        let edges = [&build_path[..], &normal_path[..]].concat();
        assert_eq!(
            reports_of_kinds(&edges, &[T], &[PathKind::Build], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Build, &build_path))])
        );
        assert_eq!(
            reports_of_kinds(&edges, &[T], &[PathKind::Dev], Reach::Transitive),
            BTreeMap::new()
        );
    }

    /// The kind of a path is the kind of its least serious edge, and edges between host packages
    /// are build edges, as described on [`Path`].
    #[test]
    fn each_path_has_the_kind_of_its_least_serious_edge() {
        let cases = [
            (
                "normal all the way",
                vec![
                    normal(on_target(S), on_target(A)),
                    normal(on_target(A), on_target(T)),
                ],
                PathKind::Normal,
            ),
            (
                "a normal edge into a proc macro",
                vec![normal(on_target(S), on_host(T))],
                PathKind::Normal,
            ),
            (
                "out of a proc macro",
                vec![
                    normal(on_target(S), on_host(A)),
                    build(on_host(A), on_host(T)),
                ],
                PathKind::Build,
            ),
            (
                "a build edge after normal ones",
                vec![
                    normal(on_target(S), on_target(A)),
                    build(on_target(A), on_host(T)),
                ],
                PathKind::Build,
            ),
            (
                "a dev first edge",
                vec![
                    dev(on_target(S), on_target(A)),
                    normal(on_target(A), on_target(T)),
                ],
                PathKind::Dev,
            ),
            (
                "a dev first edge, then a build edge",
                vec![
                    dev(on_target(S), on_target(A)),
                    build(on_target(A), on_host(T)),
                ],
                PathKind::Dev,
            ),
        ];
        for (name, edges, kind) in cases {
            assert_eq!(
                reports(&edges, &[T], Reach::Transitive),
                BTreeMap::from([(id(T), path(kind, &edges))]),
                "{name}"
            );
        }
    }

    #[test]
    fn a_path_stops_at_the_first_target() {
        let to_t = [normal(on_target(S), on_target(T))];
        let t_to_u = [normal(on_target(T), on_target(U))];
        let edges = [to_t, t_to_u].concat();
        assert_eq!(
            reports(&edges, &[T, U], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &to_t))])
        );
    }

    #[test]
    fn a_target_beyond_another_is_reported_by_a_path_around_it() {
        let to_t = [normal(on_target(S), on_target(T))];
        let t_to_u = [normal(on_target(T), on_target(U))];
        let around_t = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(B)),
            normal(on_target(B), on_target(U)),
        ];
        let edges = [&to_t[..], &t_to_u[..], &around_t[..]].concat();
        assert_eq!(
            reports(&edges, &[T, U], Reach::Transitive),
            BTreeMap::from([
                (id(T), path(PathKind::Normal, &to_t)),
                (id(U), path(PathKind::Normal, &around_t)),
            ])
        );
    }

    #[test]
    fn a_direct_search_follows_only_the_sources_own_dependencies() {
        let two_hops_to_t = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(T)),
        ];
        let one_hop_to_u = [build(on_target(S), on_host(U))];
        let edges = [&two_hops_to_t[..], &one_hop_to_u[..]].concat();
        assert_eq!(
            reports(&edges, &[T, U], Reach::Direct),
            BTreeMap::from([(id(U), path(PathKind::Build, &one_hop_to_u))])
        );
    }

    /// A dependency's dev-dependencies are never built, so a resolution doesn't contain them.
    /// This checks that the search ignores them anyway.
    #[test]
    fn dev_edges_after_the_first_hop_are_ignored() {
        let edges = [
            normal(on_target(S), on_target(A)),
            dev(on_target(A), on_target(T)),
        ];
        assert_eq!(reports(&edges, &[T], Reach::Transitive), BTreeMap::new());
    }

    #[test]
    fn the_source_is_never_reported() {
        // A dev-dependency of the source can depend on the source.
        let edges = [
            dev(on_target(S), on_target(A)),
            normal(on_target(A), on_target(S)),
        ];
        assert_eq!(reports(&edges, &[S], Reach::Transitive), BTreeMap::new());
    }

    #[test]
    fn cycles_are_searched_once() {
        let to_t = [
            normal(on_target(S), on_target(A)),
            normal(on_target(A), on_target(B)),
            normal(on_target(B), on_target(T)),
        ];
        let back_to_a = normal(on_target(B), on_target(A));
        let edges = [&to_t[..], &[back_to_a]].concat();
        assert_eq!(
            reports(&edges, &[T], Reach::Transitive),
            BTreeMap::from([(id(T), path(PathKind::Normal, &to_t))])
        );
    }
}

mod properties {
    use super::*;

    /// The most packages in a generated resolution. Small graphs still have every shape of path,
    /// and failures shrink to something readable.
    const MAX_PACKAGES: usize = 8;

    /// A search to run: a resolution, and the arguments to search it with.
    #[derive(Debug)]
    struct Scenario {
        edges: BTreeSet<Edge>,
        targets: BTreeSet<PackageId>,
        kinds: BTreeSet<PathKind>,
        reach: Reach,
    }

    impl Scenario {
        fn search(&self) -> BTreeMap<PackageId, Path> {
            search(
                &resolution(self.edges.iter().copied()),
                |package| self.targets.contains(&package),
                &self.kinds,
                self.reach,
            )
        }
    }

    /// The edges that Cargo can produce in a resolution, as described on
    /// [`crate::graph::Graph`].
    #[derive(Debug, Clone, Copy)]
    enum Shape {
        /// A normal dependency.
        Normal,
        /// A dev-dependency, which only the source crate has.
        Dev,
        /// A build dependency, which runs on the host.
        Build,
        /// A normal dependency on a proc macro, which runs on the host.
        NormalProcMacro,
        /// A dev-dependency on a proc macro, which only the source crate has.
        DevProcMacro,
        /// Any dependency of a package on the host, which is a build edge.
        OnHost,
    }

    const SHAPES: [Shape; 6] = [
        Shape::Normal,
        Shape::Dev,
        Shape::Build,
        Shape::NormalProcMacro,
        Shape::DevProcMacro,
        Shape::OnHost,
    ];

    /// An edge of `shape` from package `from` to package `to`, or `None` if Cargo can't produce
    /// it: a package depending on itself, or the source crate on the host, which would be a
    /// cycle through a build dependency or a proc macro.
    fn edge(shape: Shape, from: usize, to: usize) -> Option<Edge> {
        let edge = match shape {
            Shape::Normal => normal(on_target(from), on_target(to)),
            Shape::Dev => dev(on_target(SOURCE), on_target(to)),
            Shape::Build => build(on_target(from), on_host(to)),
            Shape::NormalProcMacro => normal(on_target(from), on_host(to)),
            Shape::DevProcMacro => dev(on_target(SOURCE), on_host(to)),
            Shape::OnHost => build(on_host(from), on_host(to)),
        };
        let well_formed = edge.from != edge.to && edge.to != on_host(SOURCE);
        well_formed.then_some(edge)
    }

    /// Searches of well-formed resolutions: any edges that Cargo can produce, including cycles
    /// through the source's dev-dependencies. Any package can be a target, including the source,
    /// and the search can use any non-empty set of kinds, with either reach.
    fn scenarios() -> impl Strategy<Value = Scenario> {
        (2..=MAX_PACKAGES)
            .prop_flat_map(|packages| {
                (
                    proptest::collection::vec(
                        (select(SHAPES.to_vec()), 0..packages, 0..packages),
                        0..=packages * 3,
                    ),
                    subsequence((0..packages).collect::<Vec<_>>(), 0..=packages),
                    subsequence(ALL_KINDS.to_vec(), 1..=ALL_KINDS.len()),
                    select(vec![Reach::Direct, Reach::Transitive]),
                )
            })
            .prop_map(|(edges, targets, kinds, reach)| Scenario {
                edges: edges
                    .into_iter()
                    .filter_map(|(shape, from, to)| edge(shape, from, to))
                    .collect(),
                targets: targets.into_iter().map(id).collect(),
                kinds: kinds.into_iter().collect(),
                reach,
            })
    }

    /// The nodes a path visits, in order, starting with the source.
    fn nodes(path: &Path) -> Vec<Node> {
        let first = path.edges.first().map(|edge| edge.from);
        first
            .into_iter()
            .chain(path.edges.iter().map(|edge| edge.to))
            .collect()
    }

    fn edge_path_kind(kind: EdgeKind) -> PathKind {
        match kind {
            EdgeKind::Normal => PathKind::Normal,
            EdgeKind::Build => PathKind::Build,
            EdgeKind::Dev => PathKind::Dev,
        }
    }

    /// The targets that the search's reach gets to from the source, without passing through
    /// another target or following a dev edge after the first hop, whatever the kind of path.
    fn reachable_targets(scenario: &Scenario) -> BTreeSet<PackageId> {
        let start = on_target(SOURCE);
        let max_hops = match scenario.reach {
            Reach::Direct => 1,
            Reach::Transitive => usize::MAX,
        };
        let mut seen = BTreeSet::from([start]);
        let mut frontier = vec![start];
        let mut found = BTreeSet::new();
        let mut hops = 0;
        while !frontier.is_empty() && hops < max_hops {
            hops += 1;
            let mut next = Vec::new();
            for node in frontier {
                for edge in scenario.edges.iter().filter(|edge| edge.from == node) {
                    if edge.kind == EdgeKind::Dev && node != start {
                        continue;
                    }
                    if !seen.insert(edge.to) {
                        continue;
                    }
                    if edge.to.package != id(SOURCE) && scenario.targets.contains(&edge.to.package)
                    {
                        found.insert(edge.to.package);
                    } else {
                        next.push(edge.to);
                    }
                }
            }
            frontier = next;
        }
        found
    }

    proptest! {
        /// Property 1: each path starts at the source on the target platform, ends at the target
        /// it's reported for, and is made of the resolution's edges, joined end to end. A direct
        /// search's paths are a single edge.
        #[test]
        fn each_path_is_made_of_edges_from_the_source_to_its_target(scenario in scenarios()) {
            for (target, path) in scenario.search() {
                prop_assert_eq!(path.edges.first().map(|edge| edge.from), Some(on_target(SOURCE)));
                prop_assert_eq!(path.edges.last().map(|edge| edge.to.package), Some(target));
                for pair in path.edges.windows(2) {
                    prop_assert_eq!(pair[0].to, pair[1].from, "{:?} isn't joined end to end", path);
                }
                for edge in &path.edges {
                    prop_assert!(scenario.edges.contains(edge), "{edge:?} isn't in the resolution");
                }
                if scenario.reach == Reach::Direct {
                    prop_assert_eq!(path.edges.len(), 1, "a direct search reported {:?}", path);
                }
            }
        }

        /// Property 2: only targets are reported, never the source, and a path passes through no
        /// target before its last node.
        #[test]
        fn each_path_stops_at_the_first_target(scenario in scenarios()) {
            for (target, path) in scenario.search() {
                prop_assert_ne!(target, id(SOURCE), "the source is reported as a target");
                prop_assert!(
                    scenario.targets.contains(&target),
                    "{target:?} is reported, but isn't a target"
                );
                let nodes = nodes(&path);
                for node in &nodes[1..nodes.len() - 1] {
                    prop_assert!(
                        !scenario.targets.contains(&node.package),
                        "{path:?} passes through the target {node:?}"
                    );
                }
            }
        }

        /// Property 3: no path visits a node twice.
        #[test]
        fn no_path_visits_a_node_twice(scenario in scenarios()) {
            for path in scenario.search().values() {
                let nodes = nodes(path);
                let distinct: BTreeSet<_> = nodes.iter().collect();
                prop_assert_eq!(distinct.len(), nodes.len(), "{:?} has a cycle", path);
            }
        }

        /// Property 4: a dev edge can only be the first edge of a path.
        #[test]
        fn dev_edges_are_only_first(scenario in scenarios()) {
            for path in scenario.search().values() {
                prop_assert!(
                    path.edges[1..].iter().all(|edge| edge.kind != EdgeKind::Dev),
                    "{path:?} has a dev edge after the first"
                );
            }
        }

        /// Property 5: each path has the kind of its least serious edge, and that kind is one
        /// of the kinds searched for.
        #[test]
        fn each_path_has_the_kind_of_its_edges(scenario in scenarios()) {
            for path in scenario.search().values() {
                let least_serious = path.edges.iter().map(|edge| edge_path_kind(edge.kind)).max();
                prop_assert_eq!(Some(path.kind), least_serious);
                prop_assert!(
                    scenario.kinds.contains(&path.kind),
                    "{path:?} is reported, but {:?} paths weren't searched for",
                    path.kind
                );
            }
        }

        /// Property 6: every target the search's reach gets to is reported when searching
        /// for every kind, and no other target is reported for any kinds.
        #[test]
        fn every_reachable_target_is_reported(scenario in scenarios()) {
            let reported: BTreeSet<PackageId> = scenario.search().into_keys().collect();
            let reachable = reachable_targets(&scenario);
            if scenario.kinds.len() == ALL_KINDS.len() {
                prop_assert_eq!(reported, reachable);
            } else {
                prop_assert!(
                    reported.is_subset(&reachable),
                    "reported {reported:?} includes targets that aren't reachable: {reachable:?}"
                );
            }
        }

        /// Property 7: a normal edge from the source straight to a target is the path reported
        /// for it, whatever other paths there are, when searching for normal paths.
        #[test]
        fn a_normal_edge_from_the_source_is_reported(
            mut scenario in scenarios(),
            target in 1..MAX_PACKAGES,
        ) {
            // Made a target, and normal paths searched for, rather than discarding scenarios
            // where they aren't, which would discard about half of them.
            scenario.targets.insert(id(target));
            scenario.kinds.insert(PathKind::Normal);
            let shortcut = normal(on_target(SOURCE), on_target(target));
            scenario.edges.insert(shortcut);
            prop_assert_eq!(
                scenario.search().remove(&id(target)),
                Some(path(PathKind::Normal, &[shortcut]))
            );
        }
    }
}
