# fraiseql-server

HTTP server for the FraiseQL v2 compiled-query engine. Loads a `schema.compiled.json` artifact and exposes the same compiled query surface over three transports: GraphQL (`POST /graphql` + WebSocket subscriptions), REST (auto-generated from the schema, with OpenAPI 3.0 spec), and Arrow Flight gRPC (analytical bulk delivery, optional). Auth, rate limiting, sanitisation, metrics, and APQ are shared across all three transports.

## Features

- Generic `Server<DatabaseAdapter>` for type-safe database swapping and testing
- Three transports on the same query surface: GraphQL, REST (with OpenAPI), and Arrow Flight gRPC (optional)
- PKCE OAuth and OIDC/JWKS authentication
- Configurable rate limiting with sliding window enforcement
- Audit logging for compliance and access tracking
- Error sanitization to prevent implementation detail leakage
- Automatic Persisted Queries (APQ) for repeated query optimization
- Health check endpoints

## Usage

```toml
[dependencies]
fraiseql-server = "2.3"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-server)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
