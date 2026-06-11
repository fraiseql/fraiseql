# Operations Documentation

Guides for running FraiseQL in production.

| Document | Purpose |
|----------|---------|
| [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md) | How `schema.compiled.json` moves from CI to production, sensitivity classification, deployment options |
| [zero-downtime-deploys.md](zero-downtime-deploys.md) | Rolling / blue-green / canary deploys behind a load balancer; expand-contract migrations; graceful drain; why one schema per process |
| [observer-idempotency.md](observer-idempotency.md) | `EffectivelyOnce` checkpoint table schema, growth rates, cleanup strategy, failure modes |
