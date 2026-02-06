<!-- Skip to main content -->
---

title: Consistency Model
description: FraiseQL provides **strict serializable consistency** within a single database instance and **causal consistency** across federated instances. This document spe
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# Consistency Model

**Version:** 1.0
**Status:** Complete
**Date:** January 11, 2026
**Audience:** Architects, database engineers, enterprise evaluators

---

## 1. Overview

FraiseQL provides **strict serializable consistency** within a single database instance and **causal consistency** across federated instances. This document specifies the consistency guarantees clients can rely on.

### 1.1 Core Principle

> **What the database guarantees, FraiseQL guarantees.**

FraiseQL does not weaken database consistency. If the database provides serializable transactions, FraiseQL preserves that. If the database provides eventual consistency, FraiseQL preserves that.

### 1.2 Consistency Levels Supported

| Database | Consistency Level | Isolation | Multi-Version | TTL |
|----------|-------------------|-----------|---------------|-----|
| **PostgreSQL** | Serializable | ✅ Full ACID | ✅ MVCC | N/A |
| **MySQL** | Serializable | ✅ Full ACID | ✅ InnoDB MVCC | N/A |
| **SQL Server** | Serializable | ✅ Full ACID | ✅ Snapshot | N/A |
| **SQLite** | Serializable | ✅ Full ACID | ⚠️ Limited | N/A |

---

## 2. Single-Database Consistency (Primary Guarantee)

### 2.1 ACID Transaction Guarantees

FraiseQL queries and mutations respect the database's ACID properties:

#### 2.1.1 Atomicity

**What it means:** Mutation either fully succeeds or fully fails. No partial updates.

**Scope:** Single mutation operation

- Mutation: `createUser(name: "Bob", email: "bob@example.com")`
- All side effects apply, or none apply
- No partial state visible to other queries

**Guarantee:**

```python
<!-- Code example in Python -->
# Before mutation
SELECT COUNT(*) FROM users  # 100

# Mutation fails (email constraint violation)
mutation {
  createUser(name: "Bob", email: "duplicate@example.com") { id }
}

# After mutation
SELECT COUNT(*) FROM users  # Still 100 (no partial insert)
```text
<!-- Code example in TEXT -->

#### 2.1.2 Consistency (Logical)

**What it means:** Database integrity constraints are never violated.

**Scope:** All queries, mutations, subscriptions

- Foreign key constraints enforced
- Unique constraints enforced
- Check constraints enforced
- Referential integrity maintained

**Guarantee:**

```python
<!-- Code example in Python -->
# Foreign key constraint: order.user_id → user.id
# This mutation will fail (invalid user_id):
mutation {
  createOrder(user_id: 9999, amount: 100) { id }  # Returns error
}

# After error, database state is unchanged
```text
<!-- Code example in TEXT -->

#### 2.1.3 Isolation

**What it means:** Concurrent operations don't interfere with each other.

**Isolation levels** (in order of strictness):

| Level | Dirty Reads | Non-Repeatable | Phantom | Implementation |
|-------|-------------|----------------|---------|-----------------|
| **Read Uncommitted** | ❌ Not in FraiseQL | - | - | - |
| **Read Committed** | ✅ Prevented | ⚠️ Possible | ⚠️ Possible | PostgreSQL default |
| **Repeatable Read** | ✅ Prevented | ✅ Prevented | ⚠️ Possible | MySQL default |
| **Serializable** | ✅ Prevented | ✅ Prevented | ✅ Prevented | ✅ FraiseQL default |

**FraiseQL isolation guarantee:** Queries and mutations operate at **Serializable** isolation level.

```python
<!-- Code example in Python -->
# Two concurrent mutations (serializable isolation)
# Client A:
mutation {
  updateUser(id: 1, name: "Alice Update 1") { name }
}

# Client B:
mutation {
  updateUser(id: 1, name: "Alice Update 2") { name }
}

# Result: One succeeds, one fails (conflict detected)
# Never: Both succeed with mixed updates
```text
<!-- Code example in TEXT -->

#### 2.1.4 Durability

**What it means:** Once mutation succeeds, it persists even after failure.

**Scope:** Confirmed mutations

- Mutation returns success response (in GraphQL `data` field, not `errors`)
- Database has written change to durable storage
- Change survives server restart, power loss, etc.

**Guarantee:**

```python
<!-- Code example in Python -->
# Mutation succeeds (returns in data field)
mutation {
  createUser(name: "Bob") { id }
  # Response received by client with data: { createUser: { id: 123 } }
}

# Server crashes immediately after
# Database restart: Change still there

# Later query:
query {
  user(id: 123) { name }  # Returns "Bob"
}
```text
<!-- Code example in TEXT -->

**Non-guarantee:**

```python
<!-- Code example in Python -->
# Mutation fails (returns in errors field)
mutation {
  createUser(name: "Alice", email: "duplicate@example.com") { id }
  # Response received with errors: [...], data: null
}

# Server crashes
# Database restart: Change was never applied (not durable)
```text
<!-- Code example in TEXT -->

---

## 3. Read Consistency

### 3.1 Read-After-Write Consistency (RAW)

**What it means:** After a write succeeds, subsequent reads see the write.

**Scope:** Same client connection

```python
<!-- Code example in Python -->
# Write succeeds
mutation {
  updateUser(id: 1, name: "Alice") { name }
  # Response: data: { updateUser: { name: "Alice" } }
}

# Subsequent read sees the write
query {
  user(id: 1) { name }  # Returns "Alice"
}
```text
<!-- Code example in TEXT -->

**Strong guarantee:** Applies at all times, for all clients.

### 3.2 Read-Your-Writes Consistency (RYW)

**What it means:** Client always sees results of its own writes, even if different server processes handle the requests.

**Scope:** Same authenticated user

```python
<!-- Code example in Python -->
# Request 1: Write to Server A
mutation {
  updateUserProfile(name: "NewName") { name }
}

# Request 2: Read from Server B (different process)
query {
  me { name }  # Returns "NewName"
}
```text
<!-- Code example in TEXT -->

**Implementation:** Achieved via:

- Database replication (all writes go to primary)
- Session affinity (client sticky to server)
- Causal token tracking (server sends version, client tracks)

### 3.3 Monotonic Reads

**What it means:** Client never sees a version of data earlier than a previous read.

**Scope:** Same client over time

```python
<!-- Code example in Python -->
# Read 1: User has 5 posts
query { user(id: 1) { posts { count } } }
# Returns: 5

# Wait 1 second, some other client adds a post

# Read 2: User still sees 5 posts (at worst)
query { user(id: 1) { posts { count } } }
# Returns: >= 5 (5 or 6, never less than 5)
```text
<!-- Code example in TEXT -->

**Strong guarantee:** FraiseQL never shows older versions of data.

### 3.4 Consistent Prefix Reads

**What it means:** Client sees causally-consistent sequence of updates, not out-of-order.

**Scope:** Related data across reads

```python
<!-- Code example in Python -->
# Event 1: Create order (id: 100)
mutation { createOrder(user_id: 1, amount: 100) { id } }

# Event 2: Create order update (status: shipped)
mutation { updateOrder(id: 100, status: "shipped") { status } }

# Client reads:
query { order(id: 100) { status } }
# Never sees: order doesn't exist yet (would be out-of-order)
# Always sees: order with status = "shipped" (or not created yet)
```text
<!-- Code example in TEXT -->

---

## 4. Write Consistency

### 4.1 Serializable Writes

**What it means:** Concurrent writes are serialized (don't interleave).

**Scope:** All mutations

```python
<!-- Code example in Python -->
# Concurrent mutations on same row
# Client A: UPDATE user SET balance = balance - 100 WHERE id = 1
# Client B: UPDATE user SET balance = balance - 50 WHERE id = 1

# Initial balance: 1000

# Result: One succeeds first, second uses updated value
# Possible: A succeeds (900), B succeeds (850) ✅
# Never: Both use original (900 and 950) ❌
```text
<!-- Code example in TEXT -->

### 4.2 Transaction Isolation for Multi-Statement

**What it means:** Multi-statement mutations are atomic.

**Scope:** Multiple queries within single mutation (future feature)

```python
<!-- Code example in Python -->
mutation {
  # Statement 1
  createOrder(user_id: 1) { id }
  # Statement 2
  updateUser(id: 1, balance: balance - 100) { balance }
  # Statement 3
  createAuditLog(action: "order_created") { id }

  # All succeed or all fail together
  # No partial state visible
}
```text
<!-- Code example in TEXT -->

### 4.3 Write Conflicts

**What it means:** Conflicting writes are detected and one fails.

**Scope:** Concurrent modifications

```python
<!-- Code example in Python -->
# Client A: updateUser(id: 1, name: "Alice", version: 5)
# Client B: updateUser(id: 1, name: "Bob", version: 5)

# Both use same version (5)
# Result: A succeeds (version → 6), B fails with conflict error
```text
<!-- Code example in TEXT -->

---

## 5. Multi-Database Consistency (Federation)

### 5.1 Causal Consistency Guarantee

When FraiseQL fetches federated entities from multiple databases, it provides **causal consistency**:

**Definition:** If operation A causally affects operation B, every observer sees A before B.

**Example:**

```python
<!-- Code example in Python -->
# Database 1: Users
# Database 2: Orders

# Operation A: Create order in Database 2
mutation {
  createOrder(user_id: 1) { id }  # → order_123
}

# Operation B: Query user + their orders (from both databases)
query {
  user(id: 1) {
    name
    orders { id }  # Includes order_123
  }
}

# Causal guarantee:
# - Client always sees order_123 in the user's orders
# - Never sees state before order was created
# - Even if databases are replicas with lag
```text
<!-- Code example in TEXT -->

### 5.2 Consistency Across Databases

**Single-Database Query (Strict Serializable):**

```text
<!-- Code example in TEXT -->
Database: PostgreSQL
Consistency: Serializable ACID
Latency: <10ms
```text
<!-- Code example in TEXT -->

**Federated Query (Causal Consistent):**

```text
<!-- Code example in TEXT -->
Database 1: PostgreSQL (Users)
Database 2: MySQL (Orders)
Consistency: Causal
Latency: <100ms (slower due to coordination)
```text
<!-- Code example in TEXT -->

**Trade-off:** Federation sacrifices strict serializability for scalability.

### 5.3 No Cross-Database Transactions

**Important:** FraiseQL does NOT provide distributed transactions across federated databases.

```python
<!-- Code example in Python -->
# This does NOT have cross-database atomicity:
mutation {
  createUser(name: "Alice") { id }  # Database 1
  createOrder(user_id: NEW_ID) { id }  # Database 2
}

# Why: Distributed 2-phase commit is too expensive
# If operation fails: Rollback may be partial

# Instead: Client should handle failure
if not mutation_succeeded:
  client should retry both or roll back manually
```text
<!-- Code example in TEXT -->

### 5.4 Federation via Database Linking (Optimized)

When using database-level federation (FDW, Linked Servers), consistency is **stricter**:

```python
<!-- Code example in Python -->
# PostgreSQL FDW federation
# Both databases: PostgreSQL
Consistency: Serializable ACID (via FDW)

# vs HTTP federation
# Any databases, via HTTP
Consistency: Causal
```text
<!-- Code example in TEXT -->

---

## 6. Subscription Consistency

### 6.1 Event Ordering Guarantees

Subscriptions provide **per-entity ordering** of events:

```python
<!-- Code example in Python -->
# Subscription: orderUpdated(where: { id: 100 })
orderUpdated { id, status, timestamp }

# Events for same entity are ordered:
Event 1: status = "pending" (timestamp: T1)
Event 2: status = "shipped" (timestamp: T2)
Event 3: status = "delivered" (timestamp: T3)

# Client always sees events in this order
# Never: Event 3 before Event 1
```text
<!-- Code example in TEXT -->

### 6.2 Event Delivery Consistency

**At-least-once delivery:**

- Events are delivered at least once
- Client may receive duplicates (same event twice)
- Client should be idempotent

```python
<!-- Code example in Python -->
# Same event delivered twice (network retry)
{
  "event_id": "evt_12345",
  "data": { "id": 100, "status": "shipped" }
}
{
  "event_id": "evt_12345",  # Same ID
  "data": { "id": 100, "status": "shipped" }
}

# Client should check event_id and skip duplicates
```text
<!-- Code example in TEXT -->

### 6.3 No Cross-Entity Ordering

Events from different entities may arrive out-of-order:

```python
<!-- Code example in Python -->
# Subscription: orderUpdated + userUpdated

# Database timeline:
T1: Order 100 updated → Event A
T2: User 5 updated → Event B
T3: Order 200 updated → Event C

# Client may receive:
Event B, Event C, Event A  # Out of original order!

# Why: Events are per-entity ordered, not globally ordered
```text
<!-- Code example in TEXT -->

---

## 7. Caching Consistency

### 7.1 Cache Invalidation on Write

When a mutation succeeds, all related cache entries are invalidated:

```python
<!-- Code example in Python -->
# Initial query (cached)
query { user(id: 1) { name posts { id } } }
# Cache key: user:1, user:1:posts
# Cached result: name="Alice", posts=[100, 101]

# Mutation
mutation { updateUser(id: 1, name: "Bob") { name } }

# Automatic cache invalidation:
# - user:1 invalidated ✓
# - user:1:posts invalidated ✓
# - related:user:1 queries invalidated ✓

# Next query (cache miss, fresh from database)
query { user(id: 1) { name } }
# Cache hit: name="Bob"
```text
<!-- Code example in TEXT -->

### 7.2 Cache TTL (Time-to-Live)

Some queries use stale-cache-acceptable-time (SCAT):

```python
<!-- Code example in Python -->
# Query cached for 60 seconds max
query @cacheControl(maxAge: 60) {
  products { id name }
}

# Fresh if: query < 60s old
# Stale if: query > 60s old, re-fetch from database
```text
<!-- Code example in TEXT -->

### 7.3 Cache Coherence

**Strong cache coherence:** All clients see consistent cache results.

- Cache invalidations are broadcast
- All servers invalidate same keys
- No stale data persists across servers

---

## 8. Consistency Under Failures

### 8.1 Database Unavailable

**Query:** Returns error, no partial data

```python
<!-- Code example in Python -->
query { user(id: 1) { name } }
# Database is down
# Returns: ERROR, data: null
```text
<!-- Code example in TEXT -->

**Mutation:** Returns error, no changes applied

```python
<!-- Code example in Python -->
mutation { updateUser(id: 1, name: "Bob") { name } }
# Database is down
# Returns: ERROR, data: null
# Database unchanged
```text
<!-- Code example in TEXT -->

### 8.2 Connection Lost Mid-Query

**Before response sent:** Client sees error, no data
**After response sent:** Data is consistent (database committed)

### 8.3 Server Crash

**Durable mutations:** Persisted to database (ACID durability)
**Cache:** Invalidated on restart (refreshed from database)
**In-flight requests:** Clients receive errors, must retry

---

## 9. Eventual Consistency (Not Provided)

**Important:** FraiseQL does NOT provide eventual consistency by default.

```python
<!-- Code example in Python -->
# This is NOT eventually consistent:
mutation { updateUser(id: 1, name: "Alice") }
query { user(id: 1) { name } }  # Sees "Alice" immediately

# FraiseQL always returns immediately-consistent results
```text
<!-- Code example in TEXT -->

**If you need eventual consistency:** Use event-driven architecture with subscriptions + external systems.

---

## 10. Consistency Levels by Operation

| Operation | Consistency | Isolation | Write Atomicity | Read Freshness |
|-----------|-------------|-----------|-----------------|-----------------|
| **Query** | Serializable | Serializable | N/A | Immediate |
| **Mutation** | Serializable | Serializable | Atomic | Immediate |
| **Subscription** | Per-entity ordered | Serializable | N/A | At-least-once |
| **Federated Query** | Causal | Serializable per DB | N/A | Causal |
| **Federated Mutation** | Causal | Serializable per DB | Per-DB atomic | Causal |
| **Cached Query** | Within TTL | Serializable | N/A | Max TTL stale |

---

## 11. Consistency Configuration

### 11.1 Per-Query Consistency Control

```python
<!-- Code example in Python -->
query @consistency(level: "serializable") {
  user(id: 1) { name }
}

# Levels:
# - "serializable" (default): Strongest consistency
# - "causal" (federation): Weaker but faster
# - "eventual" (future): Weakest but fastest
```text
<!-- Code example in TEXT -->

### 11.2 Per-Mutation Consistency Control

```python
<!-- Code example in Python -->
mutation @consistency(level: "serializable", timeout: 30000) {
  updateUser(id: 1, name: "Bob") { name }
}

# Options:
# - timeout: Max time to wait for lock (ms)
# - retry: Auto-retry on conflict (true/false)
```text
<!-- Code example in TEXT -->

---

## 12. Consistency Guarantees by Database

### 12.1 PostgreSQL

**Consistency Level:** Serializable ACID
**Mechanism:** Serializable Snapshot Isolation (SSI)
**Guarantee:**

```text
<!-- Code example in TEXT -->
✅ Serializable isolation
✅ MVCC (Multi-Version Concurrency Control)
✅ Write-ahead logging (durability)
✅ Atomic transactions
```text
<!-- Code example in TEXT -->

### 12.2 MySQL (InnoDB)

**Consistency Level:** Repeatable Read (default) → Serializable (FraiseQL default)
**Mechanism:** MVCC + Gap locks
**Guarantee:**

```text
<!-- Code example in TEXT -->
✅ Serializable isolation (with locks)
✅ MVCC
✅ Binary logging (durability)
✅ Atomic transactions
```text
<!-- Code example in TEXT -->

### 12.3 SQL Server

**Consistency Level:** Serializable ACID
**Mechanism:** Snapshot isolation + row versioning
**Guarantee:**

```text
<!-- Code example in TEXT -->
✅ Serializable isolation
✅ Snapshot isolation available
✅ Write-ahead logging (durability)
✅ Atomic transactions
```text
<!-- Code example in TEXT -->

### 12.4 SQLite

**Consistency Level:** Serializable ACID
**Mechanism:** Write-ahead logging + locking
**Guarantee:**

```text
<!-- Code example in TEXT -->
⚠️ Limited MVCC (single file)
✅ Atomic transactions
✅ Durability
❌ Limited concurrency (file-level locking)
```text
<!-- Code example in TEXT -->

---

## 13. Consistency Anti-Patterns

### 13.1 Assuming Eventual Consistency

**❌ WRONG:**

```python
<!-- Code example in Python -->
# This is NOT eventually consistent!
mutation { updateUser(id: 1, name: "Alice") }
time.sleep(1)
result = query { user(id: 1) { name } }
# Assuming result might be stale

# Result: Always sees "Alice" immediately
```text
<!-- Code example in TEXT -->

**✅ RIGHT:**

```python
<!-- Code example in Python -->
mutation { updateUser(id: 1, name: "Alice") }
# Result: Always consistent immediately, no need to wait
result = query { user(id: 1) { name } }  # Sees "Alice"
```text
<!-- Code example in TEXT -->

### 13.2 Assuming Cross-Database Transactions

**❌ WRONG:**

```python
<!-- Code example in Python -->
mutation {
  createUser(name: "Alice") { id }  # Database 1
  createOrder(user_id: NEW_ID) { id }  # Database 2
}

# Assuming both succeed or both fail
# Actually: May partially succeed
```text
<!-- Code example in TEXT -->

**✅ RIGHT:**

```python
<!-- Code example in Python -->
# Handle failure manually
try {
  user = mutation { createUser(name: "Alice") { id } }
  order = mutation { createOrder(user_id: user.id) { id } }
} catch error {
  # May have created user but not order
  # Client should rollback/compensate
}
```text
<!-- Code example in TEXT -->

### 13.3 Assuming Global Event Ordering

**❌ WRONG:**

```python
<!-- Code example in Python -->
subscription {
  orderUpdated { id, status }
  userUpdated { name }
}

# Assuming events are globally ordered by timestamp
# Actually: Only per-entity ordered
```text
<!-- Code example in TEXT -->

**✅ RIGHT:**

```python
<!-- Code example in Python -->
subscription {
  orderUpdated { id, status, timestamp }
}

# Use timestamp to order events in client
events.sort(key=lambda e: e["timestamp"])
```text
<!-- Code example in TEXT -->

---

## 14. Consistency SLA (Service Level Agreement)

| Metric | Target | Measured |
|--------|--------|----------|
| **Serializable consistency** | 99.99% | All mutations |
| **Read-after-write latency** | <10ms | p99 |
| **ACID durability** | 100% | Confirmed mutations |
| **Causal consistency (federation)** | 99.9% | Federated queries |
| **Cache coherence** | 99.99% | Cross-server caches |

---

## Summary

**FraiseQL consistency model:**

✅ **Single database:** Serializable ACID (strict)
✅ **Federated:** Causal consistency (weaker but scalable)
✅ **Subscriptions:** Per-entity ordered (eventual delivery)
✅ **Caching:** Strong cache coherence
✅ **Durability:** Write-ahead logging
✅ **Atomicity:** All-or-nothing mutations

**Golden rule:** What the database guarantees, FraiseQL guarantees. Nothing more, nothing less.

---

*End of Consistency Model*
