use std::collections::BTreeSet;
use std::process::Command;

use super::fixtures::{fixture, fixture_manifest};
use super::{EdgeKind, Graph, Platform, Resolution};

fn resolution<'a>(graph: &'a Graph, source: &str) -> &'a Resolution {
    graph
        .resolutions()
        .iter()
        .find(|r| graph.package(r.source).name == source)
        .expect("source is a workspace member")
}

fn platform(platform: Platform) -> &'static str {
    match platform {
        Platform::Target => "target",
        Platform::Host => "host",
    }
}

/// The source's resolved edges, as `from@platform -> to@platform (kind)`.
fn edges(graph: &Graph, source: &str) -> BTreeSet<String> {
    resolution(graph, source)
        .edges
        .iter()
        .map(|e| {
            let kind = match e.kind {
                EdgeKind::Normal => "normal",
                EdgeKind::Build => "build",
                EdgeKind::Dev => "dev",
            };
            format!(
                "{}@{} -> {}@{} ({kind})",
                graph.package(e.from.package).name,
                platform(e.from.platform),
                graph.package(e.to.package).name,
                platform(e.to.platform),
            )
        })
        .collect()
}

/// The source's resolved packages, as `name@platform [features]`.
fn features(graph: &Graph, source: &str) -> BTreeSet<String> {
    resolution(graph, source)
        .features
        .iter()
        .map(|(node, features)| {
            let features: Vec<_> = features.iter().map(String::as_str).collect();
            format!(
                "{}@{} [{}]",
                graph.package(node.package).name,
                platform(node.platform),
                features.join(",")
            )
        })
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|&s| s.to_owned()).collect()
}

#[test]
fn source_gets_all_its_own_features_and_only_requested_features_of_dependencies() {
    let graph = fixture("features");

    // `api` turns on `shared/sqlx`, but that doesn't count for `domain`. `domain`'s own optional
    // `serde` dependency counts because all of the source's own features are on.
    assert_eq!(
        features(&graph, "domain"),
        set(&[
            "domain@target [serde]",
            "dual@host [sqlx]",
            "dual@target []",
            "serde@target []",
            "shared@target [default,std]",
            "sqlx-core@host []",
        ])
    );

    // When `domain` is a dependency rather than the source, its optional `serde` stays off.
    assert_eq!(
        features(&graph, "api"),
        set(&[
            "api@target []",
            "domain@target []",
            "dual@host [sqlx]",
            "dual@target []",
            "shared@target [default,sqlx,std]",
            "sqlx-core@host []",
            "sqlx-core@target []",
        ])
    );
}

#[test]
fn host_and_target_are_resolved_separately() {
    let graph = fixture("features");

    // `dual` is a normal dependency without features and a build dependency with `sqlx`, so
    // `sqlx-core` is only reachable on the host.
    assert_eq!(
        edges(&graph, "domain"),
        set(&[
            "domain@target -> dual@host (build)",
            "domain@target -> dual@target (normal)",
            "domain@target -> serde@target (normal)",
            "domain@target -> shared@target (normal)",
            "dual@host -> sqlx-core@host (build)",
        ])
    );
}

#[test]
fn each_dependency_kind_resolves_to_the_expected_edges() {
    let graph = fixture("kinds");

    // `helper`'s own dev-dependency is never built for `app`, so it's absent. Edges out of the
    // proc macro run on the host, so they count as build edges.
    assert_eq!(
        edges(&graph, "app"),
        set(&[
            "app@target -> bdep@host (build)",
            "app@target -> devdep@target (dev)",
            "app@target -> helper@target (normal)",
            "app@target -> pm@host (normal)",
            "app@target -> winonly@target (normal)",
            "pm@host -> pmdep@host (build)",
        ])
    );
}

#[test]
fn dependencies_on_bin_only_crates_are_ignored() {
    let graph = fixture("bin-only");

    // Cargo ignores a dependency on a crate with no lib target, with a warning.
    assert_eq!(edges(&graph, "harness"), set(&[]));
}

/// What a package in the graph says about itself, other than its name.
#[derive(Debug, PartialEq, Eq)]
struct Description<'a> {
    version: &'a str,
    workspace_member: bool,
    has_bin: bool,
    proc_macro: bool,
}

fn describe<'a>(graph: &'a Graph, name: &str) -> Description<'a> {
    let p = graph
        .packages()
        .iter()
        .find(|p| p.name == name)
        .expect("package exists");
    Description {
        version: p.version.as_str(),
        workspace_member: p.workspace_member,
        has_bin: p.has_bin,
        proc_macro: p.proc_macro,
    }
}

#[test]
fn packages_describe_membership_and_targets() {
    let kinds = fixture("kinds");
    let bin_only = fixture("bin-only");

    assert_eq!(
        describe(&kinds, "app"),
        Description {
            version: "0.1.0",
            workspace_member: true,
            has_bin: false,
            proc_macro: false,
        }
    );
    assert_eq!(
        describe(&kinds, "pm"),
        Description {
            version: "0.1.0",
            workspace_member: false,
            has_bin: false,
            proc_macro: true,
        }
    );
    assert_eq!(
        describe(&bin_only, "tool"),
        Description {
            version: "0.1.0",
            workspace_member: true,
            has_bin: true,
            proc_macro: false,
        }
    );
}

/// The (package, features) pairs reported by `cargo tree` for `member`, using the same settings
/// as [`Graph`].
fn cargo_tree(fixture_name: &str, member: &str) -> BTreeSet<String> {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--quiet",
            // The output is parsed, so it mustn't depend on `CARGO_TERM_COLOR` or the terminal.
            "--color",
            "never",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(fixture_manifest(fixture_name))
        .args(["--package", member, "--all-features"])
        .args(["--edges", "normal,build,dev", "--target", "all"])
        .args(["--prefix", "none", "--format", "{p}|{f}"])
        .output()
        .expect("cargo tree runs");
    assert!(output.status.success(), "cargo tree failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("cargo tree prints UTF-8")
        .lines()
        .filter_map(|line| {
            let (package, features) = line.split_once('|')?;
            let name = package.split_whitespace().next()?;
            // Packages already printed by cargo tree are marked " (*)".
            let features = features.trim_end_matches(" (*)").trim();
            let mut features: Vec<_> = features.split(',').filter(|f| !f.is_empty()).collect();
            features.sort_unstable();
            Some(format!("{name} [{}]", features.join(",")))
        })
        .collect()
}

/// Compares against `cargo tree` for every member of every fixture that has no dependencies on
/// bin-only crates (which `cargo tree` lists but Cargo ignores).
#[test]
fn resolution_matches_cargo_tree() {
    for fixture_name in ["features", "kinds", "orders", "orders-broken"] {
        let graph = fixture(fixture_name);
        for resolution in graph.resolutions() {
            let member = &graph.package(resolution.source).name;
            let ours: BTreeSet<String> = resolution
                .features
                .iter()
                .map(|(node, features)| {
                    let features: Vec<_> = features.iter().map(String::as_str).collect();
                    format!(
                        "{} [{}]",
                        graph.package(node.package).name,
                        features.join(",")
                    )
                })
                .collect();
            assert_eq!(
                ours,
                cargo_tree(fixture_name, member),
                "{fixture_name}: {member}"
            );
        }
    }
}
