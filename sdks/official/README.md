# Official FraiseQL SDKs

These are the **authoring-layer SDKs**. They let you define your GraphQL schema
in your language of choice and generate the `schema.json` consumed by
`fraiseql-cli compile`. No runtime Rust dependency — pure authoring tools.

## SDK inventory

**Distribution.** Three SDKs are published to a registry and versioned in lockstep with
the engine; the other eight are source-only and are used by vendoring the directory. That
is a statement about distribution, not quality — most source-only SDKs out-score published
Rust, which is deliberately field-level-RBAC focused. See
[ADR-0019](../../docs/adr/0019-sdk-publication-boundary.md), and
`make lint-sdk-publication-claims` for the gate that keeps this column honest.


Conformance is measured by [`conformance/`](conformance/README.md): the canonical schema
is authored through each SDK's **public API**, compiled by the real CLI, and the compiled
result is compared against a shared expectation. The score is constructs satisfied out of
constructs in the fixture; a gap is declared in `conformance/manifest.json` with the
reason shown here, and the suite fails if a declared gap is no longer true.

| Directory | Language | Distribution | Conformance | Declared gaps |
|-----------|----------|--------------|-------------|---------------|
| `fraiseql-python/` | Python 3.11+ | PyPI | 24/24 | — |
| `fraiseql-typescript/` | TypeScript / Node.js | npm | 24/24 | — |
| `fraiseql-go/` | Go 1.23+ | — (source-only) | 23/24 | type_crud: the SDK ships no CRUD generator |
| `fraiseql-php/` | PHP 8.2+ | — (source-only) | 24/24 | — |
| `fraiseql-java/` | Java 21+ | — (source-only) | 24/24 | — |
| `fraiseql-csharp/` | C# / .NET 8+ | — (source-only) | 23/24 | subscriptions: the SDK ships no subscription authoring surface at all (#1024) |
| `fraiseql-fsharp/` | F# / .NET 8+ | — (source-only) | 23/24 | subscriptions: the SDK ships no subscription authoring surface at all (#1024) |
| `fraiseql-elixir/` | Elixir | — (source-only) | 23/24 | subscriptions: the SDK ships no subscription authoring surface at all (#1024) |
| `fraiseql-ruby/` | Ruby 3.2+ | — (source-only) | 23/24 | subscriptions: the SDK ships no subscription authoring surface at all (#1024) |
| `fraiseql-dart/` | Dart / Flutter | — (source-only) | 23/24 | subscriptions: the SDK ships no subscription authoring surface at all (#1024) |
| `fraiseql-rust/` | Rust | crates.io | 5/24 | queries, mutations, subscriptions, enums, input types, CRUD, relationships, Relay and error-type flags: the Rust SDK is field-level-RBAC focused and ships no builder for them |

The scores and gap counts above are checked against `conformance/manifest.json` by
`selftest.py`, because they had already drifted: the table read `19/19` when the fixture
carried 23 constructs, and Go's `type_crud` gap was not listed at all. A support matrix
nobody checks is a published claim about eleven SDKs that decays every time a construct is
added.

The TypeScript decorators `@Type`, `@Query`, `@Mutation` and `@Subscription` are **not**
an authoring path — TypeScript erases the types they would need, so they refuse rather
than register the placeholders they used to. Use `registerTypeFields` / `registerQuery` /
`registerMutation`.

## SDK layout convention

Each SDK follows the same structure:

```
fraiseql-<lang>/
├── src/          # Source code
├── tests/        # Test suite
├── conformance/  # Cross-SDK conformance exporter (see ../conformance/)
├── examples/     # Usage examples
├── README.md     # Language-specific usage guide
└── <manifest>    # pyproject.toml / package.json / pom.xml / go.mod / …
```

Build artifacts (`dist/`, `target/`, `node_modules/`, `.venv/`) are gitignored.

## Relationship to the Rust engine

```
SDK (authoring)          CLI (compilation)        Server (runtime)
fraiseql-python/   →  fraiseql-cli compile  →  fraiseql-server
fraiseql-typescript/     schema.json                schema.compiled.json
…                        + fraiseql.toml            loaded at startup
```

The SDKs produce `schema.json`. The CLI validates and compiles it to
`schema.compiled.json`. The server loads the compiled schema at startup —
no SDK dependency at runtime.

The format is specified in
[`docs/architecture/intermediate-schema.md`](../../docs/architecture/intermediate-schema.md),
whose worked examples are the conformance fixtures themselves, compiled on every CI run.

## CI

| Workflow | What it gates |
|---|---|
| `.github/workflows/sdk-conformance.yml` | The table above: every SDK authors the canonical schema, compiles it, and must preserve what was declared. Hard gate, no skips. |
| `.github/workflows/sdk-parity.yml` | Cross-SDK agreement on the emitted `schema.json`, against the Python reference. |
| `.github/workflows/<lang>-sdk.yml` | Each SDK's own unit tests and lints. |
