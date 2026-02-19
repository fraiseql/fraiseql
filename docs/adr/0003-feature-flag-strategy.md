# ADR-0003: Cargo Feature Flags for Optional Subsystems

## Status: Accepted

## Context

FraiseQL includes optional features not needed by all deployments: Arrow Flight for columnar exports, Prometheus metrics, distributed tracing, Redis caching, webhook delivery. Including all dependencies by default increases binary size (4-8 MB impact) and startup time. Different deployments have different requirements (embedded vs. cloud, observability-heavy vs. minimal).

## Decision

Use Cargo feature flags for conditional compilation of optional subsystems:

- `fraiseql-arrow`: Arrow Flight server
- `fraiseql-redis`: Redis caching backend
- `fraiseql-tracing`: OpenTelemetry integration
- `fraiseql-webhooks`: Webhook delivery and retry
- `fraiseql-metrics`: Prometheus metrics exporter

Default features: minimal (core only). Users opt in via `Cargo.toml` features.

## Consequences

**Positive:**
- Minimal binaries for basic deployments
- Clear dependency management
- Compile-time dead code elimination

**Negative:**
- Feature matrix increases test matrix
- Default-disabled features may bitrot if not tested
- Users must understand feature enablement

## Alternatives Considered

1. **Runtime configuration only**: Features always compiled; bloats all binaries
2. **Separate binaries**: `fraiseql-core`, `fraiseql-full`, etc.; distribution complexity
3. **Plugin system**: Runtime plugins; defeats compile-time optimization
