# features

Which features count when each workspace member is resolved as the source crate.

```text
api ──────────► domain ──────────────► shared ┄┄[sqlx]┄┄► sqlx-core
 │               │ ├───────────────────► dual
 │               │ ├┄┄[serde]┄┄┄┄┄┄┄┄┄► serde
 │               │ └──(build)──────────► dual[sqlx] ──► sqlx-core
 └──────────────────────────────────► shared[sqlx]

┄┄[f]┄┄►  optional, enabled by the depending crate's feature f
```

It covers three things:

- **Features turned on by another member don't count.** `api` turns on `shared`'s `sqlx` feature, but `domain` doesn't, so `domain` doesn't reach `sqlx-core` through `shared`.
- **All of the source's own features are on.** `domain`'s optional `serde` dependency counts when `domain` is the source, but not when `api` is.
- **Host and target are resolved separately.** `domain` uses `dual` normally with no features, and as a build dependency with `sqlx`. So `sqlx-core` is reachable only on the host, at build time.

`shared` also has a `default` feature, to check that default features are reported like any other.
