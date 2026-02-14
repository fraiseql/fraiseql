# fraiseql

FraiseQL v2 - Compiled GraphQL execution engine for relational databases.

## Installation

```toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["server"] }
```

## Quick Start

```rust
use fraiseql::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load compiled schema
    let schema = CompiledSchema::from_file("schema.compiled.json")?;

    // Connect to database
    let db = Arc::new(db::PostgresAdapter::new("postgresql://localhost/mydb").await?);

    // Start server
    let config = ServerConfig::from_file("fraiseql.toml")?;
    let server = Server::new(config, schema, db, None).await?;
    server.serve().await
}
```

## Feature Flags

- `postgres` (default) - PostgreSQL support
- `mysql` - MySQL support
- `sqlite` - SQLite support
- `server` - HTTP server
- `observers` - Reactive business logic
- `arrow` - Apache Arrow Flight
- `wire` - Streaming queries
- `full` - All features

## Documentation

See [docs.fraiseql.dev](https://docs.fraiseql.dev) for full documentation.

## Migration from Individual Crates

Before:
```rust
use fraiseql_core::{CompiledSchema, runtime::Executor};
use fraiseql_server::Server;
```

After:
```rust
use fraiseql::prelude::*;
use fraiseql::server::Server;
```

Individual crates remain available for advanced use cases.
