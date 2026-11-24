# orders

The design doc's worked example: a workspace with no violations of its rules.

```text
orders-tests ──► orders-api ──┬──► orders-domain
     │            [bin]       ├──► orders-db ──┬──► orders-domain
     │                        │                ├──► sqlx-core
     │                        │                └──► serde
     │                        ├──► orders-ids
     │                        └──► async-graphql
     └──► orders-domain

orders-worker ──┬──► orders-domain
    [bin]       └──► orders-db
```

- `orders-domain` depends on nothing, so it is the bottom layer.
- Only the test harness, `orders-tests`, depends on an entrypoint.
- Nothing depends on the test harness.
- Serialisation (`serde`) and SQL (`sqlx-core`) are reached only through `orders-db`.

`orders-api` and `orders-worker` have a lib target as well as a bin, because Cargo ignores a dependency on a crate with no lib target.
