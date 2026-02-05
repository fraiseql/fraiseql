# Consistency Model: CAP Theorem in FraiseQL

## Prerequisites

**Required Knowledge:**
- CAP theorem fundamentals (Consistency, Availability, Partition Tolerance)
- Distributed systems concepts
- ACID properties and transactions
- Eventual vs strong consistency
- Network partition failure modes
- Database replication and synchronization
- Multi-region deployment patterns
- FraiseQL federation architecture

**Required Software:**
- FraiseQL v2.0.0-alpha.1 or later (for federation scenarios)
- Your chosen SDK language
- PostgreSQL, MySQL, SQLite, or SQL Server (with appropriate replication tools)
- Monitoring tools to detect network partitions
- Logging infrastructure for debugging consistency issues

**Required Infrastructure:**
- Multiple FraiseQL instances (for federation scenario discussion)
- Primary database + replica/standby setup
- Network monitoring tools
- Load balancer or DNS for failover
- Optional: multi-region deployment infrastructure

**Optional but Recommended:**
- Database replication tools (Postgres replication, MySQL binlog)
- Network failure simulation tools (chaos engineering)
- Distributed transaction coordinator (if needed)
- Consistency verification tools

**Time Estimate:** 30-45 minutes to understand model, 1-2 hours for production implementation planning

## The Choice: CP (Consistency + Partition Tolerance)

FraiseQL makes a deliberate architectural choice based on the CAP theorem:

| Guarantee | Provided? | How |
|-----------|-----------|-----|
| **Strong Consistency** | ✅ Yes | ACID within database, causal across federation |
| **Partition Tolerance** | ✅ Yes | Handles network splits between subgraphs |
| **High Availability** | ❌ No | Fails gracefully instead of serving stale data |

**You can't have all three.** FraiseQL chooses Consistency and Partition Tolerance, sacrificing Availability.

---

## Why This Choice?

### The CAP Theorem Reality

When a network partition occurs between services:

```
┌─────────────────┐
│   Service A     │
│  (DB primary)   │
└────────┬────────┘
         │
    🔴 NETWORK DOWN 🔴
         │
┌────────▼────────┐
│   Service B     │
│  (DB replica)   │
└─────────────────┘
```

You must choose between:

1. **CP Mode**: Refuse to serve Service B until network recovers → Consistency, no lies
2. **AP Mode**: Service B serves best-guess data → Available, but possibly wrong

### FraiseQL's Answer: CP

**Refuse to serve wrong data.**

If Service B's database can't confirm consistency with Service A, FraiseQL returns an error instead of a guess. This costs **availability** but guarantees **correctness**.

**Why?** Because the cost of wrong data is catastrophic in enterprise systems:

- Banking: Double-charging or money loss
- Healthcare: Incorrect medication dosing
- Inventory: Overselling products you don't have
- Financial reporting: Regulatory violations

**The philosophy**: Better to fail loudly than to silently corrupt data.

---

## Mutations: Synchronous Execution

### How Mutations Work

```
Client sends mutation
         │
         ▼
FraiseQL Server receives
         │
         ├─ Validation (schema, authorization)
         │
         ├─ Acquire distributed locks (if federation)
         │
         ├─ Execute SAGA
         │  ├─ Step 1: Local DB mutation
         │  ├─ Step 2: Remote service mutation
         │  └─ Compensation logic on failure
         │
         └─ Return result to client
            (success or error, never "maybe")

         ⏱️ Client waits 100-500ms (blocking)
```

**Key point**: The client blocks until the mutation completes. There's no "queued, we'll process later" response.

### Example

```graphql
mutation CreateOrder($input: CreateOrderInput!) {
  createOrder(input: $input) {
    id
    status
    items { id, quantity }
  }
}
```

**What happens**:

1. FraiseQL validates order
2. Reserves inventory in one database
3. Calls payment service via federation
4. If payment fails: rolls back inventory (SAGA compensation)
5. Returns complete result or error to client

**Never**: "Order created, payment processing in background, check back later"

---

## Observations: Asynchronous Side Effects

### NATS JetStream ≠ Eventual Consistency Mutations

FraiseQL uses NATS JetStream for **side effects**, not **core mutations**:

```
Mutation (synchronous, blocking)
    ├─ Database: updateUser(...) ✅ completes
    └─ Returns to client immediately

Side Effects (asynchronous, via NATS)
    ├─ Webhook: Discord notification → queued
    ├─ Cache: Invalidate user cache → queued
    ├─ Events: Publish user.updated → published
    └─ Background jobs: process in Redis queue
```

**The mutation completes synchronously.**

**The side effects happen asynchronously.**

### Guarantees for Observations

| Feature | Guarantee |
|---------|-----------|
| **Webhook delivery** | At-least-once (may retry) |
| **Event publishing** | Durable (persisted in JetStream) |
| **Cache invalidation** | Best-effort (failures go to DLQ) |
| **Event ordering** | Per-entity ordered, not globally |

**Example**:
```graphql
mutation DeleteUser($id: ID!) {
  deleteUser(id: $id) {
    id
    deletedAt
  }
}
```

**Timeline**:

- `T+0ms`: Mutation executes (synchronous)
- `T+10ms`: Client receives response (user deleted)
- `T+50ms`: Webhook queued to NATS
- `T+200ms`: Discord webhook dispatched
- `T+500ms`: Cache invalidation completes

If the webhook fails, it retries. If it permanently fails, it goes to Dead Letter Queue. **But the mutation already succeeded.**

---

## Federation: Distributed Transactions via SAGA

### SAGA Pattern Implementation

When mutations span multiple services:

```
Mutation on Service A and Service B
         │
         ├─ Acquire locks on both databases
         │
         ├─ Execute mutation on Service A
         │  └─ Store result in SAGA store
         │
         ├─ Execute mutation on Service B
         │  └─ If fails: COMPENSATION phase
         │
         └─ Compensation (if needed)
            ├─ Undo Service B change (if it succeeded)
            ├─ Undo Service A change
            └─ Return error to client
```

### Example: Multi-Service Mutation

```graphql
mutation TransferInventory(
  $productId: ID!
  $fromWarehouse: ID!
  $toWarehouse: ID!
  $quantity: Int!
) {
  moveInventory(
    productId: $productId
    from: $fromWarehouse
    to: $toWarehouse
    quantity: $quantity
  ) {
    success
    fromBalance
    toBalance
  }
}
```

**Step-by-step execution**:

1. Acquire locks on both warehouse databases
2. Decrement inventory at `fromWarehouse`
3. Call `toWarehouse` service to increment
4. If step 3 fails:
   - **Compensation**: Re-increment `fromWarehouse` (undo step 2)
   - Return error to client
5. If both succeed: commit, return success to client

**Result**: Either both services updated, or neither. No partial states.

---

## When CP is Right ✅

### Use FraiseQL if:

| Domain | Why |
|--------|-----|
| **Banking/Payments** | Double-charging or lost transactions are unacceptable |
| **Inventory Management** | Overselling lost inventory costs money |
| **Healthcare** | Incorrect patient data causes harm |
| **Financial Reporting** | Stale data violates regulations (SOX, GDPR) |
| **Enterprise SaaS** | Customers expect data consistency guarantees |
| **Regulated Industries** | Audit trails require certainty |
| **Multi-tenant Systems** | One tenant's data can't bleed into another's |

---

## When CP is Wrong ❌

### Don't use FraiseQL if:

| Domain | Why | Better Choice |
|--------|-----|---|
| **Real-time Analytics** | 5-10s lag acceptable | DynamoDB, Cassandra |
| **Social Media** | Like counts approximated | DynamoDB, Cassandra |
| **IoT / Time Series** | Some data points acceptable | InfluxDB, TimescaleDB |
| **User Presence** | Eventual sync (30s) ok | Redis, Firebase |
| **Chat / Messaging** | Message reordering acceptable | Message broker (Kafka) |
| **Trending Topics** | Slightly stale data ok | Elasticsearch |
| **Session Storage** | Temporary data, relaxed consistency | Redis |

---

## Consistency Guarantees Explained

### Within a Single Database

**Isolation Level**: Serializable (ACID guarantees)

```sql
-- FraiseQL uses serializable transactions
-- Equivalent to:
BEGIN ISOLATION LEVEL SERIALIZABLE;
UPDATE users SET name = 'Alice' WHERE id = 1;
SELECT * FROM users WHERE id = 1;  -- Sees 'Alice'
COMMIT;
```

**Guarantee**: No dirty reads, no phantom reads, no lost updates.

### Across Federated Services

**Isolation Level**: Causal consistency (not strict serializability)

```
Service A executes mutation
    ↓
Service B waits for result
    ↓
Service B can see the effects of Service A's mutation
    ↓
But Service A can't retroactively see what Service B did
```

**Guarantee**: Ordered causality, not global ordering.

**Example**: You can't have this scenario:
```
Time T1: Service A changes User.name → "Alice"
Time T2: Service B reads User.name → gets "Bob" (stale)
Time T3: Service B returns response to client
```

Because SAGA ensures T1's effects are visible in T2.

---

## Multi-Tenant Isolation

### Tenant Data Must Not Cross

FraiseQL enforces strict per-tenant data scoping:

```graphql
# Configured at schema compile time
query users(tenantId: ID!) {
  users(where: { tenantId: { _eq: $tenantId } }) {
    id
    name
  }
}
```

**Guarantee**: No query can accidentally leak Tenant A's data to Tenant B.

**How**: Field-level authorization + WHERE filter compilation.

---

## What About Eventual Consistency?

### FraiseQL Does NOT Provide It

You cannot do:

```graphql
mutation UpdateUser($id: ID!, $name: String!) {
  updateUser(id: $id, name: $name) {
    id
    status  # "accepted" or "queued"
  }
}
```

And then:

```graphql
subscription onUserUpdate($id: ID!) {
  userUpdated(id: $id) {
    id
    name
    status  # "completed"
  }
}
```

**Why not?** Because:

1. It's complex to implement correctly
2. Enterprise users don't want it
3. We prioritize certainty over speed

### If You Need Eventual Consistency

Use a system designed for it:

- **DynamoDB** (AWS) → eventually consistent reads
- **Cassandra** → eventual consistency by design
- **Event sourcing** → with CQRS pattern

Or implement it yourself:

- Queue mutations to async processor
- Return job ID immediately
- Client polls status endpoint

---

## Performance Implications

### Latency Tradeoff

```
FraiseQL (CP)         100-500ms per mutation
  ├─ Validation: 5ms
  ├─ SAGA execution: 50-400ms (database dependent)
  └─ Network: 10-50ms

Eventual Consistency  <10ms mutation response
  ├─ ACK: 1-2ms
  ├─ Actual processing: later
  └─ Client waits for subscription
```

**FraiseQL is slower for individual mutations.**

**But you get certainty.**

### Throughput

| Scenario | FraiseQL | DynamoDB |
|----------|----------|----------|
| Simple query | 1,000 req/s | 10,000+ req/s |
| Complex federation mutation | 100 concurrent transactions | N/A (not designed for this) |
| Highly read-heavy | 5,000 req/s | 50,000 req/s |
| Write-heavy with consistency | 1,000 writes/s | Can't guarantee consistency |

**FraiseQL prioritizes correctness over raw throughput.**

---

## Decision Tree: Is FraiseQL Right for Me?

```

1. Do mutations need to complete before returning to client?
   YES → Continue
   NO → Use eventual consistency system

2. Can stale data cause problems?
   YES → Continue
   NO → Use AP system (DynamoDB, Cassandra)

3. Do you need strong ACID compliance?
   YES → Continue
   NO → Simpler systems work

4. Do you need to distribute transactions across services?
   YES → FraiseQL is ideal (SAGA + federation)
   NO → Any GraphQL engine works

5. Can you tolerate 100-500ms mutation latency?
   YES → FraiseQL is perfect
   NO → Use eventual consistency system
```

**If you answer YES to questions 1-5, use FraiseQL.**

---

## FAQ

### Q: Why doesn't FraiseQL queue mutations and return immediately?

**A**: Because that would require:

1. Subscriptions (WebSocket) for client status polling
2. Event sourcing for tracking mutation progress
3. Eventual consistency guarantees (we don't provide this)

It's simpler and more reliable to execute synchronously.

### Q: Can I use FraiseQL with async mutations?

**A**: Not natively. But you can:

- Return immediately with a job ID
- Implement a separate job status endpoint
- Use webhooks for notifications

See [Pattern: Async Mutations](../patterns/async-mutations.md) for implementation guide.

### Q: What happens if a SAGA step fails?

**A**: Automatic compensation:

1. FraiseQL rolls back all previous steps in reverse order
2. Releases locks
3. Returns error to client

The mutation either succeeds completely or fails cleanly. No partial states.

### Q: Is FraiseQL slower than other GraphQL engines?

**A**: Depends what you measure:

- **Individual mutation latency**: Yes, slightly slower (blocking for consistency)
- **Complex join queries**: No, faster (compile-time optimization)
- **Federation queries**: Yes, slower (SAGA coordination overhead)
- **Data accuracy**: Much faster (no stale data surprises)

### Q: Can I use FraiseQL for real-time features?

**A**: Depends:

- **Real-time presence**: No (eventual consistency is fine)
- **Real-time data updates**: Yes (WebSocket subscriptions work)
- **Real-time notifications**: Yes (webhooks + CDC)
- **Real-time analytics**: No (strong consistency unnecessary)

---

## Troubleshooting

### "Mutation taking too long (>1 second)"

**Cause:** Synchronous consistency requirement means mutations wait for database locks and replication.

**Diagnosis:**
1. Check database performance: `EXPLAIN ANALYZE` on mutation query
2. Check network latency between services: `ping federation-subgraph`
3. Monitor database locks: `SELECT * FROM pg_locks;`

**Solutions:**
- Add database indexes on frequently mutated columns
- Scale database horizontally (more replicas for read distribution)
- For federation, consider async job pattern (see pattern guide)
- Verify network is low-latency between datacenters

### "Stale data in replicas during failover"

**Cause:** Strong consistency only within single primary. Replicas lag during network partitions.

**Diagnosis:**
1. Check replication lag: PostgreSQL `SELECT now() - pg_last_xact_replay_timestamp();`
2. Monitor partition detection: Check FraiseQL logs for "partition detected"
3. Verify replica freshness before routing queries

**Solutions:**
- Route all writes to primary, reads can use replicas with acceptable lag
- Set up automatic replica promotion (e.g., Patroni, Pg-failover)
- Monitor replication lag continuously (set alerts at >5s lag)
- Document acceptable stale-data window for your use case

### "Federation query returns partial data"

**Cause:** SAGA coordination timeout or subgraph unavailability.

**Diagnosis:**
1. Check SAGA logs for "compensation triggered"
2. Verify all subgraphs are responding: `curl http://subgraph:8000/health`
3. Check network connectivity: `ping subgraph-service`
4. Review query timeout settings in fraiseql.toml

**Solutions:**
- Increase SAGA timeout (default 30s may be too aggressive): `saga_timeout_secs = 60`
- Verify all subgraphs are reachable and responsive
- Check if subgraph database is slow (may need optimization)
- Consider splitting complex federation queries into separate requests

### "Different data visible in federation subgraphs"

**Cause:** Each subgraph uses its own database. Mutations haven't fully replicated yet.

**Diagnosis:**
1. Query same entity from multiple subgraphs: `{ user(id: "X") { id } }`
2. Check replication lag between databases
3. Verify transaction order in audit logs

**Solutions:**
- This is expected during normal operation (strong consistency within each subgraph)
- For critical consistency, ensure application waits for replication
- Use federation readiness checks to detect lag
- Consider using `@requires` directive to create implicit ordering dependencies

### "High lock contention on frequently updated records"

**Cause:** Multiple simultaneous mutations on same entity cause database locks.

**Diagnosis:**
1. Find locked rows: `SELECT * FROM pg_locks WHERE NOT granted;`
2. Identify blocking queries: `SELECT * FROM pg_stat_statements WHERE calls > 1000;`
3. Monitor lock wait times in application logs

**Solutions:**
- Add database indexes on WHERE clauses in mutations
- Reduce mutation frequency if possible (batch updates)
- Consider partitioning frequently updated tables
- Implement optimistic locking if conflict is acceptable

### "Partition tolerance: system becomes unavailable instead of serving stale data"

**This is expected behavior.** FraiseQL chooses consistency over availability.

**Diagnosis:**
1. Confirm this is intentional choice for your use case
2. If not acceptable, you need different architecture

**Solutions:**
- If high availability is critical, implement caching layer (Redis) for reads during partition
- Use circuit breakers to detect partitions early
- Implement graceful degradation (serve cached data with disclaimer)
- Document expected outage windows for users

---

## Related Documentation

- [Production Deployment](./production-deployment.md) - How to scale FraiseQL
- [SAGA Pattern Details](../architecture/federation-saga.md) - Deep dive into transaction coordination
- [Multi-Tenant Isolation](../enterprise/multi-tenancy.md) - How we guarantee tenant data separation
- [Async Mutations Pattern](../patterns/async-mutations.md) - Implementing eventual-consistency-like features
