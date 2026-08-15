# Official FraiseQL SDKs

These are the **authoring-layer SDKs**. They let you define your GraphQL schema
in your language of choice and generate the `schema.json` consumed by
`fraiseql-cli compile`. No runtime Rust dependency — pure authoring tools.

## SDK inventory

Conformance is measured by [`conformance/`](conformance/README.md): the canonical schema
is authored through each SDK's **public API**, compiled by the real CLI, and the compiled
result is compared against a shared expectation. The score is constructs satisfied out of
constructs in the fixture; a gap is declared in `conformance/manifest.json` with the
reason shown here, and the suite fails if a declared gap is no longer true.

| Directory | Language | Conformance | Declared gaps |
|-----------|----------|-------------|---------------|
| `fraiseql-python/` | Python 3.11+ | 17/17 | — |
| `fraiseql-typescript/` | TypeScript / Node.js | 17/17 | — |
| `fraiseql-go/` | Go 1.23+ | 17/17 | — |
| `fraiseql-php/` | PHP 8.2+ | 17/17 | — |
| `fraiseql-java/` | Java 21+ | 17/17 | — |
| `fraiseql-csharp/` | C# / .NET 8+ | 17/17 | — |
| `fraiseql-fsharp/` | F# / .NET 8+ | 17/17 | — |
| `fraiseql-elixir/` | Elixir | 17/17 | — |
| `fraiseql-ruby/` | Ruby 3.2+ | 17/17 | — |
| `fraiseql-dart/` | Dart / Flutter | 17/17 | — |
| `fraiseql-rust/` | Rust | 4/17 | queries, mutations, enums, input types, Relay and error-type flags: the Rust SDK is field-level-RBAC focused and ships no builder for them |

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
