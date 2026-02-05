# Choosing FraiseQL: Is It Right for Your Project?

**Status:** ✅ Production Ready
**Audience:** Architects, Technical Leads
**Reading Time:** 10-15 minutes
**Last Updated:** 2026-02-05

FraiseQL is **not a general-purpose GraphQL engine**. It's optimized for a specific set of problems. This guide helps you decide if it's a good fit.

---

## Prerequisites

### Required Knowledge:

- GraphQL concepts and use cases
- Database architecture and query patterns
- ACID vs eventual consistency trade-offs
- API design and performance requirements
- Your project's data model and access patterns
- Alternative GraphQL engines and ORMs
- Latency and throughput requirements
- Data consistency requirements

### Required Software:

- None (this is a decision-making guide, not hands-on implementation)
- Optional: Documentation from your existing system/architecture

### Required Infrastructure:

- None (decision guide only)

#### Optional but Recommended:

- Performance baseline data from current system (if migrating)
- Team technical expertise assessment
- Architecture documentation

**Time Estimate:** 15-30 minutes for initial evaluation, 1-2 hours for comprehensive comparison with alternatives

## Quick Checklist

Answer these questions honestly:

- [ ] Do you need **guaranteed consistency** (no stale data)?
- [ ] Can mutations wait **100-500ms** to complete?
- [ ] Is your database the **source of truth** (not external APIs)?
- [ ] Do you have **relational data** (not primarily document-oriented)?
- [ ] Do you need **ACID compliance** or regulated industry support?

### 4+ YES → FraiseQL is likely a good fit

### 2-3 YES → Evaluate carefully

### 0-1 YES → Probably choose something else

---

## Feature Comparison Matrix

### Consistency & Reliability

| Requirement | FraiseQL | DynamoDB | Cassandra | Firebase | GraphQL-core |
|---|---|---|---|---|---|
| Strong consistency | ✅ | ⚠️ eventual | ⚠️ eventual | ⚠️ eventual | ✅ |
| ACID transactions | ✅ | ⚠️ limited | ❌ | ❌ | ✅ |
| Distributed transactions | ✅ (SAGA) | ❌ | ❌ | ❌ | ❌ |
| Multi-tenant isolation | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| 100% audit trail | ✅ | ⚠️ | ⚠️ | ✅ | ❌ |

### Performance

| Requirement | FraiseQL | DynamoDB | Cassandra | Firebase | GraphQL-core |
|---|---|---|---|---|---|
| Mutation latency | 100-500ms | <10ms | <10ms | <100ms | 50-200ms |
| Query throughput | High | Very high | Very high | Medium | Medium |
| N+1 prevention | ✅ | ✅ | ✅ | ✅ | ❌ |
| Automatic caching | ✅ | ❌ | ❌ | ✅ | ⚠️ |

### Operational

| Requirement | FraiseQL | DynamoDB | Cassandra | Firebase | GraphQL-core |
|---|---|---|---|---|---|
| Managed service | ❌ | ✅ | ⚠️ | ✅ | ❌ |
| Infrastructure needed | PostgreSQL+ | AWS | Cassandra | Google Cloud | Any DB |
| Scaling complexity | Low | Automatic | Medium-High | Automatic | High |
| Cost | Database-dependent | Per request | Self-hosted | Per request | Self-hosted |

### Developer Experience

| Requirement | FraiseQL | DynamoDB | Cassandra | Firebase | GraphQL-core |
|---|---|---|---|---|---|
| Language support | 16 languages | AWS SDKs | CQL | Firebase SDKs | Any |
| Schema validation | ✅ Compile-time | ⚠️ Runtime | ⚠️ Runtime | ⚠️ Runtime | ⚠️ Runtime |
| Authorization rules | ✅ Compiled | ⚠️ Custom | ⚠️ Custom | ⚠️ Custom | ⚠️ Custom |
| API generation | ✅ Automatic | ⚠️ Manual | ❌ | ⚠️ Manual | ⚠️ Manual |
| Query optimization | ✅ Compile-time | ⚠️ At query | ⚠️ At query | ⚠️ At query | ❌ |

---

## Use Case Analysis

### ✅ Excellent Fit

#### 1. Financial Services & Banking

**Why FraiseQL**:

- Requires absolute consistency (no double-charging)
- Needs audit trail (regulatory compliance)
- Mutations are infrequent, must be correct
- Multi-step transactions are common

**Example**: "Transfer $1000 from account A to account B across services"

```graphql
mutation Transfer($fromId: ID!, $toId: ID!, $amount: Money!) {
  transferMoney(fromId: $fromId, toId: $toId, amount: $amount) {
    fromBalance
    toBalance
    transactionId
  }
}
```text

FraiseQL guarantees: Either both accounts updated, or neither. No partial transfers.

---

#### 2. Healthcare & Medical Records

**Why FraiseQL**:

- Patient safety depends on data accuracy
- Regulatory compliance (HIPAA, etc.)
- Audit trail required
- Data corruption is unacceptable

**Example**: "Update patient medication with cross-service lab result verification"

```graphql
mutation PrescribeMedication($patientId: ID!, $medication: String!) {
  prescribeMedication(patientId: $patientId, medication: $medication) {
    patient { id, allergies }
    prescription { id, medication }
  }
}
```text

FraiseQL guarantees: Prescription never issued if allergy check fails.

---

#### 3. Inventory Management

**Why FraiseQL**:

- Overselling causes financial loss
- Multiple warehouses need coordination
- Order processing is transactional
- Consistency prevents double-booking

**Example**: "Move inventory between warehouses"

```graphql
mutation MoveInventory(
  $sku: String!
  $from: ID!
  $to: ID!
  $quantity: Int!
) {
  moveInventory(sku: $sku, from: $from, to: $to, quantity: $quantity) {
    fromWarehouse { available }
    toWarehouse { available }
  }
}
```text

FraiseQL guarantees: Inventory either moves completely or not at all.

---

#### 4. Enterprise SaaS (Multi-tenant)

**Why FraiseQL**:

- Data isolation is critical
- Customers expect consistency
- ACID compliance expected
- Audit logging required

**Example**: "Multi-tenant user management with role hierarchy"

```graphql
query GetTenantUsers($tenantId: ID!) {
  users(tenantId: $tenantId) {
    id, email, role
  }
}

mutation AddUser($tenantId: ID!, $email: String!, $role: String!) {
  addUserToTenant(tenantId: $tenantId, email: $email, role: $role) {
    id, email, role
  }
}
```text

FraiseQL guarantees: No cross-tenant data leaks, mutations atomic per tenant.

---

### ⚠️ Possible Fit (With Caveats)

#### 1. E-commerce (Without Real-time Features)

**Pros**:

- Order processing needs consistency
- Inventory accuracy critical
- Payment processing needs ACID

**Cons**:

- Users expect <100ms response times (FraiseQL does 100-500ms)
- Real-time stock updates nice-to-have (not required)
- Shopping cart updates don't need strict consistency

**Verdict**: Use FraiseQL for:

- ✅ Order checkout & payment
- ✅ Inventory management
- ❌ Real-time cart updates (use cache)
- ❌ Live stock counts (use Redis)

---

#### 2. CMS & Content Management

**Pros**:

- Data consistency important
- Publishing workflows benefit from SAGA
- Audit trail required

**Cons**:

- Read-heavy (FraiseQL doesn't optimize for this)
- Mutation latency acceptable
- Caching is effective

**Verdict**: FraiseQL works but might be overkill.

- Better choice: WordPress, Strapi, or simpler CMS

---

### ❌ Poor Fit

#### 1. Real-time Analytics

**Why NOT FraiseQL**:

- Needs high throughput (500k+ rows/sec)
- Eventual consistency is fine
- Mutations rare, queries frequent
- Stale data acceptable

**Better choice**: DynamoDB, Cassandra, ClickHouse

**Example anti-pattern**:

```graphql
query RealTimeMetrics {
  metrics(last: 10000) {
    timestamp, value
  }
}
```text

FraiseQL would be slow. Use Cassandra instead.

---

#### 2. Social Media

**Why NOT FraiseQL**:

- Availability > Consistency (AP, not CP)
- Like counts can be approximated
- Comment ordering eventual ok
- High throughput required (1000+ req/sec per user)

**Better choice**: DynamoDB, Cassandra, Firebase

**Example anti-pattern**:

```graphql
mutation LikePost($postId: ID!) {
  likePost(postId: $postId) {
    likes  # Doesn't need exact count
  }
}
```text

DynamoDB's eventual consistency is perfect here.

---

#### 3. IoT & Time Series

**Why NOT FraiseQL**:

- Millions of writes/sec
- Some data loss acceptable
- Queries are time-range based
- Relational structure minimal

**Better choice**: InfluxDB, TimescaleDB, Prometheus

**Example anti-pattern**:

```graphql
mutation LogSensorReading($sensorId: ID!, $value: Float!) {
  logReading(sensorId: $sensorId, value: $value) {
    sensorId, value, timestamp
  }
}
```text

Use time-series DB directly.

---

#### 4. Real-time Chat / Presence

**Why NOT FraiseQL**:

- Needs low latency (<50ms ideal)
- Eventually consistent is fine
- Message ordering eventual ok
- High concurrent connections

**Better choice**: Firebase, Socket.io + Redis, Websockets

**Example anti-pattern**:

```graphql
mutation SendMessage($chatId: ID!, $text: String!) {
  sendMessage(chatId: $chatId, text: $text) {
    id, text, createdAt
  }
}
```text

Use message broker + cache instead.

---

## Decision Flowchart

```text
START
  │
  ├─ Do you need STRONG CONSISTENCY?
  │  ├─ NO → Don't use FraiseQL, use DynamoDB/Cassandra
  │  └─ YES
  │     │
  │     ├─ Can mutations wait 100-500ms?
  │     │  ├─ NO → Don't use FraiseQL, use eventual consistency system
  │     │  └─ YES
  │     │     │
  │     │     ├─ Is your data RELATIONAL (tables, joins)?
  │     │     │  ├─ NO → Don't use FraiseQL, use document DB
  │     │     │  └─ YES
  │     │     │     │
  │     │     │     ├─ Do you need distributed transactions?
  │     │     │     │  ├─ YES → FraiseQL SAGA is perfect
  │     │     │     │  └─ NO
  │     │     │     │     │
  │     │     │     │     ├─ Do you need enterprise features?
  │     │     │     │     │  ├─ YES (audit, RBAC, multi-tenant)
  │     │     │     │     │  │  └─ FraiseQL is ideal
  │     │     │     │     │  └─ NO
  │     │     │     │     │     └─ FraiseQL works, but simpler systems might too
  │
  └─ END
```text

---

## Migration Paths

### From Other GraphQL Engines

**From Apollo Server**:

- Apollo is interpretation-based, FraiseQL is compiled
- No direct migration, but patterns similar
- FraiseQL eliminates resolvers entirely
- Time: 2-4 weeks for small API

**From Hasura**:

- Hasura auto-generates API from schema, FraiseQL compiles schema
- Hasura supports more databases (Oracle, etc.)
- FraiseQL has better transaction support
- Time: 2-3 weeks for migration

**From Prisma**:

- Prisma is ORM-based, FraiseQL is query-generation-based
- Both eliminate N+1 problems
- FraiseQL has federation support, Prisma doesn't
- Time: 1-2 weeks (small API)

### To Other Systems

**If you choose wrong and need to migrate OUT**:

**FraiseQL → DynamoDB**:

- Time: 3-4 weeks
- Loss: Strong consistency guarantees
- Gain: Higher throughput, better availability

**FraiseQL → Firebase**:

- Time: 2-3 weeks
- Loss: Transaction support, schema flexibility
- Gain: Managed service, less ops work

**FraiseQL → Cassandra**:

- Time: 4-6 weeks
- Loss: Transaction support, schema validation
- Gain: Extreme scale, availability

---

## Red Flags: Don't Use FraiseQL If

🚫 **You need mutation latency < 50ms**

- FraiseQL's synchronous SAGA adds 100-500ms overhead

🚫 **You need Availability in distributed scenarios**

- FraiseQL chooses Consistency, refuses AP

🚫 **Your data is primarily document-based**

- FraiseQL assumes relational schema

🚫 **You need infinite scaling without cost increase**

- FraiseQL's cost scales with database performance

🚫 **You want a managed service (hands-off)**

- FraiseQL requires managing PostgreSQL/MySQL

🚫 **You're building real-time analytics**

- Use ClickHouse, InfluxDB, or similar

🚫 **You want "eventual consistency" design**

- FraiseQL refuses this philosophy

---

## Green Flags: Do Use FraiseQL If

✅ **You need guaranteed consistency**

- FraiseQL makes it a first-class guarantee

✅ **You have complex multi-service transactions**

- SAGA pattern with automatic compensation

✅ **You're in regulated industry** (finance, healthcare)

- Audit logging and compliance built-in

✅ **You need multi-tenant data isolation**

- Field-level RBAC compiled into schema

✅ **You want compile-time schema validation**

- Errors caught at build time, never runtime

✅ **You're tired of N+1 query problems**

- Joins determined at compile time

✅ **You want schema as code** (not API comments)

- 16 languages supported for schema authoring

---

## Recommendation: Talk to the Team

Before choosing FraiseQL, answer these questions:

1. **Consistency**: Is "guaranteed consistency" worth 100-500ms latency?
2. **Availability**: Can your system tolerate failures instead of approximate responses?
3. **Scope**: Do you have relational data and multi-service coordination?
4. **Compliance**: Do you need regulated industry features (audit, RBAC)?
5. **Scale**: Does your database scale to your throughput needs?

If the answers are yes, FraiseQL is the right choice.

If the answers are mixed, discuss trade-offs with your team. Every architecture choice involves trade-offs.

**There is no universally "best" system.** Only the right choice for your specific problem.

---

## Troubleshooting Decision Process

### "I'm unsure if FraiseQL is right for us"

#### Decision Framework:

1. **What's your primary concern?**
   - Data consistency → FraiseQL ✅
   - High availability → Other options ❌
   - Real-time performance (<50ms) → Other options ❌
   - Schema safety → FraiseQL ✅

2. **What's your data model?**
   - Highly relational (10+ tables, joins) → FraiseQL ✅
   - Mostly document-oriented (JSON data) → Firebase/Datastore ✅
   - Time-series focused → ClickHouse/Prometheus ✅
   - Mixed relational + documents → FraiseQL can handle ✅

3. **Do you have this requirement?**
   - Field-level RBAC enforcement → FraiseQL ✅
   - Audit logging compliance → FraiseQL ✅
   - Multi-tenant isolation → FraiseQL ✅
   - Low-latency real-time (<10ms p95) → Other options

4. **What's your team's expertise?**
   - GraphQL comfortable → FraiseQL ✅
   - SQL comfortable → FraiseQL ✅
   - REST API comfortable → No GraphQL learning ❌
   - Needs ORM (no schema code) → Other options ✅

### If 3+ checks passed: Strong FraiseQL fit
### If 1-2 checks passed: Evaluate carefully
### If 0 checks passed: Probably wrong tool

### "Our team is skeptical about consistency trade-offs"

#### Address Concerns:

| Concern | Counter-Point | Evidence |
|---------|---------------|----------|
| "100-500ms latency is too slow" | Most business logic already has this latency | Compare: API Gateway (20ms) + DB (50ms) + Network (30ms) = 100ms baseline |
| "We need real-time updates" | FraiseQL supports WebSocket subscriptions | See [Real-time subscriptions](../architecture/realtime/subscriptions.md) |
| "We'll need eventual consistency anyway" | Implement at application layer if truly needed | See [Federation Guide](../integrations/federation/guide.md) for patterns |
| "Consistency not important for us" | Then FraiseQL isn't the right choice | Consider alternatives |

### "We're between FraiseQL and [Alternative]"

#### Quick Comparison:

| Need | FraiseQL | Firebase | DynamoDB | GraphQL-Core |
|------|----------|----------|----------|--------------|
| Strong consistency | ✅ | ❌ | ❌ | ✅ |
| Multi-database | ✅ | ❌ | ❌ | ✅ |
| Schema as code | ✅ | ❌ | ❌ | ❌ |
| Built-in RBAC | ✅ | ❌ | ❌ | ❌ |
| Low-latency real-time | ❌ | ✅ | ✅ | ❌ |
| Serverless | ❌ | ✅ | ✅ | ❌ |
| Learning curve | Medium | Low | Low | High |

### Recommendation:

- If you need consistency + schema safety → FraiseQL
- If you need serverless + real-time → Firebase/DynamoDB
- If you need maximum flexibility → GraphQL-core

### "How do we pilot FraiseQL to prove it works?"

#### Phased Approach:

#### Phase 1 (Week 1): POC on single feature

- Pick one GraphQL query with 2-3 tables
- Define schema in Python/TypeScript
- Compile and run local test
- Time: 2-4 hours
- Success metric: Query executes and returns data

### Phase 2 (Week 2): Expand to one service

- Migrate one real service to FraiseQL
- Run side-by-side with existing API for comparison
- Load test: Compare performance profiles
- Time: 2-3 days
- Success metric: FraiseQL performance acceptable

### Phase 3 (Weeks 3-4): Production trial

- Deploy to staging
- Shadow traffic (duplicate requests to both)
- Monitor error rates, latency, consistency
- Time: 1-2 weeks
- Success metric: All metrics within acceptable range

### Phase 4 (Week 5+): Full migration

- Gradual cutover: 10% → 25% → 50% → 100%
- Rollback plan ready
- Time: 2-4 weeks depending on traffic
- Success metric: Running in production with no issues

---

## See Also

- [Consistency Model Deep Dive](./consistency-model.md)
- [Production Deployment](./production-deployment.md)
- [Foundation Concepts](../foundation/01-what-is-fraiseql.md)
- [Core Architecture](../foundation/02-core-concepts.md)
