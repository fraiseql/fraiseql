# fraiseql-db

Database abstraction layer for FraiseQL v2. This crate provides runtime SQL generation and database adapters for multiple backends, enabling FraiseQL to target different databases without an ORM layer.

## Features

- PostgreSQL (primary), MySQL, SQLite, and SQL Server adapters
- Runtime SQL generation tailored to each backend's dialect
- Connection pooling with configurable bounds
- Database introspection for schema discovery
- Collation configuration per database backend
- Rich filter operators for advanced query predicates

## Cargo Features

`postgres` (default), `mysql`, `sqlite`, `sqlserver`, `wire-backend`, `rich-filters`

## Usage

```toml
[dependencies]
fraiseql-db = { version = "2.1.0", features = ["postgres"] }
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-db)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
