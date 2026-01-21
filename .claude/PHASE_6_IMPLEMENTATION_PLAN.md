# Phase 6: Observers & Events - Implementation Plan

**Status**: Planning Phase
**Target**: Complete Phase 6 (Observers & Events)
**Estimated LOC**: 7,500
**Estimated Time**: 20-25 hours (8 subphases)
**Approach**: TDD with incremental subphases

---

## Overview

Phase 6 implements the post-mutation observer system that processes side effects asynchronously after database transactions commit. Observers trigger actions (email, Slack, webhooks, etc.) based on entity lifecycle events (INSERT, UPDATE, DELETE).

### Key Objectives

1. **Event System**: PostgreSQL LISTEN/NOTIFY-based event flow
2. **Observer Pattern**: Trait-based extensible observer system
3. **Condition Evaluation**: DSL for conditional actions
4. **Action Execution**: Pluggable action implementations (webhook, email, Slack, SMS, push, search, cache)
5. **Reliability**: Retry logic, dead letter queue, backpressure handling
6. **Observability**: Structured logging, Prometheus metrics

### Dependencies

- Phase 1 (Foundation) - Configuration system
- Phase 2 (Core Runtime) - Metrics, tracing
- Phase 5 (Auth) - User context injection

---

## Architecture Overview

### Event Flow

```
1. Database mutation completes
   ↓
2. PostgreSQL pg_notify('fraiseql_events', JSON)
   ↓
3. EventListener receives NOTIFY on separate connection
   ↓
4. Event deserialized to EntityEvent
   ↓
5. Sent to bounded mpsc::channel (backpressure)
   ↓
6. ObserverExecutor processes from channel:
   - Find matching observers by (event_type, entity)
   - Evaluate condition (if present)
   - For each action: Execute with retry logic
   - Track result, log event
   ↓
7. On failure: log/alert/dlq based on FailurePolicy
```

### Backpressure Strategy

```
PostgreSQL NOTIFY → mpsc::channel (capacity: 1000) → Worker pool (concurrency: 50)

If channel full:
  - Drop new events (configurable: Drop/Block/DropOldest)
  - Log warning + update metrics
  - Send alert if backlogged > 500

If all workers busy:
  - Queue in channel
  - Block sender if policy=Block (risky with PG listener)
  - Drop excess if policy=Drop
```

### Testing Seams (Mock Implementations)

```
Traits with Mock Implementations:
  ✓ EventSource → MockEventSource (yields predefined events)
  ✓ ActionExecutor → MockActionExecutor (records executions, injectable failures)
  ✓ DeadLetterQueue → MockDeadLetterQueue (in-memory storage)
  ✓ ConditionEvaluator → MockConditionEvaluator (configurable results)
  ✓ TemplateRenderer → Re-exported from fraiseql-runtime
```

---

## Subphase Breakdown

### Subphase 6.0: Foundation & Testing Seams (2 hours)

**Objective**: Create core traits and mock implementations

**Files to Create**:
```
crates/fraiseql-observers/src/
├── lib.rs
├── error.rs              # ObserverError (14 error codes)
├── traits.rs             # Trait definitions
├── testing.rs            # Mock implementations
├── config.rs             # Configuration structs
└── event.rs              # EntityEvent types
```

**Key Deliverables**:
- ✅ `ObserverError` enum with 14 error codes (OB001-OB014)
- ✅ Trait definitions: EventSource, ActionExecutor, DeadLetterQueue, ConditionEvaluator, TemplateRenderer
- ✅ Mock implementations for all traits
- ✅ `EntityEvent`, `FieldChanges`, `EventKind` types
- ✅ Configuration structs: `ObserverRuntimeConfig`, `ObserverDefinition`, `ActionConfig`

**Cargo.toml**:
- New crate: `crates/fraiseql-observers`
- Dependencies: tokio, async-trait, sqlx, serde, tracing, uuid, chrono
- Features: `testing` (for mocks)

---

### Subphase 6.1: Event Plumbing (2 hours)

**Objective**: PostgreSQL listener and event matching

**Files to Create**:
```
crates/fraiseql-observers/src/
├── listener.rs           # PostgreSQL LISTEN/NOTIFY
├── matcher.rs            # Event-to-observer matching
└── tests/listener_test.rs
```

**Key Deliverables**:
- ✅ `EventListener` - Connects to PostgreSQL, receives NOTIFY
- ✅ Backpressure handling (Drop/Block/DropOldest policies)
- ✅ Event deserialization from JSON
- ✅ EventMatcher - Maps events to observers
- ✅ Health tracking (connected, last_event_time)

---

### Subphase 6.2: Condition Evaluator (3 hours)

**Objective**: Implement condition evaluation DSL

**Files to Create**:
```
crates/fraiseql-observers/src/
├── condition.rs          # Condition parser and evaluator
└── tests/condition_test.rs
```

**Key Deliverables**:
- ✅ Condition DSL parser (regex-based tokenization)
- ✅ Operators: `==`, `!=`, `>`, `<`, `>=`, `<=`, `&&`, `||`, `!`
- ✅ Nested field access: `customer.tier`
- ✅ Special functions: `status_changed_to()`, `field_changed()`, `is_new()`, `is_deleted()`
- ✅ 7+ test scenarios

---

### Subphase 6.3: Core Actions (4 hours)

**Objective**: Implement webhook, Slack, and email actions

**Files to Create**:
```
crates/fraiseql-observers/src/actions/
├── mod.rs                # Action trait and registry
├── webhook.rs            # HTTP webhook action
├── slack.rs              # Slack message action
├── email.rs              # Email action
└── tests/
```

**Key Deliverables**:
- ✅ `Action` trait with `execute()` method
- ✅ `WebhookAction` - POST to URL with template rendering
- ✅ `SlackAction` - Send to Slack webhook with formatted messages
- ✅ `EmailAction` - SMTP integration with template rendering
- ✅ `ActionFactory` for creating action instances
- ✅ Template rendering support (Jinja-style)

---

### Subphase 6.4: Executor & Orchestration (4 hours)

**Objective**: Core execution engine with retry and DLQ

**Files to Create**:
```
crates/fraiseql-observers/src/
├── executor.rs           # ObserverExecutor (orchestration)
├── retry.rs              # Retry logic with backoff
├── dlq.rs                # Dead Letter Queue
└── tests/
```

**Key Deliverables**:
- ✅ `ObserverExecutor` - Orchestrates observer execution
- ✅ Condition evaluation (skip action if condition false)
- ✅ `RetryExecutor` - Retry with backoff strategies
- ✅ Backoff types: Exponential, Linear, Fixed
- ✅ `DeadLetterQueue` - PostgreSQL storage for failed actions
- ✅ Metrics collection (`ObserverMetrics`)
- ✅ Integration tests (15+ scenarios)

---

### Subphase 6.5: Additional Actions (3 hours)

**Objective**: SMS, Push, Search, Cache action stubs

**Files to Create**:
```
crates/fraiseql-observers/src/actions/
├── sms.rs                # SMS action (stub)
├── push.rs               # Push notification (stub)
├── search.rs             # Search index update
└── cache.rs              # Cache invalidation
```

**Key Deliverables**:
- ✅ `SmsAction` - Stub implementation
- ✅ `PushAction` - Stub implementation
- ✅ `SearchAction` - Elasticsearch integration stub
- ✅ `CacheAction` - Redis invalidation stub
- ✅ All registered in `ActionFactory`

---

### Subphase 6.6: Database & Schema (1 hour)

**Objective**: Database tables and migrations

**Files to Create**:
```
crates/fraiseql-observers/migrations/
├── 001_create_observer_events.sql
├── 002_create_observer_dlq.sql
└── 003_create_system_functions.sql
```

**Key Deliverables**:
- ✅ `_system.observer_events` table (event logging)
- ✅ `_system.observer_dlq` table (dead letter queue)
- ✅ Indexes for performance
- ✅ `_system.emit_event()` PostgreSQL function
- ✅ Migration support during server startup

---

### Subphase 6.7: Comprehensive Tests (6 hours)

**Objective**: Full unit, integration, and E2E tests

**Test Coverage Goals**:
- ✅ 30+ condition evaluation scenarios
- ✅ 20+ executor scenarios
- ✅ 15+ listener scenarios
- ✅ 10+ retry scenarios
- ✅ 10+ integration scenarios
- **Total**: 60+ tests, 90%+ coverage

---

### Subphase 6.8: Documentation & Polish (2 hours)

**Objective**: Complete documentation and API finalization

**Deliverables**:
- ✅ API reference documentation
- ✅ Architecture guide
- ✅ Configuration examples
- ✅ Troubleshooting guide
- ✅ Public API finalization
- ✅ Code quality polish

---

## Implementation Approach

### Phases Are TDD (Test-Driven Development)

Each subphase follows this pattern:

1. **RED**: Write failing tests first
2. **GREEN**: Implement minimal code to pass tests
3. **REFACTOR**: Clean up code, remove duplication
4. **QA**: Verify no regressions

### Code Organization

```
crates/fraiseql-observers/        ← New crate
├── Cargo.toml
├── src/
│   ├── lib.rs                     # Public API exports
│   ├── error.rs                   # Error types
│   ├── event.rs                   # Event data structures
│   ├── traits.rs                  # Trait definitions
│   ├── testing.rs                 # Mock implementations
│   ├── config.rs                  # Configuration
│   ├── listener.rs                # PostgreSQL listener
│   ├── matcher.rs                 # Event matching
│   ├── condition.rs               # Condition evaluation
│   ├── executor.rs                # Main orchestration
│   ├── retry.rs                   # Retry logic
│   ├── dlq.rs                     # Dead letter queue
│   └── actions/
│       ├── mod.rs
│       ├── webhook.rs
│       ├── slack.rs
│       ├── email.rs
│       ├── sms.rs
│       ├── push.rs
│       ├── search.rs
│       └── cache.rs
├── tests/
│   ├── condition_test.rs
│   ├── executor_test.rs
│   ├── listener_test.rs
│   ├── retry_test.rs
│   └── integration_test.rs
└── migrations/
    ├── 001_create_observer_events.sql
    ├── 002_create_observer_dlq.sql
    └── 003_create_system_functions.sql
```

---

## Technology Stack

### Core Dependencies

- tokio (async runtime)
- async-trait (async trait methods)
- sqlx (database)
- serde/serde_json (serialization)
- tracing (observability)
- uuid, chrono (data types)
- thiserror (error handling)
- regex (condition DSL parsing)
- reqwest (HTTP for webhooks)

---

## Integration Points

### With Phase 1 (Foundation)

- Load observer config from `fraiseql.toml`
- Use same configuration system
- Graceful shutdown coordination

### With Phase 2 (Core Runtime)

- Register Prometheus metrics
- Use tracing for structured logging
- Rate limiting coordination

### With Phase 5 (Auth)

- Inject `user_id` from auth context into `EntityEvent`
- Template rendering can access `{{ user_id }}`

### Server (lib.rs)

- Optional observer executor initialization
- Background event listener start
- Graceful shutdown drains pending events

---

## Success Criteria

### Code Quality
- ✅ 90%+ code coverage (60+ tests)
- ✅ All public APIs documented
- ✅ No clippy warnings
- ✅ No unsafe code

### Functionality
- ✅ Events flow correctly through system
- ✅ Conditions evaluate accurately
- ✅ Actions execute with retry
- ✅ Failed actions go to DLQ
- ✅ Backpressure works
- ✅ Metrics track correctly

### Integration
- ✅ Works with Phase 1, 2, 5
- ✅ Server starts without errors
- ✅ Graceful shutdown

### Documentation
- ✅ Complete API reference
- ✅ Architecture guide
- ✅ Configuration examples

---

## Estimated Time Breakdown

| Subphase | Component | Hours | Status |
|----------|-----------|-------|--------|
| 6.0 | Foundation | 2 | Planned |
| 6.1 | Event Plumbing | 2 | Planned |
| 6.2 | Condition Evaluator | 3 | Planned |
| 6.3 | Core Actions | 4 | Planned |
| 6.4 | Executor & Orchestration | 4 | Planned |
| 6.5 | Additional Actions | 3 | Planned |
| 6.6 | Database & Schema | 1 | Planned |
| 6.7 | Comprehensive Tests | 6 | Planned |
| 6.8 | Documentation & Polish | 2 | Planned |
| **Total** | | **27** | |

---

## Next Steps After Phase 6

**Phase 7**: Notifications & Delivery (multi-channel)
**Phase 8**: Advanced Features (search, caching, jobs)
**Phase 9**: Interceptors (WASM/Lua customization)
**Phase 10**: Polish & Performance

---

**Status**: Ready for implementation
**Date**: 2026-01-21
**Confidence**: High (detailed specification in docs/endpoint-runtime/06-PHASE-6-OBSERVERS.md)
