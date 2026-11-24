# orders-broken

The `orders` workspace with one deliberate violation of each direct rule in the design doc's worked example. Everything else is the same as `orders`.

| Rule | Violation |
| --- | --- |
| Domain is the bottom layer | `orders-domain ──► orders-ids` |
| Only test harnesses depend on entrypoints | `orders-api ──► orders-worker` |
| Nothing depends on test harnesses | `orders-worker ┄┄(dev)┄┄► orders-tests` |
| Domain does not serialise | `orders-domain ┄┄[serde]┄┄► serde` |

The `serde` dependency is optional, behind `orders-domain`'s own `serde` feature, which nothing turns on. It still counts, because all of the source crate's own features are on.
