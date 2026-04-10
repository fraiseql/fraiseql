# ADR-0010: Async Mutation Handlers for Long-Running Operations

## Status: Proposed

## Context

FraiseQL mutations compile to SQL function calls (`PL/pgSQL`, `PL/MySQL`, etc.). If the work cannot be expressed as a database function, there is no mutation path today.

This rules out an entire class of operations:

- AI/ML analysis (sentiment, classification, image recognition)
- Batch processing (report generation, bulk exports, data transformation)
- External service calls (payments, third-party integrations)
- Async workers (email, SMS, webhooks)

**Current workarounds all have significant friction:**

1. **SQL mutations only**: Cannot call external services; limited to deterministic database operations
2. **Federation**: Requires standing up a GraphQL server; heavyweight for simple job enqueueing
3. **Webhooks**: Post-mutation side effects, not part of response; hard to model in GraphQL schema
4. **Custom job queue**: Users must build their own infrastructure outside FraiseQL

## Decision

Implement **Async Mutation Handlers** — a pluggable system for delegating mutations to external services while maintaining compile-time safety and observability.

### Core Design

**Schema author declares** async mutations in `fraiseql.toml`:
```toml
[[fraiseql.async_mutations.handlers]]
mutation_name = "analyzeDocument"
handler_type = "http"
endpoint = "http://worker:8000/analyze"
timeout_secs = 600
max_retries = 3
```

**Schema definition** (Python):
```python
@fraiseql.mutation(operation="analyzeDocument")
@fraiseql.field(requires_scope=["documents:analyze"])
def analyze_document(document_id: int, analysis_type: str) -> "JobHandle":
    """Enqueue document for async analysis."""
    pass
```

**Compiler validates** mutation exists, generates routing metadata in `schema.compiled.json`, and **auto-injects `JobHandle`, `JobStatus`, and `jobStatus` query** into the compiled schema — same pattern as Relay type injection (`inject_relay_types` in `converter/relay.rs`). The user never defines these types; the compiler emits them when any `async_mutations` handler is declared.

Auto-injected types:
```graphql
enum JobStatus { QUEUED, RUNNING, COMPLETED, FAILED, CANCELLED }

type JobHandle {
  jobId: String!
  status: JobStatus!
  estimatedCompletionSecs: Int
}

type Job {
  jobId: String!
  status: JobStatus!
  result: JSON        # mutation-specific payload, null until COMPLETED
  error: String       # null unless FAILED
  createdAt: String!
  updatedAt: String!
}
```

Auto-injected query:
```graphql
type Query {
  jobStatus(jobId: String!): Job
}
```

The compiler rewrites each async mutation's `return_type` from the user-declared type to `"JobHandle"`. The original return type is preserved in a new `async_result_type` field on `MutationDefinition` so the runtime knows what shape to expect when the job completes.

**Runtime executes** via pluggable `AsyncMutationHandler` trait:
```rust
pub trait AsyncMutationHandler: Send + Sync {
    async fn execute(
        &self,
        mutation_name: &str,
        input: serde_json::Value,
        context: &SecurityContext,
    ) -> Result<JobHandle>;
}
```

**Client receives** job handle immediately:
```graphql
mutation {
  analyzeDocument(documentId: 42, analysisType: "sentiment") {
    jobId
    status
    estimatedCompletionSecs
  }
}
```

Client can poll/subscribe for updates via `jobStatus(jobId)` query.

### Key Properties

1. **Compile-time safe**: Mutations declared in schema; routing known before execution
2. **Secure**: Authorization checked before delegation; SecurityContext passed (never raw credentials)
3. **Observable**: Correlation IDs, metrics, job status query
4. **Pluggable**: Trait-based; easy to implement HTTP, AMQP, custom handlers
5. **Framework-agnostic**: Handler endpoints can be any HTTP service, queue, etc.

## Consequences

### Positive Consequences

- ✅ **Closes adoption gap**: Unlocks AI, batch processing, microservice use cases
- ✅ **Maintains FraiseQL principles**: Compile-time safety, security by design, observability
- ✅ **Simple for users**: One `fraiseql.toml` section + standard Python decorator
- ✅ **Extensible**: New handler types (Redis, gRPC, custom) via trait implementations
- ✅ **Testable**: Handlers are mockable; test without external services
- ✅ **Graceful fallback**: Optional feature; runs without handlers if needed

### Negative Consequences

- ⚠️ **More moving parts**: Requires managing/monitoring external services
- ⚠️ **Job tracking complexity**: Need strategy for job storage (in-memory, Redis, database)
- ⚠️ **New error surface**: Handler timeouts, failures, retries add operational concerns
- ⚠️ **Limited immediate feedback**: Client must poll/subscribe; not synchronous
- ⚠️ **Handler versioning**: Changes to handler interface could break deployed services

## Alternatives Considered

### Alt 1: Arbitrary Command Execution (`JSON in/JSON out`)

Execute shell commands or scripts as mutations:
```toml
[[mutation.handlers]]
name = "analyzeDocument"
command = "python analyze.py"
```

**Pros**: Maximum flexibility

**Cons**: 
- ❌ **Security nightmare** — shell injection, privilege escalation, arbitrary code execution
- ❌ **Not deterministic** — can't validate at compile time
- ❌ **Unobservable** — hard to trace, monitor, debug
- ❌ **Violates FraiseQL design** — breaks auditability and safety guarantees

**Verdict**: **Rejected**. Incompatible with FraiseQL's core principles.

---

### Alt 2: Full Webhook Pattern

Every mutation has optional post-mutation webhooks:
```toml
[[mutation.handlers]]
name = "updateUser"
on_success_webhook = "http://worker:8000/post-update"
on_failure_webhook = "http://worker:8000/on-error"
```

**Pros**: Simple, works with existing mutation infrastructure

**Cons**:
- ❌ Not part of mutation response (client doesn't know about job)
- ❌ One-way communication (handler can't communicate back)
- ❌ Only useful for side effects, not for mutations that *are* the async job
- ❌ Harder to model in GraphQL schema

**Verdict**: **Partial complement**. Useful for post-mutation side effects; doesn't solve async mutation problem.

---

### Alt 3: Lazy Evaluation (`@defer`)

Return computation reference evaluated on demand:
```graphql
mutation {
  analyzeDocument(input: {...}) {
    result @defer {
      sentiment
      entities
    }
  }
}
```

**Pros**: GraphQL-native, leverages `@defer` spec

**Cons**:
- ❌ Doesn't map to "enqueue job" paradigm (fundamentally different model)
- ❌ Complexity in implementing deferred handlers
- ❌ Unclear error handling mid-defer
- ❌ GraphQL deferred syntax not mature in tooling

**Verdict**: **Rejected for primary use case**. Deferred queries optimize existing queries; don't support long-running jobs.

---

### Alt 4: Inline Job Object in Mutation Return

Async mutations return a `Job` object:
```graphql
type Job {
  id: String!
  status: JobStatus!
  result: String  # filled when complete
  error: String   # filled when failed
}

mutation {
  analyzeDocument(input: {...}): Job
}
```

**Pros**: All info in one response

**Cons**:
- ❌ Schema bloat — every async mutation has same output type
- ❌ Not extensible — hard to add mutation-specific metadata
- ❌ `result` field is `String` (opaque, loses type safety)
- ❌ Couples schema to job model

**Verdict**: **Rejected**. `JobHandle` + separate `jobStatus` query is cleaner.

---

### Alt 5: Mutation Context (Delegate from SQL)

Allow SQL mutations to call handlers:
```sql
SELECT mutation_context.enqueue_job('analyzeDocument', $input) as job_id;
```

**Pros**: Keeps everything in SQL

**Cons**:
- ❌ SQL doesn't have first-class async primitives
- ❌ Hard to implement correctly across database engines
- ❌ Handler lifecycle tied to transaction (risky)
- ❌ Still need to return `JobHandle` from SQL result

**Verdict**: **Rejected**. Mixing async/sync contexts in SQL is error-prone.

---

## Implementation Plan

### Phase 1: MVP (v2.3.0)

**Core functionality:**
- [ ] `AsyncMutationHandler` trait in `fraiseql-core`
- [ ] `HttpAsyncMutationHandler` implementation
- [ ] `[fraiseql.async_mutations]` config in compiler
- [ ] Schema compilation includes `async_handler_registry`
- [ ] Mutation router delegates to handlers
- [ ] `JobHandle` and `JobStatus` types in schema
- [ ] `jobStatus(jobId: String!): Job` query for polling
- [ ] Integration tests with mock HTTP handler
- [ ] Documentation + examples (AI analysis, email, reports)

**Timeline**: 2-3 weeks focused development

### Phase 2: Robustness (v2.4.0)

- [ ] AMQP handler (RabbitMQ, etc.)
- [ ] Webhook handler with retry logic
- [ ] Job result caching (Redis-backed)
- [ ] Idempotency keys
- [ ] Circuit breaker for handler failures
- [ ] Structured logging with correlation IDs

### Phase 3: Advanced (v2.5.0+)

- [ ] Subscription support for job updates
- [ ] DLQ for failed jobs
- [ ] Job history and audit logging
- [ ] Rate limiting per mutation type

## Security Considerations

| Threat | Mitigation |
|--------|-----------|
| **SSRF** (handler endpoint injection) | Validate endpoint URL at compile time; block localhost in production |
| **Credential exposure** | Pass `SecurityContext` {user_id, scopes, org_id} only; never raw credentials |
| **Authorization bypass** | Check `required_scopes` before delegation |
| **Handler DoS** | Timeout + `max_retries` prevents runaway handlers |
| **Arbitrary code execution** | No shell commands; only trait-based handlers |
| **Handler tampering** | Optional HMAC-signed requests (Phase 2) |

## Resolved Design Decisions

1. **Type injection**: `JobHandle`, `JobStatus` enum, `Job` type, and `jobStatus` query are compiler-injected (same pattern as Relay types). The user never defines them.
2. **Return type rewriting**: Compiler rewrites async mutation `return_type` → `"JobHandle"`, stores original in `async_result_type` for job completion payloads.
3. **Job storage backend**: Pluggable `JobStore` trait. MVP ships with in-memory (`DashMap`-backed, consistent with existing server deps). Redis and database backends in Phase 2.
4. **Authorization model**: `SecurityContext` passed to handler; handler is responsible for domain-level authorization. Framework enforces `requires_scope` before delegation.

## Open Questions (For Later)

1. **Job persistence TTL** (fixed, configurable per mutation?) → 7-day default, configurable
2. **Handler communication** (polling, callback, subscription?) → Start with polling
3. **Timeout behavior** (fail fast or pending?) → Return error with recommendation to poll
4. **Handler versioning** (endpoints, compatibility?) → Recommend versioned endpoints (e.g., `/v1/jobs`)

## Testing Strategy

**Unit tests**: Mutation validation, job status resolver, handler mocks

**Integration tests**: End-to-end mutation → job submission → polling with real/mock HTTP handler

**Example test**:
```rust
#[tokio::test]
async fn test_async_mutation_enqueue() {
    let mock_handler = MockAsyncMutationHandler::new();
    let result = execute_mutation("analyzeDocument", input, &ctx, &schema, &mock_handler).await?;
    
    assert_eq!(result["jobId"], "job_123");
    assert_eq!(result["status"], "QUEUED");
}
```

## Rollout

1. **Soft launch** (next release): Merge behind `--experimental-async-mutations` flag
2. **Production ready** (v2.3.0): Remove flag, document as stable
3. **Ecosystem** (v2.4.0+): Example projects, handler implementations, adoption guide

## References

- [GraphQL Federation Spec](https://www.apollographql.com/docs/apollo-server/using-federation/introduction/)
- OWASP [SSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)
- FraiseQL Architecture: `docs/architecture/overview.md`
- Relay type injection pattern: `crates/fraiseql-cli/src/schema/converter/relay.rs`
