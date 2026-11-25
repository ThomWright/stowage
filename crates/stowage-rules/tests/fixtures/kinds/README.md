# kinds

How each kind of dependency edge is resolved, from a single source crate `app`.

```text
app ──────────────────► helper ┄┄(dev)┄┄► helper-dev
 ├────────────────────► pm (proc macro) ──► pmdep
 ├──(build)───────────► bdep
 ├──(dev)─────────────► devdep
 └──cfg(windows)──────► winonly
```

It covers these cases:

- **A dependency's dev-dependencies are never built.** `helper-dev` is not part of `app`'s resolution.
- **A proc macro runs on the host.** The edge into `pm` is normal, but `pm` and all of its dependencies are built for the host, so `pm → pmdep` counts as a build edge.
- **Build and dev dependencies of the source** are kept, with their kinds.
- **Platform-specific dependencies** count on every platform, so `winonly` is included.

Its `stowage.toml` has no rules, so checking it passes with a warning.
