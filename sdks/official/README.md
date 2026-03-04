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
