# fraiseql-error

Error types and result aliases for the FraiseQL v2 ecosystem. This crate provides a unified error hierarchy used across all FraiseQL crates, ensuring consistent error handling from schema compilation through query execution.

## Features

- Typed error variants for each failure domain: Parse, Validation, Database, Configuration, and more
- `Result<T>` type alias for concise signatures across the workspace
- `ErrorContext` for chaining errors with additional context as they propagate
- `ValidationFieldError` for field-level validation errors with path information

## Usage

```toml
[dependencies]
fraiseql-error = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-error)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
