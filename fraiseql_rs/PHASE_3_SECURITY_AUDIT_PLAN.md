# Phase 3: Security Audit - Implementation Plan

**Status**: PLANNING
**Objective**: Fix 5 critical security gaps in the subscriptions module
**Target Production Grade**: A+ (95%+)

---

## CRITICAL SECURITY GAPS IDENTIFIED

### 1. MISSING AUTHENTICATION MIDDLEWARE (CRITICAL)
**Issue**: No JWT/token validation before WebSocket subscription
**Current State**: Clients can connect with arbitrary user_id
**Files to Modify**: `websocket.rs`, `protocol.rs`, `connection_manager.rs`
**Implementation Steps**:
1. Extract and validate Bearer token from WebSocket Upgrade headers
2. Decode JWT and extract user_id + tenant_id
3. Reject ConnectionInit if auth fails
4. Store validated user context in ConnectionMetadata

### 2. MISSING FIELD-LEVEL AUTHORIZATION (CRITICAL)
**Issue**: No permission checks on subscription fields
**Current State**: Users can subscribe to any field without validation
**Files to Modify**: `executor.rs`, integrate RBAC `field_auth.rs`
**Implementation Steps**:
1. Add FieldAuthChecker integration to SubscriptionExecutor
2. Validate subscription fields against user permissions
3. Reject subscriptions if user lacks field access
4. Log authorization violations for audit trail

### 3. MISSING SUBSCRIPTION SCOPE VALIDATION (CRITICAL)
**Issue**: No validation that query parameters match authenticated user
**Current State**: User A can subscribe to other users' data
**Files to Modify**: `executor.rs`, `protocol.rs`
**Implementation Steps**:
1. Extract variables from subscription request
2. Validate userId/tenantId parameters match authenticated context
3. Reject subscriptions with mismatched scope
4. Support scoped events (e.g., `userUpdated(id: $userId)`)

### 4. INCOMPLETE QUERY VALIDATION (MEDIUM)
**Issue**: Doesn't detect fragment cycles or excessive depth
**Current State**: Only field count checked, not depth or cycles
**Files to Modify**: `executor.rs`
**Implementation Steps**:
1. Implement query depth analysis (limit: 15 levels)
2. Detect and reject fragment cycles
3. Validate variable types against schema
4. Check payload sizes before deserialization

### 5. IN-MEMORY RATE LIMITING (MEDIUM)
**Issue**: Rate limits can be bypassed in distributed deployments
**Current State**: Token buckets only in-memory per instance
**Files to Modify**: `rate_limiter.rs`
**Implementation Steps**:
1. Implement Redis-backed distributed rate limiting
2. Add global server-side subscription creation limit
3. Implement event queue backpressure
4. Add connection recycling detection

---

## IMPLEMENTATION APPROACH

### Phase 3.1: Authentication (4 hours)
Implement WebSocket token validation

**Tests to Add**:
- test_auth_missing_token_rejected
- test_auth_invalid_token_rejected
- test_auth_expired_token_rejected
- test_auth_valid_token_accepted
- test_auth_user_id_extracted_correctly
- test_auth_tenant_id_extracted_correctly

### Phase 3.2: Field-Level Authorization (4 hours)
Integrate permission checking into executor

**Tests to Add**:
- test_authz_missing_field_permission_rejected
- test_authz_field_accessible_granted
- test_authz_multiple_fields_partial_denial
- test_authz_cross_tenant_denied
- test_authz_violations_logged

### Phase 3.3: Scope Validation (3 hours)
Validate subscription parameters match user context

**Tests to Add**:
- test_scope_mismatched_user_id_rejected
- test_scope_mismatched_tenant_id_rejected
- test_scope_matched_user_id_accepted
- test_scope_null_parameters_require_auth
- test_scope_variables_validated

### Phase 3.4: Query Validation Hardening (3 hours)
Detect cycles, depth, and variable type mismatches

**Tests to Add**:
- test_validation_fragment_cycle_rejected
- test_validation_excessive_depth_rejected
- test_validation_variable_type_mismatch_rejected
- test_validation_payload_size_check_early
- test_validation_depth_limit_15_levels

### Phase 3.5: Distributed Rate Limiting (4 hours)
Redis-backed rate limit enforcement

**Tests to Add**:
- test_ratelimit_distributed_redis_consistent
- test_ratelimit_global_server_limit_enforced
- test_ratelimit_backpressure_on_queue
- test_ratelimit_connection_recycling_detected
- test_ratelimit_per_instance_bypass_prevented

### Phase 3.6: Security Test Suite (2 hours)
End-to-end security validation

**Tests to Add**:
- test_security_e2e_authenticated_unauthorized_denied
- test_security_e2e_cross_user_data_access_denied
- test_security_e2e_cross_tenant_isolation_enforced
- test_security_e2e_distributed_dos_mitigated
- test_security_e2e_token_expiry_enforced

---

## ACCEPTANCE CRITERIA

### Authentication ✅
- [ ] JWT token required for WebSocket upgrade
- [ ] Invalid tokens rejected
- [ ] User context available to all handlers
- [ ] Tests pass: 6/6

### Field Authorization ✅
- [ ] FieldAuthChecker integrated into executor
- [ ] Unauthorized fields rejected
- [ ] Field access logged
- [ ] Tests pass: 5/5

### Scope Validation ✅
- [ ] User/tenant IDs validated against context
- [ ] Mismatched scopes rejected
- [ ] Variable parameters validated
- [ ] Tests pass: 5/5

### Query Validation ✅
- [ ] Fragment cycles detected and rejected
- [ ] Query depth limited to 15 levels
- [ ] Variable types validated
- [ ] Payload size checked early
- [ ] Tests pass: 5/5

### Distributed Rate Limiting ✅
- [ ] Redis-backed rate limiting working
- [ ] Global server limit enforced
- [ ] Backpressure mechanism active
- [ ] Connection recycling blocked
- [ ] Tests pass: 5/5

### Security Suite ✅
- [ ] End-to-end security tests passing
- [ ] No data access violations
- [ ] DoS attacks mitigated
- [ ] Audit trails present
- [ ] Tests pass: 5/5

---

## FILES TO CREATE/MODIFY

### New Files
- `fraiseql_rs/src/subscriptions/auth_middleware.rs` - JWT token validation
- `fraiseql_rs/src/subscriptions/query_validator.rs` - Enhanced validation (cycles, depth)
- `fraiseql_rs/src/subscriptions/distributed_rate_limiter.rs` - Redis-backed rate limiting

### Modified Files
- `fraiseql_rs/src/subscriptions/mod.rs` - Add new modules
- `fraiseql_rs/src/subscriptions/websocket.rs` - Add auth middleware to upgrade handler
- `fraiseql_rs/src/subscriptions/executor.rs` - Integrate FieldAuthChecker + scope validation
- `fraiseql_rs/src/subscriptions/protocol.rs` - Add validation error types
- `fraiseql_rs/src/subscriptions/integration_tests.rs` - Add 26 security tests

---

## COMMIT STRUCTURE

1. **Commit 1**: feat(subscriptions): add auth middleware with JWT validation
2. **Commit 2**: feat(subscriptions): integrate field-level authorization checks
3. **Commit 3**: feat(subscriptions): add subscription scope validation
4. **Commit 4**: feat(subscriptions): enhance query validation (cycles, depth, types)
5. **Commit 5**: feat(subscriptions): implement distributed rate limiting with Redis
6. **Commit 6**: test(subscriptions): add Phase 3 security test suite [MAJOR]

---

## TIMELINE

| Phase | Duration | Tasks |
|-------|----------|-------|
| 3.1 | 4h | Auth middleware + token validation |
| 3.2 | 4h | Field authorization integration |
| 3.3 | 3h | Scope validation |
| 3.4 | 3h | Query validation hardening |
| 3.5 | 4h | Distributed rate limiting |
| 3.6 | 2h | Security test suite |
| **Total** | **20h** | **All 5 critical issues fixed** |

---

## EXPECTED GRADE IMPROVEMENT

**Before Phase 3**: A (90%+)
**After Phase 3**: A+ (95%+)

**Risk Reduction**:
- Authorization: CRITICAL → ✅ FIXED
- Authentication: CRITICAL → ✅ FIXED
- Field-Level Auth: CRITICAL → ✅ FIXED
- Scope Validation: CRITICAL → ✅ FIXED
- Rate Limiting: MEDIUM → ✅ FIXED

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
