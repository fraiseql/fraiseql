# fraiseql-federation

Apollo Federation v2 support for FraiseQL, providing entity resolution, saga orchestration, and multi-subgraph composition. This crate enables FraiseQL to participate in a federated graph as a subgraph, resolving entities and coordinating queries across distributed services.

## Features

- Apollo Federation v2 compliant
- Entity resolution and representation handling
- Multi-subgraph query planning
- Saga orchestration for cross-subgraph transactions (opt-in `saga` feature)
- Circuit breaker for downstream services
- SSRF protection for service URLs

## Usage

```toml
[dependencies]
fraiseql-federation = "2.3"
```

Distributed sagas (forward + remote HTTPS dispatch, compensation, concurrency-safe
recovery, mTLS, retry/backoff, and `@requires` pre-fetch) are a **stable, opt-in** API
behind the `saga` Cargo feature, so builds that don't orchestrate cross-subgraph
transactions aren't forced to compile the Postgres saga store:

```toml
[dependencies]
fraiseql-federation = { version = "2.3", features = ["saga"] }
```

Use `SagaCoordinator` with `SagaCoordinatorStep`s.

## Documentation

- [API Documentation](https://docs.rs/fraiseql-federation)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
