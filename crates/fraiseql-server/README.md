# fraiseql-server

HTTP server for the FraiseQL v2 GraphQL engine. This crate provides a production-ready server that loads a compiled schema and serves GraphQL queries over REST and gRPC transports, with built-in security and observability.

## Features

- Generic `Server<DatabaseAdapter>` for type-safe database swapping and testing
- REST and gRPC transports
- PKCE OAuth and OIDC/JWKS authentication
- Configurable rate limiting with sliding window enforcement
- Audit logging for compliance and access tracking
- Error sanitization to prevent implementation detail leakage
- Automatic Persisted Queries (APQ) for repeated query optimization
- Health check endpoints

## Usage

```toml
[dependencies]
fraiseql-server = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-server)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
