# FraiseQL v2 Architecture

Complete architectural documentation for FraiseQL v2.

---

## 🚀 NEW: Rust Core Implementation Architecture

**Complete Rust implementation design for Phases 2-5**

| Document | Description | Lines | Status |
|----------|-------------|-------|--------|
| **[RUST_CORE_ARCHITECTURE.md](RUST_CORE_ARCHITECTURE.md)** | Complete core runtime architecture | 1,500+ | ✅ Ready |
| **[CODE_EXAMPLES.md](CODE_EXAMPLES.md)** | Runnable code examples | 800+ | ✅ Ready |
| **[ADVANCED_FEATURES_ARCHITECTURE.md](ADVANCED_FEATURES_ARCHITECTURE.md)** | Federation, RBAC, subscriptions, observability | 1,200+ | ✅ Ready |
| **[IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md)** | Day-by-day implementation plan | 1,000+ | ✅ Ready |

**Core Features Designed:**

- ✅ Database abstraction layer (PostgreSQL, MySQL, SQLite, SQL Server)
- ✅ WHERE clause generation (50+ operators, SQL injection proof)
- ✅ JSONB projection (recursive, auth-aware)
- ✅ Field-level authorization
- ✅ Connection pooling (deadpool)
- ✅ Caching (LRU + Redis)

**Advanced Features Designed:**

- ✅ **Federation** - Apollo Federation v2 with view-based protocol
- ✅ **RBAC** - Hierarchical roles with permission caching
- ✅ **Subscriptions** - Database-driven event streams (LISTEN/NOTIFY)
- ✅ **Observability** - Metrics, traces, logs via middleware
- ✅ **Extension Points** - Custom auth rules, validators, hooks

**Start implementing:** [IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md) → Phase 2 (Database Layer)

---

## 📁 Directory Structure

### [Core Pipeline](core/)

Fundamental compilation and execution architecture.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [compilation-pipeline.md](core/compilation-pipeline.md) | 7-phase compilation process | 1,024 | 60 min |
| [compilation-phases.md](core/compilation-phases.md) | Deep dive into each compilation phase | 1,597 | 50 min |
| [compilation-vs-runtime.md](core/compilation-vs-runtime.md) | Separation of concerns | 425 | 15 min |
| [execution-model.md](core/execution-model.md) | 6-phase runtime execution | 1,235 | 90 min |
| [execution-semantics.md](core/execution-semantics.md) | Detailed execution semantics | 1,488 | 60 min |
| [authoring-languages.md](core/authoring-languages.md) | Multi-language schema authoring | 903 | 30 min |

**Start here:** [compilation-pipeline.md](core/compilation-pipeline.md) → [execution-model.md](core/execution-model.md)

---

### [Database Integration](database/)

Multi-database support and data plane architecture.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [database-targeting.md](database/database-targeting.md) | Compile-time database specialization | 644 | 30 min |
| [arrow-plane.md](database/arrow-plane.md) | Columnar data processing with Apache Arrow | 1,756 | 60 min |

**Start here:** [database-targeting.md](database/database-targeting.md)

---

### [Reliability](reliability/)

Consistency, error handling, failure recovery, and versioning.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [consistency-model.md](reliability/consistency-model.md) | Transaction guarantees and consistency | 724 | 30 min |
| [error-handling-model.md](reliability/error-handling-model.md) | Error propagation and recovery | 954 | 40 min |
| [failure-modes-and-recovery.md](reliability/failure-modes-and-recovery.md) | Failure scenarios and recovery strategies | 1,136 | 50 min |
| [versioning-strategy.md](reliability/versioning-strategy.md) | Schema evolution and version management | 1,557 | 60 min |

**Start here:** [consistency-model.md](reliability/consistency-model.md)

---

### [Security](security/)

Security architecture and authentication.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [security-model.md](security/security-model.md) | Overall security architecture | 1,131 | 50 min |
| [authentication-detailed.md](security/authentication-detailed.md) | Authentication flows and providers | 1,853 | 70 min |

**Start here:** [security-model.md](security/security-model.md)

---

### [Performance](performance/)

Performance optimization and tuning.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [performance-characteristics.md](performance/performance-characteristics.md) | Performance benchmarks and guarantees | 977 | 40 min |
| [advanced-optimization.md](performance/advanced-optimization.md) | Database tuning and optimization strategies | 1,483 | 60 min |

**Start here:** [performance-characteristics.md](performance/performance-characteristics.md)

---

### [Integration](integration/)

Federation, multi-plane architecture, and extension points.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [federation.md](integration/federation.md) | GraphQL Federation v2 architecture | 2,537 | 90 min |
| [multiplane-interactions.md](integration/multiplane-interactions.md) | Query/Command/Data plane coordination | 793 | 35 min |
| [extension-points.md](integration/extension-points.md) | Plugin and extension architecture | 783 | 35 min |
| [integration-patterns.md](integration/integration-patterns.md) | Common integration scenarios | 724 | 30 min |

**Start here:** [federation.md](integration/federation.md)

---

### [Real-time](realtime/)

Subscriptions and event streaming.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [subscriptions.md](realtime/subscriptions.md) | Database-native event streams | 1,618 | 70 min |

**Start here:** [subscriptions.md](realtime/subscriptions.md)

---

### [Design Decisions](decisions/)

Architectural patterns and anti-patterns.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [design-decisions.md](decisions/design-decisions.md) | Key architectural decisions and rationale | 973 | 40 min |
| [anti-patterns.md](decisions/anti-patterns.md) | Common mistakes and how to avoid them | 819 | 35 min |
| [state-management.md](decisions/state-management.md) | State management patterns | 694 | 30 min |

**Start here:** [design-decisions.md](decisions/design-decisions.md)

---

### [Observability](observability/)

Monitoring, logging, and instrumentation.

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [observability-model.md](observability/observability-model.md) | Metrics, traces, logs architecture | 1,369 | 50 min |

**Start here:** [observability-model.md](observability/observability-model.md)

---

## 🎯 Recommended Reading Order

**For System Understanding:**

1. Core: compilation-pipeline.md
2. Core: execution-model.md
3. Database: database-targeting.md
4. Security: security-model.md

**For Deep Dive:**

1. Core: compilation-phases.md
2. Core: execution-semantics.md
3. Integration: federation.md
4. Real-time: subscriptions.md

**For Production:**

1. Reliability: consistency-model.md
2. Reliability: failure-modes-and-recovery.md
3. Performance: advanced-optimization.md
4. Observability: observability-model.md

---

**Back to:** [Documentation Home](../README.md) | [Reading Order Guide](../reading-order.md)
