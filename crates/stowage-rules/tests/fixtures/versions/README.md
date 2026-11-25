# versions

Two versions of the same external crate, `codec`, so reports have to tell them apart.

```text
app ──┬──► codec 1.0.0
      └──► codec-compat ──► codec 2.0.0

lib ─────► codec-compat ──► codec 2.0.0
```

`app` reaches both versions: `codec 1.0.0` directly, and `codec 2.0.0` through `codec-compat`. Every other crate name appears once.
