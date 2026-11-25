//! Checking a workspace's rules from a test.

#[cfg(test)]
mod tests;

use std::error::Error as _;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::{Config, ConfigErrors, Graph, LoadError, Report};

/// The config file, at the workspace root.
const CONFIG_FILE: &str = "stowage.toml";

/// Checks the rules in `stowage.toml`, at the root of the workspace containing the current
/// directory, and panics if any are broken.
///
/// Call it from a test, which `cargo test` runs in the directory of the crate it belongs to:
///
/// ```no_run
/// #[test]
/// fn stowage_rules() {
///     stowage_rules::assert_workspace();
/// }
/// ```
///
/// Warnings about the config are written to standard error, where `cargo test` shows them even
/// when the test passes.
///
/// The workspace's graph is loaded once per process, so several tests can call this without
/// running `cargo metadata` again.
///
/// # Panics
///
/// If the workspace's graph can't be loaded, `stowage.toml` is missing or invalid, or a rule is
/// broken. The panic message says which, and lists every violation or config error.
#[expect(clippy::panic, reason = "a broken rule fails the test that calls this")]
// The panic then points at the test, not at this function.
#[track_caller]
pub fn assert_workspace() {
    static GRAPH: OnceLock<Result<Graph, LoadError>> = OnceLock::new();
    let graph = GRAPH.get_or_init(Graph::load_current_dir);
    match check_workspace(graph) {
        Ok(report) => {
            if !report.warnings().is_empty() {
                // Written to stderr directly, because libtest captures `eprintln!` and only shows
                // it when the test fails.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a test shouldn't fail because stderr can't be written to"
                )]
                let _ = writeln!(io::stderr(), "{report}");
            }
        }
        Err(failure) => panic!("{failure}"),
    }
}

/// Why a workspace failed its check.
#[derive(Debug)]
enum Failure<'a> {
    /// The workspace's graph couldn't be loaded.
    Load(&'a LoadError),
    /// There's no `stowage.toml` at the workspace root.
    MissingConfig { root: PathBuf },
    /// `stowage.toml` exists, but couldn't be read.
    ReadConfig { root: PathBuf, error: io::Error },
    /// `stowage.toml` isn't a valid config for the workspace.
    InvalidConfig { root: PathBuf, errors: ConfigErrors },
    /// At least one rule is broken.
    Violations(Report),
}

/// Checks the rules in the `stowage.toml` at the root of `graph`'s workspace, as described on
/// [`assert_workspace`]. A report without violations, which may still have warnings, is a pass.
fn check_workspace(graph: &Result<Graph, LoadError>) -> Result<Report, Failure<'_>> {
    let graph = graph.as_ref().map_err(Failure::Load)?;
    let root = graph.workspace_root();
    let source = match fs::read_to_string(root.join(CONFIG_FILE)) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Failure::MissingConfig {
                root: root.to_owned(),
            });
        }
        Err(error) => {
            return Err(Failure::ReadConfig {
                root: root.to_owned(),
                error,
            });
        }
    };
    let config = Config::parse(&source, graph).map_err(|errors| Failure::InvalidConfig {
        root: root.to_owned(),
        errors,
    })?;
    let report = config.check(graph);
    if report.violations().is_empty() {
        Ok(report)
    } else {
        Err(Failure::Violations(report))
    }
}

impl fmt::Display for Failure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => {
                // Each cause in turn, ending with cargo's own error, which says what to fix.
                write!(f, "{error}")?;
                let mut cause = error.source();
                while let Some(error) = cause {
                    write!(f, ": {error}")?;
                    cause = error.source();
                }
                Ok(())
            }
            Self::MissingConfig { root } => {
                write!(f, "no {CONFIG_FILE} at {}", root.display())
            }
            Self::ReadConfig { root, error } => {
                write!(
                    f,
                    "couldn't read {CONFIG_FILE} at {}: {error}",
                    root.display()
                )
            }
            Self::InvalidConfig { root, errors } => {
                write!(
                    f,
                    "{CONFIG_FILE} at {} is invalid:\n{errors}",
                    root.display()
                )
            }
            Self::Violations(report) => write!(f, "{report}"),
        }
    }
}
