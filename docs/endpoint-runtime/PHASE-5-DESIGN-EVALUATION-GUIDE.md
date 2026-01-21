# Phase 5 Authentication Design: Evaluation Framework

This guide helps evaluate the two authentication design approaches against FraiseQL's core principles and organizational priorities.

## Quick Reference: Two Design Philosophies

| Aspect | V1: Performance-First | V2: Stable Foundation |
|--------|---------------------|----------------------|
| **Core Philosophy** | Optimize from day one | Get correctness first |
| **Caching** | Built-in (JWKS, tokens) | Developer adds when needed |
| **Storage** | Postgres, Redis, In-Memory provided | Trait-based, developers choose |
| **Complexity** | ~1000 LOC (complex registry) | ~500 LOC (minimal trait) |
| **Initial Target** | <100µs token validation | Correct validation first |
| **Provider System** | Dynamic registry with hooks | Generic OIDC + custom trait |
| **Developer Flexibility** | Must use provided solutions | Can use any storage system |

---

## Evaluation Criteria Framework

### 1. Architectural Alignment (Highest Priority)

**FraiseQL Core Principle**: *"Compile-time optimization, zero-runtime overhead"*

#### Questions to Evaluate:

- **Does it align with FraiseQL's philosophy?**
  - V1: "Optimize everything from day one" → matches some aspects
  - V2: "Simple foundation, optimize when benchmarks show it's needed" → matches philosophy more closely

- **Which approach prevents technical debt?**
  - V1: Complex caching system that needs maintenance long-term
  - V2: Minimal core that grows only with evidence-based optimization

- **Which allows "zero-cost abstraction" mindset?**
  - V1: Multiple abstraction layers (caching, registry, middleware hooks)
  - V2: Single abstraction (SessionStore trait) that developers control

### 2. Long-Term Maintainability

#### Code Complexity Comparison:

**V1 Performance-First**:
```
- JwksCache (40 lines)
- TokenCache (25 lines)
- FastJwtValidator (35 lines)
- ProviderRegistry (30 lines)
- AuthMiddlewareHook trait (25 lines)
- SessionStore implementations (3x 50 lines = 150 lines)
- Middleware system (40 lines)
Total: ~350-400 lines of framework code
+ 3-5 built-in provider implementations (~500 lines)
= ~900 LOC minimum
```

**V2 Stable Foundation**:
```
- JwtValidator (35 lines)
- SessionStore trait (25 lines)
- OAuthProvider trait (25 lines)
- OidcProvider implementation (80 lines)
- Middleware (25 lines)
Total: ~190 lines of framework code
+ PostgreSQL reference example (70 lines)
+ Redis reference example (50 lines)
= ~310 LOC total
```

#### Maintenance Burden:

- **V1**: Cache invalidation (always hard), registry synchronization, multiple provider implementations, middleware hook ordering
- **V2**: Single trait to maintain, developers own optimization decisions

### 3. Developer Experience (DX)

#### Setting Up Auth:

**V1**:
```toml
# Must configure multiple backends
[auth]
provider = "google"
session_store = "postgres"  # or redis, or memory
cache_ttl = "3600s"

[auth.google]
client_id_env = "GOOGLE_CLIENT_ID"
```

**V2**:
```toml
# Minimal, delegate storage choice
[auth]
provider = "oidc"

[auth.oidc]
issuer = "https://accounts.google.com"
client_id_env = "OIDC_CLIENT_ID"
```

#### Implementing Custom SessionStore:

**V1**: Must understand registry, caching layers, eviction policies
```rust
// Complex trait with many considerations
impl SessionStore for MyStore {
    // Must handle cache invalidation
    // Must integrate with JwksCache
    // Must respect TTL policies
}
```

**V2**: Simple trait, clear responsibility
```rust
// Simple 4-method trait
impl SessionStore for MyStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair>
    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData>
    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()>
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>
}
```

### 4. Flexibility & Extensibility

#### Real-World Scenarios:

**Scenario A: Team uses DynamoDB for sessions**
- **V1**: Must implement custom SessionStore, but also understand caching system - High friction
- **V2**: Implement SessionStore trait, done - Natural fit

**Scenario B: Team wants in-memory caching**
- **V1**: Already has it, but forced to use system
- **V2**: Can add caching wrapper around SessionStore at their discretion

**Scenario C: Team uses Keycloak for OIDC**
- **V1**: Must register as custom provider, handle registry
- **V2**: Use OidcProvider with Keycloak URL directly

**Scenario D: Team wants to avoid Redis entirely**
- **V1**: Still includes Redis implementation they don't need
- **V2**: Use PostgreSQL, no Redis dependency

### 5. Performance Implications

#### Real-World Performance:

**V1 Targets**:
- JWT validation: <100µs (cached, 95%+ cache hit rate)
- Session lookup: <1ms (cached, 90%+ cache hit rate)
- Memory: ~50MB for 10K cached sessions

**V2 Starting Point**:
- JWT validation: ~1-5ms (no cache initially)
- Session lookup: ~5-10ms (database roundtrip)
- When to optimize: Benchmarks show bottleneck

#### Performance Reality:

- **V1**: Optimizations are useful only if:
  - Token validation happens >100 times/second
  - Session lookups happen >100 times/second
  - Memory is constrained (~50MB for 10K sessions is reasonable)

- **V2**: Can add same optimizations later if needed:
  - Add JwtCache wrapper without changing SessionStore
  - Add session result caching layer
  - Better informed because real data drives decisions

### 6. Security Considerations

#### Token Validation:

- **V1**: Cached token validation = must handle token revocation edge cases
  - What if token is revoked before cache expires?
  - Need separate revocation list checking

- **V2**: Fresh validation each time = straightforward, always correct
  - Token revocation is immediate
  - JWKS updates are immediate
  - Simpler threat model

#### Cache Poisoning:

- **V1**: Risks in caching JWKS/tokens:
  - What if JWKS endpoint is compromised?
  - Cached data could be stale
  - Need cache invalidation strategy

- **V2**: Minimal caching by default:
  - Validate fresh from source each time
  - Cache added deliberately where justified

### 7. When Each Approach Wins

#### V1 (Performance-First) is Better When:

- [ ] High-throughput requirement (>1000 req/sec with JWT validation)
- [ ] Latency budget is tight (<10ms total request time)
- [ ] You have infrastructure to manage Redis/caching
- [ ] Team is experienced with distributed caching
- [ ] Cache invalidation patterns are well-understood

#### V2 (Stable Foundation) is Better When:

- [ ] Long-term maintainability matters more than micro-optimizations
- [ ] Team values simplicity and auditability
- [ ] You want flexibility in storage backend choice
- [ ] You prefer evidence-based optimization (measure first)
- [ ] You use diverse infrastructure (not all Postgres/Redis)
- [ ] You want to avoid premature complexity

---

## Evaluation Questions for Your Team

**These questions should guide the decision:**

### Question 1: What is FraiseQL's primary goal?
- A) Be the fastest GraphQL engine → V1 (Performance-First)
- B) Be the most reliable, maintainable compiled GraphQL engine → V2 (Stable Foundation)

### Question 2: What does "zero-cost abstraction" mean in your context?
- A) Fast at runtime, even if complex at compile time → V1
- B) Simple, transparent, pay only for what you use → V2

### Question 3: Who maintains this code?
- A) Dedicated team with strong ops/caching expertise → V1
- B) Distributed team, prefer understandable code → V2

### Question 4: How much customization do end users need?
- A) Follow our recommendations, use provided storage backends → V1
- B) Use whatever storage you already have → V2

### Question 5: What happens if optimization targets change?
- A) Hard to remove complex caching system → V1 risk
- B) Easy to add optimizations based on real data → V2 advantage

---

## Decision Matrix

| Criteria | Weight | V1 Score | V2 Score |
|----------|--------|----------|----------|
| Aligns with FraiseQL philosophy | 25% | 70% | 90% |
| Long-term maintainability | 20% | 60% | 90% |
| Developer flexibility | 20% | 65% | 95% |
| Initial complexity | 15% | 40% | 95% |
| Security model clarity | 10% | 70% | 90% |
| **Weighted Total** | **100%** | **65%** | **91%** |

*Note: These scores are illustrative. Your team should weight criteria based on your priorities.*

---

## Recommended Evaluation Process

### Step 1: Alignment Check (15 minutes)
- Read FraiseQL's core principles again
- Does "compile-time optimization, zero-runtime overhead" mean:
  - Optimize everything from day one? → leans V1
  - Simple, transparent code that grows based on evidence? → leans V2

### Step 2: Use Case Review (20 minutes)
- Think about actual FraiseQL users:
  - Are they performance-sensitive? High-traffic GraphQL APIs?
  - Or do they value simplicity and customization?
  - What storage systems do they use?

### Step 3: Long-Term Vision (15 minutes)
- Where is FraiseQL in 2-3 years?
- What will the team's priorities be?
- Which design is easier to evolve toward?

### Step 4: Risk Assessment (15 minutes)
- What could go wrong with V1?
  - Cache invalidation bugs
  - Provider registry complexity
  - Forced use of specific storage backends
- What could go wrong with V2?
  - Users won't optimize when needed
  - Performance might not be competitive
  - Generic OIDC might be limiting

### Step 5: Hybrid Option (Optional)
- Could you take the best of both?
- Start with V2 (simple, correct core)
- Add V1-style caching as optional layer when benchmarks demand it
- Keep middleware hooks for extensibility

---

## Key Trade-offs Summary

### V1: Pay now, optimize always
```
✅ Fast from day one
✅ Optimized for high throughput
✅ Multiple storage backends
❌ Complex caching logic
❌ Cache invalidation edge cases
❌ More code to maintain
❌ Forces specific architecture choices
```

### V2: Get it right first, optimize when needed
```
✅ Simple, understandable code
✅ Developers choose their storage
✅ Security is straightforward
✅ Easy to add caching later
✅ Aligns with "compile-time optimization" philosophy
❌ Higher latency without caching
❌ Requires adding optimization if performance needed
❌ Generic OIDC might not cover all cases
```

---

## Final Recommendation Framework

**Choose V1 if**:
- FraiseQL positions as "fastest compiled GraphQL engine"
- Most users will be high-traffic services
- You want caching built-in from day one
- Team has strong distributed systems experience

**Choose V2 if**:
- FraiseQL positions as "reliable, maintainable compiled GraphQL engine"
- You want flexibility and simplicity
- Evidence-based optimization fits your values
- You want to prevent technical debt from premature optimization

**Choose Hybrid if**:
- Start with V2 (stable foundation)
- Track performance metrics
- Add V1-style caching only when benchmarks show it's needed
- Best of both worlds, but requires discipline

---

## References

- **V1 (Performance-First)**: `05-PHASE-5-AUTH-DESIGN.md`
- **V2 (Stable Foundation)**: `05-PHASE-5-AUTH-DESIGN-ALT.md`
- **FraiseQL Core Philosophy**: `.claude/CLAUDE.md` - "FraiseQL v2 Development Guide"

---

## Questions for the Evaluation Agent

When reviewing these designs, consider:

1. **Philosophical Fit**: Which design better embodies FraiseQL's stated goals?
2. **Maintenance Reality**: Which would be easier to maintain over time?
3. **User Flexibility**: Which gives FraiseQL users more options?
4. **Technical Debt**: Which one creates less future debt?
5. **Evidence**: Which approach follows "measure before optimizing"?

Your evaluation should provide clear reasoning for which approach better serves FraiseQL's long-term vision.
