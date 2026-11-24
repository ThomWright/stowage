# bin-only

A dependency on a crate with no lib target.

```text
harness ──(dev)──► tool (bin only)
```

Cargo ignores this dependency when building and prints a warning (`ignoring invalid dependency 'tool' which is missing a lib target`). Stowage follows what Cargo builds, so the edge is left out. `cargo tree` still lists it, so the comparison test with `cargo tree` skips this fixture.
