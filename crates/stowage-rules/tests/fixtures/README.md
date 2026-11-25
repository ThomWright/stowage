# Test fixtures

Small Cargo workspaces, loaded by tests with real `cargo metadata`. Each directory other than `external/` is a workspace, with its own README describing its scenario.

- `external/` holds crates that stand in for crates.io crates. Fixtures depend on them by path. They sit outside every fixture workspace, so they load as external packages, not workspace members.
- The crates are empty. Only their manifests matter.
- Each workspace's `Cargo.lock` is committed, and tests load fixtures with `--locked --offline`, so a stale lockfile fails the test instead of being quietly rewritten. After changing a fixture's manifests, regenerate its lockfile with `cargo generate-lockfile --offline` in its directory.
- The Stowage workspace excludes this directory, so Cargo never treats fixture crates as Stowage's own members.

A workspace's `stowage.toml`, where it has one, is the config that `assert_workspace()` finds at its root:

- `orders`: the design doc's worked example, which it follows.
- `orders-broken`: the same, which it breaks once for each rule.
- `kinds`: no rules, so checking it passes with a warning.
- `bin-only`: invalid, with three errors.
- The others have none, so checking them fails.
