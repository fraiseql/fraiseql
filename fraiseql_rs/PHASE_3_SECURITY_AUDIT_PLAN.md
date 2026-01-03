# Phase 3: Security Audit - Revised Implementation Plan

**Status**: PLANNING + ANALYSIS COMPLETE
**Objective**: Address actual security gaps for federation + JSONB architecture
**Target Production Grade**: A+ (95%+)

---

## ARCHITECTURE ANALYSIS FINDINGS

### What IS Already Secure ✅
- **Network isolation** - Each federation subgraph runs as separate service
- **SQL parameterization** - psycopg handles all queries with $1, $2 placeholders
- **Schema-level access control** - GraphQL types define what's visible
- **JSONB view structure** - tv_* views hide internal fields
- **Rust type safety** - No string parsing, zero-copy transformations

### What IS MISSING ⚠️ (Critical Gaps)
1. **User-level row filtering** - No automatic user_id/tenant_id checks on subscription events
2. **Federation context isolation** - Subscriptions don't verify subgraph ownership
3. **Multi-tenant enforcement** - Tenant_id exists but isn't automatically enforced
4. **Subscription scope verification** - No checks that users can access subscribed events
5. **RBAC integration** - PermissionResolver exists in Rust but not hooked to subscriptions

### What Is REDUNDANT (Skip)
- JWT token validation (already handled by FastAPI auth_handler)
- SQL injection prevention (already handled by psycopg)
- Field existence checks (already handled by GraphQL schema)
- Network isolation (already handled by separate services)

---

## CRITICAL SECURITY GAPS TO ADDRESS

### 1. ROW-LEVEL FILTERING ON SUBSCRIPTION EVENTS (CRITICAL)
**Issue**: Subscriptions yield all matching events, no user filtering
**Current State**: User A subscribes to events, receives all users' data
**Root Cause**: JSONB views contain complete data, no automatic row filtering
**Files to Modify**: `executor.rs`, `event_bus.rs`, event bus implementations
**Implementation Steps**:
1. Add user_id + tenant_id to subscription context
2. Implement pre-yield filtering in subscription resolver
3. Check user_id/tenant_id before publishing events
4. Support `@filter()` decorator for row-level filtering

**Example**:
```rust
// Before: Yields all orders
async fn subscribe_orders(context: &SubscriptionContext) -> EventStream {
    // Should filter by context.user_id
}

// After: Filters by user
if order.user_id == context.user_id {
    yield order
}
```

### 2. FEDERATION CONTEXT ISOLATION (CRITICAL)
**Issue**: Subscriptions don't verify subgraph ownership
**Current State**: No federation_id or service_name in subscription context
**Root Cause**: Subscriptions are not federation-aware
**Files to Modify**: `connection_manager.rs`, `executor.rs`, `protocol.rs`
**Implementation Steps**:
1. Pass federation context (subgraph_id) from Python layer
2. Store federation_id in ConnectionMetadata
3. Validate subscription belongs to current subgraph
4. Reject cross-subgraph subscription attempts

**Example**:
```rust
// Add to ConnectionMetadata
pub federation_id: Option<String>,  // Which subgraph owns this connection

// In executor
if let Some(sub_fed) = subscription.federation_id {
    if sub_fed != context.federation_id {
        reject_subscription("Cross-subgraph access denied")
    }
}
```

### 3. MULTI-TENANT ENFORCEMENT (CRITICAL)
**Issue**: tenant_id exists but isn't enforced on events
**Current State**: Events published without tenant filtering
**Root Cause**: No automatic tenant scoping in event bus
**Files to Modify**: `event_bus.rs`, Redis/PostgreSQL implementations, `executor.rs`
**Implementation Steps**:
1. Add tenant_id routing to event channels
2. Filter events by connection's tenant_id before delivery
3. Validate all queries include tenant_id parameter
4. Block cross-tenant event access

**Example**:
```rust
// Channel naming includes tenant context
let channel = format!("orders:{}:{}", federation_id, tenant_id);
bus.subscribe(channel)?;

// Event filtering by tenant
if event.tenant_id != context.tenant_id {
    skip_event  // Don't deliver
}
```

### 4. SUBSCRIPTION SCOPE VERIFICATION (MEDIUM)
**Issue**: No checks that subscription query parameters match user context
**Current State**: User can request data for any user_id
**Root Cause**: Variables not validated against authenticated context
**Files to Modify**: `executor.rs`, `protocol.rs`
**Implementation Steps**:
1. Extract subscription variables
2. Check user_id/tenant_id variables against context
3. Reject subscriptions with mismatched scope
4. Support wildcard subscriptions (all_my_orders) vs specific (order_by_id)

**Example**:
```rust
// User context
context.user_id = 123
context.tenant_id = "acme"

// Subscription request
subscription { my_orders(user_id: 456) }  // REJECT - mismatch!
subscription { my_orders(user_id: 123) }  // ACCEPT - matches
```

### 5. RBAC INTEGRATION WITH SUBSCRIPTIONS (MEDIUM)
**Issue**: PermissionResolver exists in Rust but not hooked to subscriptions
**Current State**: Field auth not checked during subscription
**Root Cause**: RBAC layer separate from subscription execution
**Files to Modify**: `executor.rs`, integrate with RBAC module
**Implementation Steps**:
1. Call PermissionResolver before subscription activation
2. Validate all requested fields have user permission
3. Cache permission checks (PermissionResolver already does this)
4. Log permission failures for audit

**Example**:
```rust
// Before yielding events
if !permission_resolver.can_access_field(
    context.user_id,
    "orders",
    "payment_method"
) {
    skip_field  // or reject entire subscription
}
```

---

## REVISED IMPLEMENTATION APPROACH

**Skip Phase 3.1: Auth Middleware** (Already done - can be enhanced later)
- JWT validation implemented ✅
- Can be integrated into WebSocket upgrade in Python layer
- Rust side is ready to receive auth context

### Phase 3.1: Row-Level Filtering (6 hours)
Add user_id + tenant_id context to subscriptions

**Tests to Add**:
- test_row_filter_user_id_isolation
- test_row_filter_tenant_id_isolation
- test_row_filter_combined_user_and_tenant
- test_row_filter_all_events_without_filtering
- test_row_filter_partial_result_filtering

### Phase 3.2: Federation Context Isolation (4 hours)
Add federation_id to connection context

**Tests to Add**:
- test_federation_context_isolation
- test_federation_cross_subgraph_rejected
- test_federation_context_stored_in_metadata
- test_federation_context_validation_on_subscription
- test_federation_multiple_subgraphs_isolated

### Phase 3.3: Multi-Tenant Enforcement (5 hours)
Implement tenant_id filtering in event channels

**Tests to Add**:
- test_multitenant_channel_routing
- test_multitenant_event_filtering
- test_multitenant_cross_tenant_rejection
- test_multitenant_wildcard_subscriptions
- test_multitenant_tenant_id_extraction

### Phase 3.4: Subscription Scope Verification (4 hours)
Validate query parameters match user context

**Tests to Add**:
- test_scope_user_id_mismatch_rejected
- test_scope_tenant_id_mismatch_rejected
- test_scope_variable_extraction
- test_scope_wildcard_allowed
- test_scope_explicit_scope_validated

### Phase 3.5: RBAC Integration with Subscriptions (4 hours)
Hook PermissionResolver into subscription execution

**Tests to Add**:
- test_rbac_field_permission_check
- test_rbac_missing_permission_rejected
- test_rbac_cache_performance
- test_rbac_audit_logging
- test_rbac_multiple_fields_partial_access

---

## ACCEPTANCE CRITERIA

### Row-Level Filtering ✅
- [ ] user_id + tenant_id added to subscription context
- [ ] Events filtered before yielding to client
- [ ] Multi-tenant events isolated correctly
- [ ] Tests pass: 5/5

### Federation Context Isolation ✅
- [ ] federation_id stored in ConnectionMetadata
- [ ] Cross-subgraph subscriptions rejected
- [ ] Federation context validated on each subscription
- [ ] Tests pass: 5/5

### Multi-Tenant Enforcement ✅
- [ ] Tenant_id routed in event channels
- [ ] Events filtered by tenant_id
- [ ] Cross-tenant access blocked
- [ ] Tests pass: 5/5

### Subscription Scope Verification ✅
- [ ] Query variables extracted and validated
- [ ] Mismatched user_id/tenant_id rejected
- [ ] Wildcard subscriptions supported correctly
- [ ] Tests pass: 5/5

### RBAC Integration ✅
- [ ] PermissionResolver called before subscription
- [ ] Missing field permissions rejected
- [ ] Permission cache working
- [ ] Audit logging for permission denials
- [ ] Tests pass: 5/5

---

## FILES TO CREATE/MODIFY

### New Files
- `fraiseql_rs/src/subscriptions/row_filter.rs` - Row-level filtering context
- `fraiseql_rs/src/subscriptions/federation_context.rs` - Federation identity tracking

### Modified Files
- `fraiseql_rs/src/subscriptions/mod.rs` - Add new modules
- `fraiseql_rs/src/subscriptions/connection_manager.rs` - Add federation_id to ConnectionMetadata
- `fraiseql_rs/src/subscriptions/executor.rs` - Add row filtering + scope validation + RBAC integration
- `fraiseql_rs/src/subscriptions/event_bus.rs` - Add tenant_id routing
- `fraiseql_rs/src/subscriptions/protocol.rs` - Add scope validation types
- `fraiseql_rs/src/subscriptions/integration_tests.rs` - Add 25 security tests

---

## COMMIT STRUCTURE

1. **Commit 1**: feat(subscriptions): add row-level filtering context [CRITICAL]
2. **Commit 2**: feat(subscriptions): add federation context isolation [CRITICAL]
3. **Commit 3**: feat(subscriptions): implement multi-tenant enforcement [CRITICAL]
4. **Commit 4**: feat(subscriptions): add subscription scope verification [MEDIUM]
5. **Commit 5**: feat(subscriptions): integrate RBAC with subscriptions [MEDIUM]
6. **Commit 6**: test(subscriptions): add Phase 3 security test suite [MAJOR]

---

## TIMELINE

| Phase | Duration | Tasks |
|-------|----------|-------|
| 3.1 | 6h | Row-level filtering (user_id + tenant_id) |
| 3.2 | 4h | Federation context isolation |
| 3.3 | 5h | Multi-tenant enforcement |
| 3.4 | 4h | Subscription scope verification |
| 3.5 | 4h | RBAC integration with subscriptions |
| **Total** | **23h** | **All 5 critical gaps addressed** |

---

## EXPECTED GRADE IMPROVEMENT

**Before Phase 3**: A (90%+)
**After Phase 3**: A+ (95%+)

**Security Gaps Fixed**:
- Row-level filtering: CRITICAL → ✅ FIXED
- Federation isolation: CRITICAL → ✅ FIXED
- Multi-tenant enforcement: CRITICAL → ✅ FIXED
- Scope validation: MEDIUM → ✅ FIXED
- RBAC integration: MEDIUM → ✅ FIXED

---

## DO NOT (Guardrails)

❌ Do NOT modify authentication logic without tests
❌ Do NOT remove existing rate limiting, only enhance it
❌ Do NOT change the GraphQL parser core
❌ Do NOT introduce breaking changes to public APIs
❌ Do NOT skip integration testing

---

**Status**: Ready for implementation
**Next Step**: Implement Phase 3.1 - Authentication Middleware
