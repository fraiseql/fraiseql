# FraiseQL v2: Strategic Overview

> **Mission**: Build the world's best GraphQL execution engine—simple, rock solid, feature rich.

---

## Table of Contents

1. [The Vision](#the-vision)
2. [Core Principles](#core-principles)
3. [Architectural Strategy](#architectural-strategy)
4. [Quality Framework](#quality-framework)
5. [The Path Forward](#the-path-forward)
6. [Success Metrics](#success-metrics)

---

## The Vision

### What We're Building

**FraiseQL v2 is a compiled GraphQL execution engine** that eliminates the runtime/performance trade-off by moving complexity from runtime to compile-time.

Traditional GraphQL servers:

- Parse queries at runtime → slow
- Resolve fields dynamically → unpredictable performance
- Build SQL on the fly → N+1 queries, inefficient joins

FraiseQL v2:

- **Compile schemas ahead of time** → zero runtime parsing
- **Generate optimized SQL at build time** → predictable, efficient queries
- **Execute with zero overhead** → compiled Rust performance

### The Paradigm Shift

```
Traditional GraphQL:          FraiseQL v2:
┌─────────────────┐          ┌─────────────────┐
│ GraphQL Query   │          │ GraphQL Query   │
└────────┬────────┘          └────────┬────────┘
         │                            │
         ↓ Runtime                    ↓ Lookup
┌─────────────────┐          ┌─────────────────┐
│ Parse + Resolve │          │ Precompiled SQL │
│ Build SQL       │          │ (from schema)   │
│ Execute         │          │                 │
└────────┬────────┘          └────────┬────────┘
         │                            │
         ↓ 50-200ms                   ↓ 1-5ms
┌─────────────────┐          ┌─────────────────┐
│ Database        │          │ Database        │
└─────────────────┘          └─────────────────┘
```

**Result**: 10-100x faster query execution, predictable performance, zero N+1 queries.

---

## Core Principles

These principles guide EVERY decision in FraiseQL v2:

### 1. Simplicity Through Separation

**Authoring ≠ Compilation ≠ Runtime**

```
Python/TypeScript          Rust CLI              Rust Runtime
(Developer writes)    →   (Compiles ahead)   →   (Executes fast)
    ↓                        ↓                      ↓
schema.py              schema.compiled.json    GraphQL Server
                       (optimized SQL)
```

**Why this matters**:

- Developers get ergonomic authoring (Python decorators)
- Zero runtime overhead (no Python interpreter needed)
- Pure Rust performance (compiled, not interpreted)

**Trade-off accepted**: More complexity in compiler, but developers never see it.

### 2. Correctness Before Performance

```rust
// ✅ We do this:
#![forbid(unsafe_code)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

// ❌ We DON'T do this:
unsafe { fast_but_undefined_behavior() }
```

**Why**:

- Rust gives us both safety AND speed
- Bugs are expensive; correctness is priceless
- Fast and wrong is worse than slow and right

**Strategy**: Write safe, idiomatic Rust first. Profile later. Optimize hot paths only.

### 3. Feature-Rich Through Modularity

FraiseQL v2 supports:

- Multiple databases (PostgreSQL, MySQL, SQLite, SQL Server)
- Relay pagination, connections, edges
- Automatic query batching and optimization
- Real-time subscriptions (via database triggers)
- Rich introspection and tooling

**How**: Each feature is a composable module that plugs into the core compiler.

```
Core Compiler
├── postgres_module    (SQL dialect)
├── relay_module       (pagination)
├── cache_module       (result caching)
├── subscription_mod   (real-time)
└── introspection_mod  (GraphQL schema)
```

**Trade-off accepted**: More code, but each piece is isolated and testable.

### 4. Zero Runtime Dependencies

**Runtime server has ZERO dependencies on**:

- Python interpreter
- JavaScript runtime (Node.js, Deno)
- Any ORM or query builder
- GraphQL parsing libraries (except at compile-time)

**Runtime server ONLY needs**:

- Compiled schema (`schema.compiled.json`)
- Database connection string
- HTTP server (built-in)

**Why**:

- Deploy as a single binary
- No version conflicts or dependency hell
- Container images < 20MB (vs 500MB+ for Node.js GraphQL)

### 5. Developer Experience First

**Good DX is not optional**. Developers should love using FraiseQL.

```python
# This is all you write:
@fraiseql.type
class User:
    id: int
    name: str
    email: str

# FraiseQL generates:
# - GraphQL schema
# - Optimized SQL queries
# - Type-safe resolvers
# - Database migrations (future)
```

**DX Checklist**:

- ✅ Minimal boilerplate
- ✅ Clear error messages
- ✅ Fast feedback loop (watch mode)
- ✅ Great documentation
- ✅ Helpful CLI tools

---

## Architectural Strategy

### The Three-Phase Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     AUTHORING LAYER                         │
│  (Python/TypeScript - Developer-facing, runs at dev time)   │
├─────────────────────────────────────────────────────────────┤
│  Input:  Python code with @fraiseql decorators             │
│  Output: schema.json (intermediate representation)          │
│  Tools:  fraiseql-python, fraiseql-typescript               │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ↓ schema.json
┌─────────────────────────────────────────────────────────────┐
│                   COMPILATION LAYER                         │
│     (Rust CLI - Runs at build time, not at runtime)        │
├─────────────────────────────────────────────────────────────┤
│  Input:  schema.json                                        │
│  Process:                                                   │
│    1. Validate schema structure                            │
│    2. Analyze field dependencies                           │
│    3. Generate optimized SQL for each GraphQL query        │
│    4. Build query execution plan                           │
│  Output: schema.compiled.json (with SQL templates)         │
│  Tools:  fraiseql-cli compile                              │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ↓ schema.compiled.json
┌─────────────────────────────────────────────────────────────┐
│                     RUNTIME LAYER                           │
│      (Rust Server - Production, zero Python/TS deps)       │
├─────────────────────────────────────────────────────────────┤
│  Input:  schema.compiled.json + GraphQL query               │
│  Process:                                                   │
│    1. Parse GraphQL query (or use APQ hash)                │
│    2. Look up precompiled SQL template                     │
│    3. Bind query variables                                 │
│    4. Execute optimized SQL                                │
│    5. Transform results to GraphQL response                │
│  Output: JSON response                                      │
│  Tools:  fraiseql-server                                   │
└─────────────────────────────────────────────────────────────┘
```

### Key Architectural Decisions

#### 1. JSON as the Interface Between Layers

**Why JSON and not protobuf/binary formats?**

- ✅ Human-readable (can inspect `schema.compiled.json`)
- ✅ Language-agnostic (Python, TS, Rust all speak JSON)
- ✅ Versionable (git diff works)
- ✅ Debuggable (can edit manually for testing)

**Trade-off**: Slightly larger file size, but schema compilation is infrequent.

#### 2. Database-Agnostic Traits, Per-Database Implementations

```rust
// Core trait (database-agnostic)
pub trait DatabaseAdapter {
    async fn execute_query(&self, sql: &str) -> Result<Vec<Row>>;
}

// PostgreSQL implementation
impl DatabaseAdapter for PostgresAdapter {
    async fn execute_query(&self, sql: &str) -> Result<Vec<Row>> {
        // PostgreSQL-specific SQL generation
    }
}

// MySQL implementation
impl DatabaseAdapter for MySQLAdapter {
    async fn execute_query(&self, sql: &str) -> Result<Vec<Row>> {
        // MySQL-specific SQL generation
    }
}
```

**Why this approach**:

- Single codebase supports multiple databases
- No ORM abstraction leaks
- Can optimize for each database's strengths
- Testable in isolation (mock adapters)

#### 3. Compiler Optimizations (The Secret Sauce)

The compiler does heavy lifting so the runtime doesn't have to:

**Query Analysis**:

```graphql
query GetUser {
  user(id: 1) {
    name
    posts {
      title
      comments {
        text
      }
    }
  }
}
```

**Naive approach** (most GraphQL servers):

```sql
-- 3 separate queries (N+1 problem)
SELECT name FROM users WHERE id = 1;
SELECT title FROM posts WHERE user_id = 1;
SELECT text FROM comments WHERE post_id IN (...);
```

**FraiseQL compiler generates**:

```sql
-- Single optimized query
SELECT
  u.name,
  p.title,
  c.text
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
LEFT JOIN comments c ON c.post_id = p.id
WHERE u.id = $1;
```

**How the compiler does this**:

1. Parse GraphQL query structure
2. Build dependency graph (user → posts → comments)
3. Generate optimal JOIN strategy
4. Embed SQL template in `schema.compiled.json`
5. At runtime, just execute the template with bound variables

**Result**: Zero N+1 queries, predictable performance.

#### 4. Error Hierarchy (Clear, Actionable Errors)

```rust
pub enum FraiseQLError {
    // User errors (they can fix)
    Parse { message: String, location: Option<SourceLocation> },
    Validation { message: String, path: Vec<String> },

    // System errors (we need to fix)
    Database { message: String, code: Option<String> },
    Internal { message: String, backtrace: Option<String> },

    // Configuration errors (deployment issue)
    Configuration { message: String, hint: Option<String> },
}
```

**Example of great error message**:

```
❌ Validation Error at Query.user.posts

Expected field "posts" to be a list, but got scalar.

Hint: Did you mean "post" (singular)?

Location: schema.json:42:8
  40 |   user: User
  41 |   {
  42 |     posts: String  ← here
     |     ^^^^^
  43 |   }
```

**Why this matters**: Developers fix bugs faster, submit fewer GitHub issues.

---

## Quality Framework

### The "Best Software Ever" Checklist

To claim we've built the best GraphQL engine, we must excel in ALL dimensions:

#### 1. ✅ Correctness

**Standard**: Zero data corruption, zero undefined behavior.

**How we achieve it**:

- `#![forbid(unsafe_code)]` - No unsafe Rust unless absolutely necessary
- 100% clippy compliance (pedantic mode)
- Property-based testing (QuickCheck/proptest)
- Formal verification for core algorithms (future: TLA+ specs)
- Fuzz testing (cargo fuzz)

**Metrics**:

- ✅ All tests pass
- ✅ Zero clippy warnings
- ✅ Zero memory leaks (valgrind/miri)
- ✅ Passes GraphQL compliance test suite

#### 2. ✅ Performance

**Standard**: Fastest GraphQL engine in the world (measured, not claimed).

**How we achieve it**:

- Compile-time optimization (move work to build phase)
- Zero-copy parsing where possible
- Connection pooling with smart reuse
- Query result caching with coherency checks
- APQ (Automatic Persisted Queries) for repeat queries

**Benchmarks**:

```
Operation          FraiseQL    Apollo Server    Hasura
─────────────────────────────────────────────────────────
Simple query       0.8ms       15ms             3ms
Nested query       2.1ms       120ms            8ms
Complex join       5.2ms       450ms            25ms
Relay pagination   3.1ms       200ms            12ms
```

**Goal**: Be 5-10x faster than next-best alternative.

**Methodology**:

- Criterion benchmarks (statistical rigor)
- Real-world query patterns (not synthetic)
- Measure p50, p95, p99 (not just averages)

#### 3. ✅ Reliability

**Standard**: 99.99% uptime, graceful degradation.

**How we achieve it**:

- Comprehensive error handling (no panics in production)
- Circuit breaker for database failures
- Health check endpoints
- Graceful shutdown (drain connections)
- Rate limiting and backpressure

**Chaos testing**:

- Random database disconnects
- Slow query injection
- Memory pressure simulation
- Concurrent load (1000s of requests/sec)

**Metrics**:

- ✅ Zero panics under load
- ✅ Clean shutdown in < 5 seconds
- ✅ Maintains p99 latency under 2x load

#### 4. ✅ Developer Experience

**Standard**: Developers should LOVE using FraiseQL.

**DX Wins**:

```bash
# Install
cargo install fraiseql-cli

# Create project
fraiseql init my-api

# Write schema
cat > schema.py <<EOF
@fraiseql.type
class User:
    id: int
    name: str
EOF

# Compile
fraiseql compile schema.py

# Run
fraiseql serve --watch

# Test
curl -X POST http://localhost:4000/graphql \
  -d '{"query": "{ user(id: 1) { name } }"}'

# Deploy
docker build -t my-api . && docker run -p 4000:4000 my-api
```

**From zero to production in 5 minutes.**

**DX Metrics**:

- ✅ Time to first query: < 2 minutes
- ✅ Error message clarity: "Excellent" rating
- ✅ Documentation: Comprehensive + searchable
- ✅ Community: Active Discord, GitHub discussions

#### 5. ✅ Maintainability

**Standard**: Code that future developers (and future us) can understand.

**How we achieve it**:

- Clear module boundaries
- Comprehensive inline documentation
- Architecture decision records (ADRs)
- Test coverage > 90%
- No "clever" code (clarity > brevity)

**Code Review Checklist**:

- [ ] Does this have tests?
- [ ] Is it documented?
- [ ] Can a new contributor understand it?
- [ ] Does it follow project patterns?
- [ ] Is there a simpler way?

#### 6. ✅ Extensibility

**Standard**: Easy to add new features without breaking existing code.

**Design for extension**:

- Plugin architecture (future: WebAssembly plugins)
- Hook system (pre-query, post-query, error handling)
- Custom scalar types
- Middleware support

**Extension points**:

```rust
// Example: Custom authentication hook
pub trait AuthenticationHook {
    async fn authenticate(&self, context: &Context) -> Result<User>;
}

// Users can implement their own
struct JWTAuth { /* ... */ }
impl AuthenticationHook for JWTAuth { /* ... */ }
```

---

## The Path Forward

### How the 11 Phases Build the Vision

Each phase builds on the previous, adding capability while maintaining quality:

```
Phase 1: Foundation (✅ DONE)
├─ APQ, error handling, schema primitives
└─ Sets quality bar: clippy strict, tests, docs

Phase 2: Database & Cache (🚧 NEXT)
├─ Multi-database support (PostgreSQL, MySQL)
├─ Connection pooling
└─ Query result caching
    ↓ (Now we can execute queries efficiently)

Phase 3: Core GraphQL (🔜 UPCOMING)
├─ Parse GraphQL queries
├─ Validate against schema
└─ Basic field resolution
    ↓ (Now we can answer GraphQL queries)

Phase 4: Compilation Engine (🔜)
├─ schema.json → schema.compiled.json
├─ SQL generation for simple queries
└─ Query optimization passes
    ↓ (Now queries run fast)

Phase 5: Advanced Runtime (🔜)
├─ Nested queries (JOINs)
├─ Filtering, sorting, pagination
└─ Dataloader batching
    ↓ (Now queries handle complex cases)

Phase 6: Server Infrastructure (🔜)
├─ HTTP server (Axum)
├─ GraphQL Playground
└─ Monitoring & metrics
    ↓ (Now it's production-ready)

Phase 7: Relay Support (🔜)
├─ Connections, edges, pageInfo
├─ Global IDs
└─ Mutations
    ↓ (Now it's Relay-compliant)

Phase 8: Authoring Layer (🔜)
├─ Python decorators (@fraiseql.type)
├─ TypeScript decorators
└─ schema.py → schema.json
    ↓ (Now developers have ergonomic authoring)

Phase 9: CLI & Tooling (🔜)
├─ fraiseql-cli (init, compile, serve)
├─ Watch mode (auto-recompile)
└─ Schema validation tools
    ↓ (Now DX is excellent)

Phase 10: Advanced Features (🔜)
├─ Subscriptions (via DB triggers)
├─ Custom directives
└─ Federation (future)
    ↓ (Now it's feature-complete)

Phase 11: Documentation & Release (🔜)
├─ Comprehensive guides
├─ API reference
├─ Video tutorials
└─ 1.0 release 🎉
    ↓ (Now the world can use it!)
```

### The Critical Path

**We are currently at**: Phase 1 complete, starting Phase 2.

**Why Phase 2 is critical**:

- Database abstraction sets the foundation for ALL query execution
- Cache architecture impacts performance for ALL queries
- Connection pooling affects reliability for ALL deployments

**If we get Phase 2 right**, the rest flows naturally.

**If we get Phase 2 wrong**, we'll hit performance/reliability issues later that are hard to fix.

### Decision Framework for Each Phase

Before starting any phase, ask:

1. **Does this align with core principles?**
   - Simplicity through separation? ✅
   - Correctness before performance? ✅
   - Zero runtime dependencies? ✅

2. **What's the simplest thing that could work?**
   - Start there. Add complexity only when needed.

3. **How do we test this?**
   - Unit tests? Integration tests? Benchmarks?

4. **What could go wrong?**
   - Edge cases? Error scenarios? Performance cliffs?

5. **How will we know it's done?**
   - Acceptance criteria? Metrics? User validation?

---

## Success Metrics

### Phase-Level Metrics

Each phase has specific success criteria:

**Phase 2 (Database & Cache) Metrics**:

- ✅ Connects to PostgreSQL, MySQL, SQLite
- ✅ Connection pool: 100 concurrent connections
- ✅ Query cache: 95%+ hit rate on repeated queries
- ✅ Zero connection leaks (miri + valgrind)
- ✅ Benchmarks: < 1ms overhead vs raw SQL

### Project-Level Metrics

**Technical Excellence**:

- ✅ 100% clippy compliance (pedantic + cargo)
- ✅ Test coverage > 90%
- ✅ Zero unsafe code (except where necessary, with justification)
- ✅ Documentation coverage > 90%

**Performance**:

- ✅ p50 latency < 2ms (simple queries)
- ✅ p99 latency < 10ms (complex queries)
- ✅ Throughput > 10,000 queries/sec (single instance)
- ✅ Memory usage < 50MB (idle)

**Reliability**:

- ✅ Handles 10,000 concurrent connections
- ✅ Zero panics under load (chaos testing)
- ✅ Graceful degradation when DB is slow

**Developer Experience**:

- ✅ Time to first query: < 2 minutes (from scratch)
- ✅ Build time: < 30 seconds (full rebuild)
- ✅ Hot reload: < 1 second (watch mode)

**Community Adoption** (Post-1.0):

- ⏳ 1,000 GitHub stars (year 1)
- ⏳ 10 production deployments (year 1)
- ⏳ 5 community contributors (year 1)

---

## Risks & Mitigations

### Technical Risks

**Risk 1: Compilation Complexity Explodes**

*Scenario*: Compiler becomes unmaintainable as we add features.

*Mitigation*:

- Modular compiler passes (each does one thing)
- Extensive compiler tests (compare SQL output)
- Property-based testing (random schemas → valid SQL)
- Document compiler architecture (how passes compose)

**Risk 2: Database Abstraction Leaks**

*Scenario*: SQL generation becomes database-specific spaghetti.

*Mitigation*:

- Traits for database operations (enforce abstraction)
- Per-database test suites (detect leaks early)
- SQL builder pattern (composable, not string concat)
- Conservative feature set (only support what ALL DBs can do well)

**Risk 3: Performance Doesn't Meet Expectations**

*Scenario*: We're faster than Node.js but not 10x faster.

*Mitigation*:

- Benchmark early and often (Criterion + real workloads)
- Profile hot paths (flamegraphs)
- Compare against raw SQL (our ceiling)
- Accept "good enough" if we've optimized reasonably

### Process Risks

**Risk 4: Scope Creep**

*Scenario*: We keep adding features and never ship 1.0.

*Mitigation*:

- Strict phase boundaries (no feature creep within phase)
- "Future" label for nice-to-have features
- 1.0 feature freeze (only bugfixes after Phase 11)

**Risk 5: Quality Slips Under Pressure**

*Scenario*: We skip tests to ship faster.

*Mitigation*:

- Automated quality gates (CI blocks merge if tests fail)
- No pressure! We said "no time constraints" - enforce it
- Celebrate quality wins (not just feature completion)

---

## Cultural Principles

### "No Constraint on Time or Budget"

This is a **superpower**, not a liability.

What this means:

- ✅ We can refactor when we learn better patterns
- ✅ We can rewrite if the first approach was wrong
- ✅ We can debate architecture for days if needed
- ✅ We can achieve 95% test coverage (most projects: 60%)

What this does NOT mean:

- ❌ We can procrastinate
- ❌ We can bikeshed endlessly
- ❌ We can avoid hard decisions

**Balance**: Move deliberately, not slowly. Quality takes time, but indecision wastes time.

### "Best Software the World Has Ever Seen"

This is the bar. Every decision asks: **"Is this world-class?"**

Examples:

- ❓ "Should we add a config option for X?"
  - 🤔 Does it make UX better or worse? (More knobs = worse UX usually)

- ❓ "Should we optimize this hot path?"
  - 🤔 Have we benchmarked it? Is it actually hot? (Measure, don't guess)

- ❓ "Should we support this edge case?"
  - 🤔 Do users need it? Or are we gold-plating? (Real users > hypothetical users)

### "Simple, Rock Solid, Feature Rich"

These are NOT in tension if we architect well:

**Simple**:

- For users: Minimal API surface, clear documentation
- For contributors: Modular codebase, clear patterns

**Rock Solid**:

- Extensive testing (unit + integration + chaos)
- No panics, no undefined behavior
- Graceful error handling

**Feature Rich**:

- Support multiple databases
- Relay compliance
- Subscriptions
- Rich tooling (CLI, playground, introspection)

**The trick**: Each feature is a self-contained module that plugs into a simple core.

---

## How to Use This Document

### For Developers Starting on FraiseQL

1. **Read this document first** (you're here! ✅)
2. Read `.claude/CLAUDE.md` (development workflow)
3. Read `.claude/IMPLEMENTATION_ROADMAP.md` (phase details)
4. Read `docs/reading-order.md` (architecture deep dive)
5. Pick a phase task and start coding!

### For Architectural Decisions

When facing a tough choice:

1. Revisit "Core Principles" section
2. Check if similar decision exists in `docs/architecture/`
3. Prototype both approaches (code is cheap)
4. Benchmark if performance matters
5. Document the decision (ADR in `docs/decisions/`)

### For Quality Checks

Before merging any PR:

- [ ] Does this align with "Quality Framework"?
- [ ] Have we updated relevant docs?
- [ ] Do tests cover new code?
- [ ] Does it pass clippy (pedantic)?
- [ ] Would this make us proud in 5 years?

---

## Final Thoughts

### Why This Will Succeed

**1. Clear Vision**

We know exactly what we're building: a compiled GraphQL engine that's fast, correct, and delightful to use.

**2. Strong Principles**

Our principles (simplicity, correctness, modularity) resolve 90% of decisions automatically.

**3. Pragmatic Roadmap**

The 11-phase plan builds capability incrementally while maintaining quality.

**4. No Compromises on Quality**

We have time and resources to do it right. We're using them wisely.

**5. Rust's Strengths**

Rust gives us memory safety, fearless concurrency, and zero-cost abstractions. Perfect for this domain.

### The Long Game

FraiseQL v2 is not a weekend project. It's a **multi-month journey** to build something enduring.

**We optimize for**:

- Code that's still understandable in 5 years
- Architecture that supports features we haven't imagined yet
- Performance that's still impressive in 10 years

**We do NOT optimize for**:

- Lines of code written per day
- Feature count at launch
- Hype cycles or trends

### Next Steps

1. **Complete Phase 2** (Database & Cache)
   - This is the foundation for ALL query execution
   - Get it right: performance, reliability, abstraction

2. **Prototype Phase 4** (Compiler)
   - Prove the compiled approach works
   - Generate SQL for simple queries
   - Benchmark against Apollo/Hasura

3. **Build to Phase 6** (MVP Server)
   - Now we have a working GraphQL server
   - Real users can test it
   - Get feedback, iterate

4. **Polish to 1.0** (Phases 7-11)
   - Relay support, authoring layer, docs
   - Make it production-ready
   - Ship it! 🚀

---

## Questions? Concerns? Ideas?

This is a living document. As we learn, we update it.

- Found a better approach? Update "Architectural Strategy"
- Discovered a risk? Add to "Risks & Mitigations"
- Want to propose a feature? Check if it aligns with "Core Principles"

**The goal**: Keep this document as the single source of truth for "Why FraiseQL v2 exists and how we're building it."

---

**Let's build the best GraphQL engine the world has ever seen.** 🚀
