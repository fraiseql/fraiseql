# Official FraiseQL SDKs

These are the **authoring-layer SDKs**. They let you define your GraphQL schema
in your language of choice and generate the `schema.json` consumed by
`fraiseql-cli compile`. No runtime Rust dependency — pure authoring tools.

## SDK inventory

| Directory | Language | Status |
|-----------|----------|--------|
| `fraiseql-python/` | Python 3.11+ | Stable |
| `fraiseql-typescript/` | TypeScript / Node.js | Stable |
| `fraiseql-java/` | Java 17+ | Stable |
| `fraiseql-go/` | Go 1.21+ | Stable |
| `fraiseql-rust/` | Rust (for Rust-authored schemas) | Stable |
| `fraiseql-php/` | PHP 8.1+ | Stable |
| `fraiseql-ruby/` | Ruby 3.2+ | Beta |
| `fraiseql-csharp/` | C# / .NET 8+ | Beta |
| `fraiseql-dart/` | Dart / Flutter | Beta |
| `fraiseql-elixir/` | Elixir | Beta |
| `fraiseql-fsharp/` | F# / .NET 8+ | Beta |

## SDK layout convention

Each SDK follows the same structure:

```
fraiseql-<lang>/
├── src/          # Source code
├── tests/        # Test suite
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

## Cross-SDK schema parity CI

`.github/workflows/sdk-parity.yml` runs on every SDK change and compares the
schema emitted by each SDK against the Python reference (see
`tests/compare_schemas.py`).

| SDK        | CI parity gate     | Mode        |
|------------|--------------------|-------------|
| Python     | reference          | —           |
| TypeScript | strict (hard fail) | full        |
| Go         | strict (hard fail) | full        |
| Rust       | soft (warn only)   | types-only¹ |
| PHP        | soft (warn only)   | full        |
| Elixir     | soft (warn only)   | full        |
| Java       | not gated          | —           |
| Ruby       | not gated          | —           |
| Dart       | not gated          | —           |
| C#         | not gated          | —           |
| F#         | not gated          | —           |

¹ The Rust SDK is RBAC-focused and does not ship query/mutation builders, so
only type names + field shapes are compared. See
`fraiseql-rust/tests/generate_parity_schema.rs`.

"Not gated" SDKs still document parity in their respective
`*-feature-parity.md` but drift is only caught in review. Adding a parity
generator + CI job for each is tracked as follow-up work.
