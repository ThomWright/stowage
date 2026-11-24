//! The resolved dependency graph of a Cargo workspace.
//!
//! This is the only module that uses guppy, so that a change to guppy's API, or a switch to
//! another graph library, stays here.

#![expect(
    clippy::disallowed_types,
    reason = "this module is the boundary that keeps guppy out of the rest of the crate"
)]
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside the tests reads the graph until the rule checker exists"
    )
)]

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use guppy::MetadataCommand;
use guppy::graph::cargo::{BuildPlatform, CargoOptions, CargoResolverVersion, CargoSet};
use guppy::graph::feature::StandardFeatures;
use guppy::graph::{
    BuildTargetId, DependencyDirection, PackageGraph, PackageLink, PackageMetadata,
};
use guppy::platform::PlatformSpec;

/// The dependency graph of a Cargo workspace, resolved separately for each workspace member.
///
/// A member's resolution is what Cargo would build for that member on its own, with all of its
/// own features on, including its dev-dependencies, on every platform. Its dependencies get only
/// the features requested by the member, directly or transitively, so a feature turned on by
/// another member doesn't count. This is what
/// `cargo tree -p <member> --all-features -e normal,build,dev --target all` shows, except that
/// a dependency on a crate with no lib target is left out, as Cargo ignores it when building.
///
/// Features are resolved as Cargo's version 2 resolver does, which is the default from edition
/// 2021. A workspace that still uses version 1 can see differences.
#[derive(Debug)]
pub struct Graph {
    /// Sorted by name, then version.
    packages: Vec<Package>,
    /// One for each workspace member.
    resolutions: Vec<Resolution>,
}

/// Identifies a package within one [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackageId(usize);

/// A package in the graph: a workspace member or an external crate.
#[derive(Debug)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) workspace_member: bool,
    /// Has at least one bin target.
    pub(crate) has_bin: bool,
    pub(crate) proc_macro: bool,
}

/// Where a package in a resolution is built to run.
///
/// - **Target**: the platform the source member is being built to run on. Normal and dev
///   dependencies are compiled for the target and linked into what the source member produces.
/// - **Host**: the machine running the compiler. Build scripts, proc macros, and all of their
///   dependencies are compiled for the host and run there, at build time.
///
/// Cargo resolves features separately for each, so one package can appear on both with
/// different features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Platform {
    Target,
    Host,
}

/// A package built for a [`Platform`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Node {
    pub(crate) package: PackageId,
    pub(crate) platform: Platform,
}

/// How a dependency edge counts towards the kind of a path that uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum EdgeKind {
    /// The dependency is linked into the package that depends on it.
    Normal,
    /// The dependency runs at build time. Every edge between two host packages is a build
    /// edge, whatever the manifest calls it, because everything on the host runs at build time.
    Build,
    /// The dependency is used only by the source member's tests, examples and benches.
    Dev,
}

/// A dependency of one node on another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Edge {
    pub(crate) from: Node,
    pub(crate) to: Node,
    pub(crate) kind: EdgeKind,
}

/// What one workspace member builds, as described on [`Graph`].
#[derive(Debug)]
pub(crate) struct Resolution {
    pub(crate) source: PackageId,
    /// Every node in the resolution, with the named features enabled on it.
    pub(crate) features: BTreeMap<Node, BTreeSet<String>>,
    pub(crate) edges: BTreeSet<Edge>,
}

/// The workspace's dependency graph couldn't be loaded, for example because `cargo metadata`
/// failed.
#[derive(Debug, thiserror::Error)]
#[error("failed to load the workspace's dependency graph")]
pub struct LoadError(#[source] guppy::Error);

impl Graph {
    /// Loads the graph of the workspace containing `manifest_path` (a `Cargo.toml`), by running
    /// `cargo metadata`.
    ///
    /// # Errors
    ///
    /// Returns an error if `cargo metadata` fails, or its output can't be resolved.
    pub fn load_from(manifest_path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let mut command = MetadataCommand::new();
        command.manifest_path(manifest_path.as_ref());
        Self::from_command(command)
    }

    fn from_command(mut command: MetadataCommand) -> Result<Self, LoadError> {
        let graph = PackageGraph::from_command(&mut command).map_err(LoadError)?;
        Self::from_package_graph(&graph).map_err(LoadError)
    }

    fn from_package_graph(graph: &PackageGraph) -> Result<Self, guppy::Error> {
        let mut metadata: Vec<PackageMetadata<'_>> = graph.packages().collect();
        // Sorted so that package order, and so report order, doesn't depend on guppy's internals.
        metadata.sort_by(|a, b| (a.name(), a.version()).cmp(&(b.name(), b.version())));
        let ids: HashMap<&guppy::PackageId, PackageId> = metadata
            .iter()
            .enumerate()
            .map(|(index, package)| (package.id(), PackageId(index)))
            .collect();
        let packages = metadata.iter().map(Package::new).collect();

        let mut options = CargoOptions::new();
        options
            .set_resolver(CargoResolverVersion::V2)
            .set_include_dev(true)
            // `Any` is the union of all platforms. `Always`, their intersection, would drop every
            // `cfg(...)` dependency.
            .set_host_platform(PlatformSpec::Any)
            .set_target_platform(PlatformSpec::Any);

        let resolutions = graph
            .workspace()
            .iter()
            .map(|member| resolve(graph, &member, &ids, &options))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            packages,
            resolutions,
        })
    }

    pub(crate) fn packages(&self) -> &[Package] {
        &self.packages
    }

    pub(crate) fn package(&self, id: PackageId) -> &Package {
        &self.packages[id.0]
    }

    pub(crate) fn resolutions(&self) -> &[Resolution] {
        &self.resolutions
    }
}

impl Package {
    fn new(metadata: &PackageMetadata<'_>) -> Self {
        Self {
            name: metadata.name().to_owned(),
            version: metadata.version().to_string(),
            workspace_member: metadata.in_workspace(),
            has_bin: metadata
                .build_targets()
                .any(|target| matches!(target.id(), BuildTargetId::Binary(_))),
            proc_macro: metadata.is_proc_macro(),
        }
    }
}

impl From<BuildPlatform> for Platform {
    fn from(platform: BuildPlatform) -> Self {
        match platform {
            BuildPlatform::Target => Self::Target,
            BuildPlatform::Host => Self::Host,
        }
    }
}

fn resolve(
    graph: &PackageGraph,
    member: &PackageMetadata<'_>,
    ids: &HashMap<&guppy::PackageId, PackageId>,
    options: &CargoOptions<'_>,
) -> Result<Resolution, guppy::Error> {
    let initials = member.to_feature_set(StandardFeatures::All);
    let cargo_set = CargoSet::new(initials, graph.feature_graph().resolve_none(), options)?;
    // Every package returned by guppy is one of `graph.packages()`, so it's always in `ids`.
    let node = |package: &PackageMetadata<'_>, platform| Node {
        package: ids[package.id()],
        platform,
    };

    let mut features = BTreeMap::new();
    for (platform, feature_set) in cargo_set.all_features() {
        for list in feature_set.packages_with_features(DependencyDirection::Forward) {
            features.insert(
                node(list.package(), platform.into()),
                list.named_features().map(str::to_owned).collect(),
            );
        }
    }

    let mut edges = BTreeSet::new();
    let mut add = |link: &PackageLink<'_>, from: Platform, to: Platform, kind: EdgeKind| {
        edges.insert(Edge {
            from: node(&link.from(), from),
            to: node(&link.to(), to),
            kind,
        });
    };
    for link in cargo_set.target_links() {
        add(
            &link,
            Platform::Target,
            Platform::Target,
            normal_or_dev(&link),
        );
    }
    for link in cargo_set.proc_macro_links() {
        add(
            &link,
            Platform::Target,
            Platform::Host,
            normal_or_dev(&link),
        );
    }
    for link in cargo_set.build_dep_links() {
        add(&link, Platform::Target, Platform::Host, EdgeKind::Build);
    }
    for link in cargo_set.host_links() {
        add(&link, Platform::Host, Platform::Host, EdgeKind::Build);
    }

    Ok(Resolution {
        source: ids[member.id()],
        features,
        edges,
    })
}

/// The kind of a link that `CargoSet` follows on the target. It is dev only when it comes from
/// the source member, because a dependency's dev-dependencies are never built.
fn normal_or_dev(link: &PackageLink<'_>) -> EdgeKind {
    if link.normal().is_present() {
        EdgeKind::Normal
    } else {
        EdgeKind::Dev
    }
}
