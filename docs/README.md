# FraiseQL Documentation

## Where to start

| If you want to… | Go to… |
|-----------------|--------|
| Understand the system at a glance | [`architecture.md`](../architecture.md) — framework map (crates, modules, data flow) |
| Read the conceptual architecture | [`docs/architecture/overview.md`](architecture/overview.md) |
| Understand the SQL compiler | [`docs/architecture/compiler.md`](architecture/compiler.md) |
| Read why design decisions were made | [`docs/adr/`](adr/) — 10 Architecture Decision Records |
| Set up a development environment | [`.claude/CLAUDE.md`](../.claude/CLAUDE.md) |
| Run the server in production | [`docs/runbooks/`](runbooks/) — 12 incident response runbooks |
| Understand the cache system | [`docs/modules/cache.md`](modules/cache.md) |
| Understand window functions | [`docs/modules/window-functions.md`](modules/window-functions.md) |
| Understand analytics fact tables | [`docs/modules/fact-table.md`](modules/fact-table.md) |
| Check database feature compatibility | [`docs/database-compatibility.md`](database-compatibility.md) |
| Review the threat model | [`docs/security/`](security/) |
| Check SLA commitments | [`docs/sla.md`](sla.md) |
| Set up MCP for AI tools | [`docs/mcp.md`](mcp.md) |
| Understand the test strategy | [`docs/testing.md`](testing.md) |
| Why FraiseQL exists | [`docs/value-proposition.md`](value-proposition.md) |

## Directory Map

```
docs/
├── README.md                    ← this file
├── adr/                         ← Architecture Decision Records (10 ADRs)
│   ├── 0001-three-layer-architecture.md
│   ├── 0002-database-driver-choices.md
│   ├── 0003-feature-flag-strategy.md
│   ├── 0004-server-crate-decomposition.md
│   ├── 0005-sdk-tier-strategy.md
│   ├── 0006-wire-protocol-justification.md
│   ├── 0007-crypto-algorithm-choices.md
│   ├── 0008-clippy-pedantic-strategy.md
│   ├── 0009-database-feature-parity.md
│   └── 0012-async-trait-retention.md
├── architecture/                ← Conceptual architecture docs
│   ├── README.md                ← Navigation guide
│   ├── overview.md              ← 3-layer model, security, error handling
│   └── compiler.md              ← GraphQL→SQL compilation pipeline
├── auth/                        ← Authentication provider guides
├── modules/                     ← Module orientation guides
│   ├── cache.md                 ← Cache sharding, TTL, cascade invalidation
│   ├── window-functions.md      ← 3-stage pipeline, dialect table
│   └── fact-table.md            ← tf_* pattern, introspection flow
├── operations/                  ← Schema lifecycle, observer idempotency
├── runbooks/                    ← 12 incident response runbooks
├── security/                    ← Threat model, complexity limits
├── database-compatibility.md    ← Feature matrix by database backend
├── fuzzing.md                   ← Fuzzing setup and targets
├── linting.md                   ← Lint policy and rationale
├── mcp.md                       ← MCP integration for AI tools
├── sla.md                       ← Service level commitments
├── testing.md                   ← Testing strategy overview
└── value-proposition.md         ← Why FraiseQL exists
```
