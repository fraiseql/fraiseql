# fraiseql-functions

Serverless functions runtime for FraiseQL. This crate provides the infrastructure for executing user-defined functions inside the FraiseQL server, with pluggable backends for WebAssembly (WASM component model) and JavaScript/TypeScript (Deno/V8).

Functions hook into FraiseQL through the `FunctionObserver` trait, integrating with `fraiseql-observers` for trigger execution (cron, before-mutation, after-mutation). Each backend is feature-gated so a deployment only compiles in what it actually runs.

## Features

- `FunctionRuntime` trait abstracting over execution backends
- `WasmRuntime` — WASM component model executor (`runtime-wasm` feature)
- `DenoRuntime` — JavaScript/TypeScript executor via V8 (`runtime-deno` feature)
- `host-live` capabilities — database and HTTP access from inside functions
- `host-storage` capabilities — object storage access from inside functions
- Cron-state migrations for scheduled function execution

## Usage

```toml
[dependencies]
fraiseql-functions = { version = "2.3", features = ["runtime-wasm"] }
```

```rust
use fraiseql_functions::{FunctionRuntime, FunctionObserver};

// Implement FunctionRuntime to register a custom backend, or enable
// the runtime-wasm / runtime-deno features for the bundled backends.
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-functions)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
