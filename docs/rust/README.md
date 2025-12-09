# FraiseQL Rust Pipeline

FraiseQL uses an exclusive Rust pipeline for all query execution, achieving 0.5-5ms response times.

## Architecture

```
PostgreSQL → Rust (fraiseql-rs) → HTTP
  (JSONB)      Transformation      (bytes)
```

## How It Works

1. **PostgreSQL** returns JSONB data
2. **Rust** transforms it:
   - snake_case → camelCase
   - Inject __typename
   - Wrap in GraphQL response structure
   - Filter fields (optional)
3. **HTTP** receives UTF-8 bytes

## Key Documents

- [Pipeline Architecture](rust-first-pipeline.md) - Technical details
- [Usage Guide](rust-pipeline-implementation-guide.md) - How to optimize queries
- [Field Projection](rust-field-projection.md) - Performance optimization

## For Contributors

The Rust code lives in `fraiseql_rs/` directory. See [Contributing Guide](../../CONTRIBUTING.md) for development setup.
