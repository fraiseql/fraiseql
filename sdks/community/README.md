# Community SDKs

This directory contains **nine community-contributed SDKs** for FraiseQL. These SDKs are
**experimental and unmaintained** — they are provided as reference implementations only.

## Status: Experimental / Unmaintained

None of the SDKs in this directory are officially supported by the FraiseQL maintainers.
They are not tested in CI and may be out of date with the current FraiseQL API.

For supported, production-ready SDKs see [`../official/`](../official/).

## Removal Timeline

These community SDKs will be **removed in FraiseQL v3.0.0, or after 12 months from the
v2.0.0 release (whichever comes first)**.

If you rely on one of these SDKs and would like to adopt maintenance responsibility, please
open an issue in the main repository.

## SDK List

| SDK | Language | Notes |
|-----|----------|-------|
| `fraiseql-clojure` | Clojure | Experimental |
| `fraiseql-dart` | Dart | Duplicated in `official/` — community version is older |
| `fraiseql-elixir` | Elixir | Duplicated in `official/` — community version is older |
| `fraiseql-groovy` | Groovy | Experimental |
| `fraiseql-kotlin` | Kotlin | Experimental |
| `fraiseql-nodejs` | Node.js / JavaScript | Experimental |
| `fraiseql-ruby` | Ruby | Duplicated in `official/` — community version is older |
| `fraiseql-scala` | Scala | Experimental |
| `fraiseql-swift` | Swift | Experimental |

## Deduplication Note (AB2)

Three SDKs in this directory (`fraiseql-dart`, `fraiseql-elixir`, `fraiseql-ruby`) also
exist under `../official/`. The official variants are actively maintained and should be
preferred. The community copies here are retained only until the removal deadline above, at
which point they will be deleted to avoid confusion.

## Using an Official SDK

```
sdks/official/
├── fraiseql-dart/
├── fraiseql-elixir/
├── fraiseql-go/
├── fraiseql-java/
├── fraiseql-python/
├── fraiseql-ruby/
├── fraiseql-rust/
├── fraiseql-typescript/
└── ...
```

Use the official SDK for your language if it is available. The official SDKs follow semantic
versioning, are tested against the FraiseQL test suite, and receive security patches.
