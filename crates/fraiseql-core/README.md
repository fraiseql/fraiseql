# fraiseql-core

Core execution engine for FraiseQL v2. This crate implements compiled GraphQL-over-SQL execution, transforming schema definitions into optimized SQL at build time and executing GraphQL queries with minimal runtime overhead.

## Features

- GraphQL-to-SQL compilation with per-database dialect output
- Schema parsing, validation, and compiled schema format
- Query execution with multi-shard LRU caching and per-entry TTL
- Mutation support with cascade cache invalidation
- Subscription support with topic-based routing
- Row-level security integrated into query generation
- Compiled schema format for deterministic, high-performance execution

## Usage

```toml
[dependencies]
fraiseql-core = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-core)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
