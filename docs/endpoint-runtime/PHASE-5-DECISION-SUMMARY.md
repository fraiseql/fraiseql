# Phase 5: FraiseQL Authentication Design - Decision Summary

## The Question
Should Phase 5 include JWT/auth token caching like V1 design, or skip it like V2 design?

## The Answer
**Skip auth token caching. Choose V2.** Here's why:

---

## Evidence

### 1. FraiseQL v1 Already Chose This

FraiseQL v1 has sophisticated query result caching but **zero auth token caching**:

```rust
// FraiseQL v1: Query result cache (✅ implemented)
pub struct QueryResultCache {
    cache: Arc<Mutex<LruCache<String, CachedResult>>>,
    dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

// FraiseQL v1: Auth middleware (❌ no caching)
pub fn validate_request(&self, req: &AuthRequest) -> Result<AuthenticatedUser> {
    let token = self.extract_token(req)?;     // No cache
    self.validate_token_structure(&token)?;   // No cache
    let claims = self.parse_claims(&token)?;  // No cache
    Ok(AuthenticatedUser { ... })
}
```

**Implication**: v1 maintainers decided "cache queries, not auth" was the right call.

### 2. Industry Standard: Nobody Caches JWT

| Framework | Auth Token Caching | Query Caching | Notes |
|-----------|------------------|---------------|-------|
| **Apollo GraphQL** | ❌ No | ✅ Yes (client) | Validates fresh |
| **HotChocolate** | ❌ No | ✅ Yes (DataLoader) | Uses ASP.NET Core |
| **Hasura** | ⚠️ Webhook only | ✅ Yes | Exception: webhook=50-200ms |
| **Dgraph** | ❌ No | ✅ Yes | Fresh validation |
| **AWS AppSync** | ❌ No | ✅ Yes | Per-request |
| **FraiseQL v1** | ❌ No | ✅ Yes | Same strategy |

**Conclusion**: JWT caching is not industry practice. Only webhook-based systems (Hasura) cache auth, because webhook calls are slow (50-200ms).

### 3. Performance Math Shows Auth Isn't the Bottleneck

**Where time goes in typical GraphQL request**:

```
Auth validation:       1-5ms    (3%)
Query planning:        1-10ms   (5%)
Database queries:      20-500ms (92%)
Result formatting:     1-10ms   (0%)
───────────────────────────────────
Total:                 23-525ms

Caching query results saves: 20-500ms (92% of time)
Caching auth saves:          1-5ms (3% of time)
```

**Real-world example**:
- Add query caching: 30ms → 10ms (3x faster, life-changing)
- Add auth caching on top: 10ms → 9.9ms (imperceptible)

### 4. FraiseQL Inherits v1's Query Caching

Phase 5 will inherit v1's sophisticated query result caching:

```
FraiseQL v2 = Query Result Cache (inherited) + Simple Auth (new)

Query cache handles:
✅ 92% of performance optimization opportunity
✅ 20-500ms savings per request
✅ Already battle-tested in v1
✅ Coherency validation prevents stale results

Auth validation:
✅ 1-5ms (acceptable overhead)
✅ Simple, correct, no cache coherency issues
✅ Matches industry standard
✅ Can add caching later if benchmarks prove needed
```

### 5. Security Trade-off: Fresh Validation > Cached Validation

**Token revocation scenario**:

```
Cached auth (risky):
├─ Token is revoked at auth server
├─ But locally cached as valid
├─ User keeps access until cache expires (5-60 minutes!)
└─ Security violation

Fresh validation (safe):
├─ Token is revoked at auth server
├─ Next request validates fresh
├─ Revocation is effective immediately
└─ Security maintained
```

**Why this matters**:
- Permission changes (user promoted/demoted)
- Account suspension
- Logout from other devices
- Scope/role revocation

Fresh validation is worth 3-5ms overhead for security.

---

## What We Learned

### Performance Reality

| Operation | Latency | Impact |
|-----------|---------|--------|
| JWT signature validation | 1-5ms | Acceptable |
| Query result cache hit | 0ms | Essential |
| Query cache miss → DB | 20-500ms | Dominant factor |

**Optimization order**:
1. ✅ Query caching (inherited from v1) → 10-30x improvement
2. ✅ Database optimization (better indexes) → 2-5x improvement
3. ⏳ Auth caching (Phase 5.7, only if benchmarks show need) → 10-50x on auth only

### Security Reality

Fresh validation is safer AND we still cache the thing that matters (query results). No contradiction.

### Developer Reality

Forcing a caching system (V1) is harder than letting developers choose (V2):
```
V1: "You must use our caching system"
└─ Works for some, wrong choice for others (Redis vs Postgres)

V2: "Validate auth simply, cache what you need in your SessionStore"
└─ Works for everyone, flexible, straightforward
```

---

## Recommendation: V2 (Stable Foundation)

### Phase 5 Design Should Be:

```
Phase 5.1-5.4: Core Framework
├─ Simple JWT validation (1-5ms per request)
├─ Trait-based SessionStore (developers choose backend)
├─ Generic OIDC provider (works with any OIDC provider)
└─ Middleware integration

Phase 5.5: Query result caching integration
├─ Inherit query cache from v1
├─ Document cache invalidation patterns
└─ Benchmarks show real performance gains

Phase 5.6-5.7: Optimization (IF NEEDED)
├─ Monitor JWT validation in production
├─ If >50% of request time in auth: Add token caching
├─ Simple wrapper around JwtValidator
└─ Takes 1-2 weeks to implement
```

### Why V2 Wins

| Dimension | V1 (Caching First) | V2 (Simple First) |
|-----------|------------------|-----------------|
| **Code Complexity** | 900 LOC | 310 LOC |
| **Security** | Cache invalidation edge cases | Fresh validation |
| **Flexibility** | Prescribed solutions | Trait-based choice |
| **Industry Alignment** | Not standard | Matches Apollo, HotChocolate |
| **FraiseQL v1 Alignment** | Contradicts v1 (v1 doesn't cache auth) | Aligns with v1 |
| **Optimization Path** | Complex to extend | Simple to add caching |
| **Real Performance Gain** | ~3-5ms if cached | 20-500ms from query cache |
| **Risk** | Cache bugs, revocation issues | Zero (fresh validation) |

---

## What To Do Now

### 1. Confirm V2 Design ✅
- V2 is simpler, safer, matches industry
- Inherited query caching from v1 handles 92% of optimization
- Auth validation overhead (1-5ms) is acceptable

### 2. Update Documentation
- Phase 5 uses V2 approach
- Explain why auth caching is deferred
- Point to v1's query caching for performance gains

### 3. Plan Query Cache Integration
- Phase 5.5: How does new auth system integrate with query cache?
- Cache invalidation on token revocation?
- Session-aware cache keys?

### 4. Monitoring Strategy
- Measure JWT validation latency in production
- If >50% of request time in auth: Plan Phase 5.7 optimization
- Otherwise: Prove caching unnecessary

### 5. Future Path: Token Caching (Phase 5.7+)

If benchmarks show auth is bottleneck (unlikely):

```rust
// Add this later if needed - no impact on current design
pub struct CachedJwtValidator {
    inner: JwtValidator,
    cache: Arc<DashMap<String, CachedClaims>>,
}

impl CachedJwtValidator {
    pub async fn validate(&self, token: &str) -> Result<Claims> {
        if let Some(claims) = self.cache.get(token) {
            return Ok(claims);
        }

        let claims = self.inner.validate(token)?;
        self.cache.insert(token, claims.clone());
        Ok(claims)
    }
}
```

Takes 1 week to add, zero risk to v2's design.

---

## Final Answer

| Question | Answer | Reasoning |
|----------|--------|-----------|
| Does FraiseQL v1 cache auth? | No | v1 caches queries instead |
| Do competitors cache JWT? | No (Hasura exception) | Only webhook auth, not JWT |
| Is 1-5ms JWT latency a problem? | No | Query cache saves 20-500ms |
| Can we add caching later? | Yes | 1-2 weeks with trait abstraction |
| What should Phase 5 do? | Choose V2 | Simple, safe, industry-standard |
| What about performance? | Query cache ≫ auth cache | 92% of gains from query cache |
| Any risk to V2? | No | Fresh validation is safer |
| Will users need auth caching? | Unlikely | Unless >1000 JWT req/sec |

---

## Committing to V2

**Decision**: Phase 5 authentication design uses V2 (Stable Foundation)

**Rationale**:
1. ✅ Aligns with FraiseQL v1's proven caching strategy (queries, not auth)
2. ✅ Matches industry standard (Apollo, HotChocolate, Dgraph)
3. ✅ Simpler code (310 LOC vs 900 LOC)
4. ✅ Safer (fresh validation, no revocation edge cases)
5. ✅ Flexible (trait-based SessionStore)
6. ✅ Evidence-based (add caching only when benchmarks show need)
7. ✅ Query caching (inherited from v1) provides 92% of performance gains
8. ✅ Can optimize auth in 1-2 weeks if/when needed

**Implementation**: Start Phase 5.1 with V2 design.

---

## References

- [Competitive Analysis](./PHASE-5-COMPETITIVE-ANALYSIS.md)
- [Performance Analysis](./PHASE-5-PERFORMANCE-ANALYSIS.md)
- [Evaluation Guide](./PHASE-5-DESIGN-EVALUATION-GUIDE.md)
- [V1 Design (Caching First)](./05-PHASE-5-AUTH-DESIGN.md)
- [V2 Design (Stable First)](./05-PHASE-5-AUTH-DESIGN-ALT.md)

