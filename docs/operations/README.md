# Operations Documentation

Guides for running FraiseQL in production.

| Document | Purpose |
|----------|---------|
| [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md) | How `schema.compiled.json` moves from CI to production, sensitivity classification, deployment options |
| [zero-downtime-deploys.md](zero-downtime-deploys.md) | Rolling / blue-green / canary deploys behind a load balancer; expand-contract migrations; graceful drain; why one schema per process |
| [observer-idempotency.md](observer-idempotency.md) | `EffectivelyOnce` checkpoint table schema, growth rates, cleanup strategy, failure modes |
| [read-replicas.md](read-replicas.md) | Serving compiled queries from PostgreSQL replicas: static read/write routing, the read-your-writes pin window, failure behaviour, tenant isolation on every pool |
| [graphql-sse-streaming.md](graphql-sse-streaming.md) | GraphQL responses over Server-Sent Events: negotiation, root-field `@stream` incremental delivery, per-batch auth re-checks, consistency caveats |
