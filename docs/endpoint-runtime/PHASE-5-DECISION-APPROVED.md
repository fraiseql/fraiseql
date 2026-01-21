# Phase 5 Authentication Design: Decision Approved

**Date**: January 21, 2026
**Status**: ✅ APPROVED
**Decision**: Implement V2 (Stable Foundation) - NO auth token caching

---

## Decision Statement

FraiseQL Phase 5 authentication will use the **V2 (Stable Foundation)** approach:

- ✅ Simple JWT validation (no token result caching)
- ✅ Trait-based SessionStore (developers choose backend)
- ✅ Generic OIDC provider (works with any OIDC service)
- ✅ Straightforward middleware integration
- ✅ Evidence-based optimization (add caching only when benchmarks show need)

**No auth token caching in Phase 5.1-5.4**. Can be added in Phase 5.7 if production metrics justify it.

---

## Rationale

### 1. FraiseQL v1 Precedent
FraiseQL v1 successfully implements query result caching but **does not cache auth tokens**. This proven architecture shows the project's maintainers concluded "cache queries (where savings are large), not auth (where savings are small)."

### 2. Industry Standard
- **Apollo GraphQL**: Fresh JWT validation every request
- **HotChocolate**: No JWT caching, uses ASP.NET Core
- **Hasura**: Only caches webhook responses (not JWT)
- **AWS AppSync**: Fresh validation per request

**Conclusion**: JWT caching is not industry standard practice.

### 3. Performance Impact
- Auth validation: 1-5ms (3% of request time)
- Query caching saves: 20-500ms (92% of request time)
- V2 performance loss: ~3-5ms per request (imperceptible to users)
- V2 total latency: Still <100ms (feels instant)

**Conclusion**: Auth validation is not the bottleneck.

### 4. Security Benefits
- Fresh validation: Immediate revocation, permission changes take effect instantly
- Cached validation: 5-60 minute delay for changes (security gap)
- Simplicity: No cache invalidation edge cases to manage

**Conclusion**: Fresh validation is simpler and safer.

### 5. Code Simplicity
- V1 (with caching): ~900 LOC
- V2 (simple): ~310 LOC
- Maintenance burden: 65% less code

**Conclusion**: Simpler code is easier to understand, audit, and maintain.

### 6. Optimization Path
If production metrics later show auth is a bottleneck:
- Phase 5.7: Add token caching
- Implementation: 1-2 weeks
- Risk: Zero (trait abstraction allows clean insertion)

**Conclusion**: Not committing to caching now doesn't close the door.

---

## What This Means

### ✅ What We Build in Phase 5.1-5.4

```
Auth System Components:
├─ JwtValidator
│  ├─ Simple, correct JWT validation
│  ├─ Signature verification
│  ├─ Expiry checking
│  └─ Claims extraction
├─ SessionStore trait
│  ├─ create_session(user_id, expires_at)
│  ├─ get_session(refresh_token_hash)
│  ├─ revoke_session(refresh_token_hash)
│  └─ revoke_all_sessions(user_id)
├─ OAuthProvider trait
│  ├─ authorization_url()
│  ├─ exchange_code()
│  ├─ user_info()
│  └─ refresh_token() [optional]
├─ OidcProvider (generic implementation)
│  └─ Works with any OIDC provider (Google, Keycloak, Auth0, etc.)
├─ Middleware
│  ├─ Extract token from Authorization header
│  ├─ Validate JWT
│  ├─ Attach claims to request
│  └─ Handle errors gracefully
└─ Configuration
   ├─ Simple TOML config
   ├─ Environment-based secrets
   └─ Per-provider settings
```

### ❌ What We Don't Build

- JWT signature result caching
- JWKS caching layer
- Token validation memoization
- Provider registry system
- Middleware hook system (can be added later)
- Multiple storage implementations (developers create their own)

### ⏳ What We Defer (Phase 5.7+)

If benchmarks show auth >50% of request time:
- Token result caching layer
- JWKS caching with TTL
- Cache invalidation strategy
- Performance optimizations

**Probability**: <5% (typical GraphQL APIs don't hit this threshold)

---

## Implementation Plan

### Phase 5.1: Core JWT Validation
**Deliverables**:
- JwtValidator struct
- Claims parsing and validation
- Error handling with clear messages
- Unit tests (100% coverage)

**Estimated**: 2-3 days

### Phase 5.2: Session Store Trait
**Deliverables**:
- SessionStore trait definition
- PostgreSQL reference implementation
- In-memory implementation (testing)
- Redis reference implementation (example only)
- Integration tests

**Estimated**: 3-4 days

### Phase 5.3: OIDC Provider
**Deliverables**:
- OAuthProvider trait
- Generic OidcProvider implementation
- OAuth flow (authorization → token exchange → user info)
- Error handling and retry logic
- Unit and integration tests

**Estimated**: 3-4 days

### Phase 5.4: Middleware & Endpoints
**Deliverables**:
- Authentication middleware
- POST /auth/start (initiate OAuth flow)
- GET /auth/callback (OAuth callback handler)
- POST /auth/refresh (refresh token)
- POST /auth/logout (revoke session)
- Integration tests

**Estimated**: 3-4 days

### Phase 5.5: Integration & Documentation
**Deliverables**:
- Query result cache integration
- Cache invalidation on token revocation
- Setup guides (Google, Keycloak, generic OIDC)
- Configuration reference
- API documentation
- Example projects

**Estimated**: 3-4 days

### Phase 5.6: Production Ready
**Deliverables**:
- Performance metrics (auth latency)
- Structured logging
- Error tracking integration
- Monitoring dashboard
- Production deployment guide

**Estimated**: 2-3 days

### Phase 5.7: Optional Optimization (IF NEEDED)
**Triggers**: Production benchmarks show auth >50% of request time
**Deliverables**:
- Token result caching layer
- JWKS caching with TTL
- Cache invalidation strategy
- Performance benchmarks

**Estimated**: 1-2 weeks (only if needed)

---

## Success Criteria

### Core Functionality
- [ ] JWT validation works (sign, verify, expiration)
- [ ] SessionStore trait implementable by developers
- [ ] PostgreSQL reference implementation production-ready
- [ ] Generic OIDC provider works with Google, Keycloak, Auth0
- [ ] All auth flows working (start, callback, refresh, logout)
- [ ] OAuth code exchange secure (PKCE support)
- [ ] Session revocation works (single and all-user)

### Code Quality
- [ ] 100% test coverage for core JWT/session logic
- [ ] All clippy warnings resolved
- [ ] No unsafe code
- [ ] Clear error messages with documentation links

### Documentation
- [ ] Setup guide for each major provider (Google, Keycloak, Auth0)
- [ ] How to implement custom SessionStore
- [ ] How to add custom OAuth provider
- [ ] Configuration reference
- [ ] Troubleshooting guide
- [ ] Performance tuning guide

### Performance
- [ ] JWT validation: <5ms (acceptable baseline)
- [ ] Session lookup: <10ms (single DB roundtrip)
- [ ] OAuth callback: <100ms (provider latency dominates)
- [ ] Zero memory leaks with concurrent sessions

### Security
- [ ] Tokens validated fresh each request
- [ ] Revocation takes effect immediately
- [ ] PKCE support for OAuth
- [ ] Token storage secure (hashed in DB)
- [ ] No sensitive data in logs

### Backward Compatibility
- [ ] Existing Phase 4 bearer token auth still works
- [ ] Phase 4 tests still pass
- [ ] Migration guide for bearer → OAuth optional but documented

---

## Decision Trade-offs

### What We Gain (V2 vs V1)
| Aspect | Gain |
|--------|------|
| **Code Simplicity** | 65% less code (~590 LOC saved) |
| **Maintainability** | Simpler architecture, fewer edge cases |
| **Flexibility** | Developers choose storage backend |
| **Security** | Fresh validation, no revocation delays |
| **Alignment** | Matches industry standard |
| **Time to Market** | Simpler to implement (~2 weeks) |

### What We Lose (V2 vs V1)
| Aspect | Loss |
|--------|------|
| **Auth Latency** | ~3-5ms per request (imperceptible) |
| **Built-in Optimizations** | Developers must add if needed |
| **Cache Hit Rate** | 0% initially (can be added later) |

**Trade-off Assessment**: Gains far outweigh losses. Performance loss is imperceptible, gains in simplicity and maintainability are substantial.

---

## Risk Assessment

### Risk: Performance Becomes Bottleneck
**Likelihood**: <5% (typical GraphQL APIs don't validate 1000+ JWT/sec)
**Impact**: High (users perceive slowness)
**Mitigation**: Add monitoring in Phase 5.6; optimize in Phase 5.7 if needed

### Risk: Security: Token Revocation Delayed
**Likelihood**: 0% (we validate fresh every request)
**Impact**: N/A
**Mitigation**: N/A (not a risk in V2)

### Risk: Developers Struggle with SessionStore
**Likelihood**: Low (trait is simple, examples provided)
**Impact**: Medium (support burden)
**Mitigation**: Clear documentation, reference implementations

### Risk: OIDC Provider Too Generic
**Likelihood**: Low (OIDC is standard)
**Impact**: Medium (need custom provider for edge cases)
**Mitigation**: Clear trait documentation, example custom implementations

---

## Related Documents

- **Design Documents**:
  - [V2 Design (Approved)](./05-PHASE-5-AUTH-DESIGN-ALT.md)
  - [V1 Design (Reference)](./05-PHASE-5-AUTH-DESIGN.md)

- **Analysis Documents**:
  - [Decision Summary](./PHASE-5-DECISION-SUMMARY.md)
  - [Performance Analysis](./PHASE-5-PERFORMANCE-ANALYSIS.md)
  - [Competitive Analysis](./PHASE-5-COMPETITIVE-ANALYSIS.md)
  - [Evaluation Guide](./PHASE-5-DESIGN-EVALUATION-GUIDE.md)

---

## Approval & Sign-off

**Decision Made**: January 21, 2026
**Status**: ✅ APPROVED
**Next Step**: Begin Phase 5.1 implementation

**Implementation Plan**: 2-3 weeks for Phase 5.1-5.4 (core framework)
**Timeline**: Phase 5.1-5.6 complete by [TBD]
**Optimization**: Phase 5.7 planned as optional, only if benchmarks justify

---

## Moving Forward

### Immediate Actions
1. ✅ Approve this decision
2. ⏳ Begin Phase 5.1 (Core JWT Validation)
3. ⏳ Setup GitHub tracking for Phase 5 tasks
4. ⏳ Schedule Phase 5 kickoff meeting

### Phase 5.1 Tasks
- [ ] Create crates/fraiseql-auth crate
- [ ] Implement JwtValidator
- [ ] Implement Claims parsing
- [ ] Write comprehensive tests
- [ ] Document JWT validation design

---

**Status**: APPROVED - Ready to implement Phase 5 with V2 design
