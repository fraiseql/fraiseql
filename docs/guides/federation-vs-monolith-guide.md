# Federation vs Monolithic Database: Decision Guide

**Version:** 1.0
**Status:** Complete
**Last Updated:** February 5, 2026

## Quick Answer

```
Single database server sufficient?
├─ YES → Stick with single database
│        (simpler until you outgrow it)
│
├─ NO: Why not?
│  ├─ Multiple teams/services → Federation
│  ├─ Geographic distribution → Federation
│  ├─ Compliance (data residency) → Federation
│  ├─ Scale (>100K qps) → Federation
│  ├─ Technology choice (multi-DB) → Federation
│  └─ Database conflicts → Federation
│
└─ Still deciding? Use this guide
```

---

## Comparison Matrix

### Operational Complexity

| Aspect | Monolith | Federation |
|--------|----------|-----------|
| **Deployment** | Single instance | 2+ instances + gateway |
| **Maintenance** | Single DB ops | Multi-DB ops + coordination |
| **Failover** | Automatic (RDS) | Manual per subgraph |
| **Scaling** | Vertical (upgrade hardware) | Horizontal (add subgraphs) |
| **Monitoring** | Single dashboard | Multiple dashboards + correlation |
| **Backup/Recovery** | Single strategy | Per-subgraph strategy |
| **Debugging** | Direct database access | Trace across services |

### Performance Characteristics

| Metric | Monolith | Federation |
|--------|----------|-----------|
| **Latency (single query)** | 10-50ms | 50-200ms |
| **Throughput (QPS)** | 1,000-10,000 | 100,000+ |
| **Concurrent users** | 1,000-10,000 | 100,000+ |
| **Join query speed** | <100ms | 200-500ms (cross-DB joins) |
| **Setup time** | 1 hour | 4-8 hours |

### Data & Scaling

| Aspect | Monolith | Federation |
|--------|----------|-----------|
| **Dataset size** | 100GB-2TB | Multi-PB (each subgraph separate) |
| **Query complexity** | Highly complex OK | Simpler per subgraph |
| **Transaction scope** | Single database | Limited (SAGA pattern) |
| **Data locality** | Single region | Multi-region possible |
| **Consistency model** | ACID | CP (Consistency + Partition Tolerance) |

---

## Decision Flowchart

### Phase 1: Current State

```
Single database working fine?
├─ YES → Keep monolith
│        Continue until pain point appears
│
└─ NO: What's the problem?
   ├─ Too slow → Consider federation
   ├─ Too expensive → Consider federation
   ├─ Multiple teams → Federation
   ├─ Compliance → Federation
   └─ Multiple tech → Federation
```

### Phase 2: Scale Analysis

```
How much data?
├─ <100GB → Monolith fine
├─ 100GB-1TB → Still monolith (with work)
└─ >1TB → Consider federation

How many queries per second?
├─ <1,000 QPS → Monolith fine
├─ 1,000-10,000 QPS → Monolith with optimization
└─ >10,000 QPS → Federation
```

### Phase 3: Operational Needs

```
Multiple independent teams?
├─ YES → Federation (each team owns subgraph)
├─ NO: Need data residency (GDPR/HIPAA)?
│  ├─ YES → Federation (data stays in-country)
│  └─ NO: Different databases needed?
│     ├─ YES → Federation
│     └─ NO: Performance not acceptable?
│        ├─ YES → Federation
│        └─ NO → Stay with monolith
```

---

## Detailed Comparison

### Monolithic Database

**Best for:**
- Startup phase
- Single team
- Related data (tightly coupled)
- <10 million records
- <1,000 queries/second
- Transactions across all data

**Operational Model:**
```
┌─ FraiseQL Application ─┐
│                         │
└─ PostgreSQL (single)   ─┘

Single node, simpler operations, full transaction support
```

**Scaling strategy:**
1. Vertical: Add CPU/memory (works to ~50K QPS)
2. Replicas: Read replicas (helps with reads)
3. Caching: Redis/Memcached layer
4. Optimization: Indexes, query optimization
5. Only then: Consider federation

**Time to implementation:** 1 hour setup
**Operational burden:** Low
**Failure modes:** Single point of failure (handle with RDS multi-AZ)

**Example Architecture:**
```yaml
fraiseql-server:
  database: postgresql://prod-db.internal:5432/fraiseql
  replicas:
    - postgresql://replica1.internal:5432/fraiseql
    - postgresql://replica2.internal:5432/fraiseql
  caching:
    redis: localhost:6379
```

---

### Federated Database

**Best for:**
- Multiple independent services
- >10 million records
- >1,000 queries/second
- Geographically distributed teams
- Data residency requirements
- Different database needs per service

**Operational Model:**
```
┌──────────────────────────────┐
│   Apollo Router (Gateway)     │
├──────────────────────────────┤
│  Subgraph 1    Subgraph 2     │ Subgraph 3
│  (Users)       (Orders)       │ (Analytics)
│  ↓             ↓              │ ↓
│  PostgreSQL    MySQL          │ ClickHouse
```

Multiple services, owned independently, coordinated by gateway

**Scaling strategy:**
1. Add new subgraph for new service
2. Each subgraph scales independently
3. Combine via federation gateway
4. Can span regions/continents

**Time to implementation:** 4-8 hours (includes testing)
**Operational burden:** Higher (multi-system monitoring)
**Benefits:** Independent scaling, team autonomy, data residency

**Example Architecture:**
```yaml
apollo-gateway:
  port: 4000

subgraphs:
  users:
    url: http://users-service:8000/graphql
    database: postgresql://users-db:5432/users

  orders:
    url: http://orders-service:8000/graphql
    database: mysql://orders-db:3306/orders

  products:
    url: http://products-service:8000/graphql
    database: sqlite:///products.db
```

---

## When to Federate

### Clear Signal #1: Multiple Teams

**Symptom:** Teams waiting on each other for database changes

**Why Federation Helps:**
- Each team owns subgraph
- Independent deployments
- No blocking on shared database

**Impact:**
- Deployment speed: 10x faster
- Team autonomy: 100%
- Coordination overhead: Medium

### Clear Signal #2: Scale Exhausted Single Database

**Symptom:** Queries slow, database CPU at 100%, can't optimize further

**Before federating:**
```bash
# 1. Run EXPLAIN ANALYZE on slow query
# 2. Add indexes on missing columns
# 3. Partition large tables
# 4. Archive old data
# 5. Scale database vertically (bigger server)

# Only THEN consider federation
```

**Federation helps if:**
- Individual queries fast (sub-100ms)
- But total throughput limited
- Need distributed computation

### Clear Signal #3: Data Residency Requirement

**Symptom:** GDPR/HIPAA requires data stay in specific region

**Why Federation Helps:**
- EU customer data → PostgreSQL in EU
- US customer data → PostgreSQL in US
- Single query spans both (via federation)

**Impact:**
- Compliance: ✅ Met
- Performance: ⚠️ Slower (cross-region)
- Cost: Higher (multiple databases + gateway)

### Clear Signal #4: Different Database Technology Needed

**Symptom:** Needs PostgreSQL features for users, ClickHouse for analytics

**Why Federation Helps:**
- Use right tool for job
- Analytics via ClickHouse (1000x faster)
- Users via PostgreSQL (ACID transactions)
- Single GraphQL interface

**Impact:**
- Flexibility: ✅ Multiple tech stacks
- Complexity: 🔴 Higher ops burden
- Performance: ⚠️ Optimization required

---

## Migration Path

### Stage 1: Monolith (Year 1)

```
Single PostgreSQL
↓
All services query same database
↓
Fast development, simple operations
```

**Red flags appearing:**
- Database response time degrading
- Teams conflicting on schema
- Data residency questions
- Different data access patterns

### Stage 2: Federation Planning (Quarter before pain)

```
Monolith → Federated
    ↓
1. Identify service boundaries
2. Plan subgraph split
3. Test federation locally
4. Prepare dual-write strategy
```

### Stage 3: Migration (2-4 weeks)

```
Week 1: Set up federation infrastructure
    ├─ Deploy Apollo Router
    ├─ Set up subgraph services
    └─ Test integration

Week 2: Dual-write (both monolith + federation)
    ├─ Service writes to both
    ├─ Validate federation working
    └─ Monitor for inconsistencies

Week 3: Cutover
    ├─ Route reads to federation
    ├─ Keep monolith as backup
    └─ Monitor closely

Week 4: Cleanup
    ├─ Decommission monolith
    ├─ Final testing
    └─ Return to normal operations
```

**Downtime:** 0-5 minutes (if well-planned)
**Risk:** Low (can rollback to monolith)

---

## Decision Table

| Situation | Recommendation | Reasoning |
|-----------|---|---|
| Startup, <1 million users | Monolith | Too early for federation complexity |
| 1-10 million users, 1 database | Monolith | Still works, continue optimization |
| 10+ million users | Evaluate | May need federation soon |
| Multiple independent teams | Federation | Team autonomy outweighs complexity |
| Data residency requirement | Federation | No choice, required for compliance |
| >50,000 QPS | Federation | Performance demands it |
| Simple application | Monolith | Keep simple until forced |
| Complex data model | Monolith | Simpler transactions |

---

## Hybrid Approach (Both)

**Pattern:** Start monolith, add federation selectively

```
Monolith (Users, Core)
    ↓
    ├─ Analytics → Federation (add ClickHouse)
    ├─ Reporting → Federation (add read-only DB)
    └─ Audit Logs → Federation (add separate DB)

Result: 80% on monolith, 20% on federation
        (lower complexity than full federation)
```

**Benefits:**
- Gradual complexity increase
- Can isolate problematic workloads
- Easy to pilot federation
- Low risk

---

## Troubleshooting Migration Decision

### "Our monolith is fine but team wants federation"

**Red flags:**
- Federation without pain point is premature
- Adds complexity unnecessarily
- May slow down development

**Recommendation:**
- Document current performance
- Set pain threshold
- Migrate when threshold crossed
- Not speculative

### "We started federation too early"

**Symptoms:**
- Coordination overhead high
- Development slower (not faster)
- Operations too complex

**Options:**
1. Consolidate back to monolith (if <2 years)
2. Simplify federation (fewer subgraphs)
3. Use managed service (reduce ops burden)

### "We need federation but can't operate it"

**Solutions:**
1. Use managed federation service
2. Hire DevOps/SRE team
3. Simplify architecture
4. Start with single federation (one subgraph split)

### "Performance still bad after federation"

**Likely causes:**
- Queries still complex
- Wrong data in wrong database
- Network latency between subgraphs
- SAGA coordination overhead

**Solutions:**
- Re-architect queries (simpler per-subgraph)
- Move data to optimize locality
- Use direct DB federation (not HTTP)
- Profile cross-service calls

---

## See Also

- **[Consistency Model](./consistency-model.md)** - FraiseQL federation consistency
- **[Federation Guide](../integrations/federation/guide.md)** - Complete federation tutorial
- **[Production Deployment](./production-deployment.md)** - Operating monolith or federation
- **[Performance Optimization](../architecture/performance/advanced-optimization.md)** - Scaling before federation
- **[SAGA Pattern](../integrations/federation/sagas.md)** - Federation transactions

---

**Remember:** The right choice for now might not be the right choice later. Plan for evolution, but don't over-engineer early.
