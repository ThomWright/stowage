//! Declarative dependency rules for Cargo workspaces.
//!
//! Stowage checks rules such as "the domain crate depends on no infrastructure" against a
//! workspace's dependency graph, from a `stowage.toml` config, inside `cargo test`.

mod assert;
mod check;
mod config;
mod de;
mod graph;
mod group;
mod rule;

pub use assert::assert_workspace;
pub use check::{Hop, Report, Violation};
pub use config::{Config, ConfigError, ConfigErrors, ConfigWarning, SchemaError};
pub use graph::{Graph, LoadError};
pub use rule::PathKind;
