# fraiseql-federation

Apollo Federation v2 support for FraiseQL, providing entity resolution, saga orchestration, and multi-subgraph composition. This crate enables FraiseQL to participate in a federated graph as a subgraph, resolving entities and coordinating queries across distributed services.

## Features

- Apollo Federation v2 compliant
- Entity resolution and representation handling
- Multi-subgraph query planning
- Saga orchestration
- Circuit breaker for downstream services
- SSRF protection for service URLs

## Usage

```toml
[dependencies]
fraiseql-federation = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-federation)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
