# fraiseql-db

Database layer for FraiseQL v2: the PostgreSQL adapter and the SQL it generates at
request time, without an ORM layer. PostgreSQL is the only backend.

## Features

- PostgreSQL adapter
- SQL generated per request from the compiled schema
- Connection pooling with configurable bounds
- Database introspection for schema discovery
- `where` operators: comparison, containment, string, null checks, arrays, pgvector,
  full-text search, network (`INET`/`CIDR`), JSONB and `ltree`

## Cargo Features

`postgres` (default), `wire-backend`, `test-postgres`

## Usage

```toml
[dependencies]
fraiseql-db = { version = "2.3", features = ["postgres"] }
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-db)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
