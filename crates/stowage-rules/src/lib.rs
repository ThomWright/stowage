//! Declarative dependency rules for Cargo workspaces.
//!
//! Stowage checks rules such as "the domain crate depends on no infrastructure" against a
//! workspace's dependency graph, from a `stowage.toml` config, inside `cargo test`.

mod graph;
mod group;

pub use graph::{Graph, LoadError};
