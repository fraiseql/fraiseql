# Failure Modes and Recovery

**Version:** 1.0
**Status:** Complete
**Date:** January 11, 2026
**Audience:** Operations engineers, SREs, infrastructure architects, security teams

---

## 1. Overview

This document specifies how FraiseQL fails and recovers. Understanding failure modes enables operators to design resilient deployments and understand recovery time objectives (RTO) and recovery point objectives (RPO).

### 1.1 Design Philosophy

> **Fail fast, recover gracefully.**

When FraiseQL encounters failure, it:

1. **Detects** the failure immediately
2. **Stops processing** (no partial/corrupt state)
3. **Reports** error to client
4. **Recovers** automatically where possible
5. **Notifies** operations team for manual intervention if needed

### 1.2 Recovery Time Objectives (RTO)

| Component | RTO | Automatic | Manual |
|-----------|-----|-----------|--------|
| **Single FraiseQL instance** | < 30 seconds | ✅ Restart | N/A |
| **FraiseQL cluster (n instances)** | < 5 seconds | ✅ Failover | N/A |
| **Database connection** | < 5 seconds | ✅ Reconnect | N/A |
| **Entire database** | 5-60 minutes | ❌ No | ✅ DBA |
| **Cache backend** | < 5 seconds | ✅ Miss → DB | N/A |
| **Federation subgraph** | < 30 seconds | ✅ Retry | ✅ escalate |
| **Authentication provider** | 5-30 minutes | ❌ No | ✅ Escalate |

### 1.3 Recovery Point Objectives (RPO)

| Component | RPO | Data Loss |
|-----------|-----|-----------|
| **Committed mutations** | 0 (durable) | None |
| **In-flight mutations** | Transaction boundary | None (atomic) |
| **Cache entries** | Variable (TTL) | Query re-execution needed |
| **Subscription events** | Variable (buffer TTL) | Events may be missed |
| **Federation caches** | < 1 second | Stale reads possible |

---

## 2. Failure Modes by Component

### 2.1 FraiseQL Runtime Failures

#### 2.1.1 Application Crash (Process Dies)

**When:** Runtime panic, OOM, segfault, kill -9

**Client Impact:**

```text
Immediate: All in-flight requests fail
Response: Connection reset / 502 Bad Gateway
Error: E_INTERNAL_UNKNOWN_ERROR_703
```text

**Detection:** Load balancer health check fails (TCP connection refused)

**Recovery:**

- **Automatic:** Kubernetes restarts pod / systemd restarts service
- **RTO:** 5-30 seconds (depends on restart mechanism)
- **Data Impact:** None (database not affected)

**During Recovery:**

```text
T0: Process dies
T1-2s: Load balancer detects unhealthy
T3-5s: Requests rerouted to healthy instances
T5-30s: New instance starts up
T30s+: Instance healthy, accepts traffic
```text

#### 2.1.2 Memory Exhaustion (OOM)

**When:** Query result too large, memory leak, cache fills

**Client Impact:**

```text
Gradual: Queries become slower
Then: Connection timeouts / 503 Service Unavailable
Error: E_EXEC_LIMIT_EXCEEDED_405 or E_INTERNAL_PANIC_701
```text

**Detection:**

- Monitoring: Memory usage > 90%
- Automatic: Queries rejected if < 100MB free

**Recovery:**

- **Automatic:** Kill process, restart
- **Manual:** Reduce query complexity, add cache tuning
- **RTO:** 5-30 seconds
- **Prevention:** Set memory limits (container/VM)

#### 2.1.3 CPU Saturation

**When:** High query volume, expensive queries, denial of service

**Client Impact:**

```text
Gradual: Query latency increases (p99 > 30s)
Then: Query timeouts / slow client requests
Error: E_DB_POSTGRES_QUERY_TIMEOUT_302
```text

**Detection:**

- Monitoring: CPU usage > 90%
- Automatic: Query timeout enforcement

**Recovery:**

- **Automatic:** Queries rejected if CPU utilization > 95%
- **Manual:** Scale out to more instances
- **RTO:** Immediate (no restart needed, queries queue)
- **Prevention:** Rate limiting, query complexity limits

#### 2.1.4 Goroutine Leak / Resource Leak

**When:** Unbounded connection pool growth, subscription client leaks

**Client Impact:**

```text
Slow: Memory gradually increases over hours/days
Then: OOM crash (see 2.1.2)
```text

**Detection:**

- Monitoring: Goroutine count increasing over time
- Manual: pprof profiling

**Recovery:**

- **Manual:** Rolling restart of instances
- **RTO:** 5-30 seconds per instance
- **Prevention:** Connection limits, goroutine limits, monitoring

---

### 2.2 Database Connection Failures

#### 2.2.1 Connection Timeout

**When:** Database unreachable, network partition, DNS failure

**Client Impact:**

```text
Immediate: Query returns error
Response: HTTP 504 Gateway Timeout (after 5s wait)
Error: E_DB_POSTGRES_CONNECTION_FAILED_300
Retryable: YES
```text

**Detection:**

- Automatic: Connection attempt fails with timeout
- Monitoring: Connection time > threshold

**Recovery:**

- **Automatic:** Retry connection after backoff (1s, 2s, 4s, 8s)
- **Client:** Retry with exponential backoff
- **RTO:** < 30 seconds (depends on retry strategy)

**Connection Pool State:**

```text
Before: 10/20 connections in use
Error: Connection attempt fails
After: Connection returned to pool marked stale
Next query: Reconnection attempted
```text

#### 2.2.2 Connection Pool Exhaustion

**When:** More queries than pool size, slow query victims

**Client Impact:**

```text
Immediate: Connection request blocks
After 5s: Query times out
Response: HTTP 504 Gateway Timeout
Error: E_DB_CONNECTION_POOL_EXHAUSTED_301
Retryable: YES (with backoff)
```text

**Detection:**

- Automatic: Pool size < available connections
- Monitoring: Active connections > 90% of pool

**Recovery:**

- **Automatic:** Increase pool size (if below max)
- **Automatic:** Wait for active queries to complete
- **Manual:** Scale out to more instances
- **RTO:** Depends on query duration (typically <30s)

**Pool Configuration:**

```text
min_connections: 5
max_connections: 20
queue_timeout: 5000ms  // How long to wait

If 20 queries active + 5 queued:
  Query 26 waits 5s then times out
```text

#### 2.2.3 Connection Closed by Database

**When:** Database restarts, network interruption, auth failure

**Client Impact:**

```text
Request: Executing query
Error: Connection closed mid-query
Response: Partial/stale data or error
Error: E_DB_POSTGRES_CONNECTION_FAILED_300
Retryable: YES (connection reopened)
```text

**Detection:**

- Automatic: Network error reading response
- Automatic: Connection marked unhealthy

**Recovery:**

- **Automatic:** Reopen connection
- **Automatic:** Retry query on new connection
- **Max retries:** 3 attempts
- **RTO:** < 10 seconds

#### 2.2.4 Database Restart

**When:** Planned maintenance, crash, failover

**Client Impact:**

```text
During restart: All connections fail
Error: E_DB_POSTGRES_CONNECTION_FAILED_300
Duration: 10 seconds (pg restart) to 5 minutes (recovery)
Requests: Queued and retry
Queries: Some fail, some succeed after restart
```text

**Detection:**

- Automatic: Connection attempts fail
- Monitoring: Database not responding

**Recovery:**

- **Automatic:** Retry connection exponentially
- **All queries:** Fail initially, queued for retry
- **RTO:** 10 seconds - 5 minutes (database dependent)
- **Data Impact:** None (durable state persisted)

**During Database Restart:**

```text
T0: Database starts shutdown
T1-5s: In-flight queries error
T5s: All connections fail
T5-20s: Database restart process
T20s: Database comes online
T20-30s: Connections re-established
T30s+: Queries succeed again
```text

---

### 2.3 Database Execution Failures

#### 2.3.1 Query Timeout

**When:** Slow query, missing index, resource contention

**Client Impact:**

```text
Waiting: Client waits for result (up to 30s default)
Timeout: Query killed by database
Response: HTTP 504 Gateway Timeout
Error: E_DB_POSTGRES_QUERY_TIMEOUT_302
Retryable: YES (with better query)
```text

**Detection:**

- Automatic: Query execution > timeout threshold
- Monitoring: p99 query time > SLO

**Recovery:**

- **Automatic:** Kill query, return error
- **Client:** Retry with fewer results (add filter/limit)
- **Manual:** Add index, optimize query
- **RTO:** Immediate (query terminated)
- **Data Impact:** None (query doesn't write)

#### 2.3.2 Deadlock

**When:** Concurrent mutations conflicting

**Client Impact:**

```text
Execution: Queries start conflicting
Deadlock detected: One query victim selected
Response: HTTP 502 Bad Gateway
Error: E_DB_POSTGRES_DEADLOCK_303
Retryable: YES (automatic on retry)
```text

**Detection:**

- Automatic: Database detects and kills one query
- Monitoring: Deadlock count increasing

**Recovery:**

- **Automatic:** Runtime retries query automatically (up to 3x)
- **If persists:** Returns error, client retries
- **RTO:** < 1 second per retry
- **Data Impact:** First query's changes persist, second rolls back

**Deadlock Scenario:**

```text
Client A: UPDATE users SET balance -= 100 WHERE id = 1
         UPDATE orders SET total += 100 WHERE user_id = 1

Client B: UPDATE orders SET total -= 50 WHERE user_id = 1
         UPDATE users SET balance += 50 WHERE id = 1

Database: Detects circular dependency
         Kills Client B's transaction
         Client B retries and succeeds
```text

#### 2.3.3 Constraint Violation

**When:** Unique constraint, foreign key, check constraint

**Client Impact:**

```text
Execution: Constraint violation during INSERT/UPDATE
Response: HTTP 400 Bad Request
Error: E_DB_MYSQL_CONSTRAINT_VIOLATION_304
Retryable: NO
```text

**Detection:**

- Automatic: Database rejects write
- User-actionable: Clear error message

**Recovery:**

- **Client:** Fix data (use different email, valid user_id, etc.)
- **Retry:** Retry with corrected data
- **RTO:** User must fix (not automatic)

**No Recovery Needed:**

```text
Mutation { createUser(email: "taken@example.com") { id } }
→ Error: Unique constraint violation on email
→ User should: Use different email
→ Automatic retry: Will fail identically
```text

#### 2.3.4 Out of Memory (Database)

**When:** Query result too large, insufficient RAM

**Client Impact:**

```text
Execution: Query consumes too much memory
Response: HTTP 503 Service Unavailable
Error: E_DB_SQLSERVER_OUT_OF_MEMORY_307
Retryable: YES (reduce query size)
```text

**Detection:**

- Monitoring: Database memory > 90%
- Automatic: Query killed before consuming all memory

**Recovery:**

- **Client:** Retry with pagination (smaller batch size)
- **Manual:** Add WHERE filter, reduce JOIN complexity
- **Manual:** Increase database RAM
- **RTO:** Immediate (query killed)

#### 2.3.5 Disk Full

**When:** Database cannot write (INSERT/UPDATE fails)

**Client Impact:**

```text
Execution: Database cannot allocate space
Response: HTTP 503 Service Unavailable
Error: E_DB_MYSQL_DISK_FULL_308
Retryable: YES (after manual intervention)
```text

**Detection:**

- Monitoring: Disk usage > 95%
- Automatic: Database rejects writes

**Recovery:**

- **Manual:** DBA frees disk space
- **RTO:** 5-30 minutes (operational)
- **Data Impact:** Writes fail until space available
- **Reads:** Unaffected (can still query)

---

### 2.4 Cache Backend Failures

#### 2.4.1 Cache Server Down

**When:** Redis/Memcached instance crashes

**Client Impact:**

```text
Query: Cache miss (server unavailable)
Result: Query executes against database
Latency: Slower than cached (depends on query)
Error: None (graceful degradation)
```text

**Detection:**

- Automatic: Cache connection fails
- Monitoring: Cache server not responding

**Recovery:**

- **Automatic:** Cache marked unavailable, queries bypass cache
- **Automatic:** Cache reconnection attempted
- **RTO:** < 5 seconds (cache warming starts)
- **Data Impact:** None (queries still work, slower)

**Graceful Degradation:**

```text
Before cache failure:
  Query: 50ms (cache hit)

During cache failure:
  Query: 200-500ms (database direct)

After cache recovery:
  Query: 50ms again (cache warming in progress)
```text

#### 2.4.2 Cache Corruption

**When:** Stale/invalid data in cache

**Client Impact:**

```text
Query: Cache returns invalid data
Result: Inconsistent response to client
Monitoring: Data inconsistency detected
```text

**Detection:**

- Monitoring: Hash validation fails
- Automatic: Data type mismatch

**Recovery:**

- **Automatic:** Invalidate entry, fetch from database
- **Automatic:** Rebuild cache from fresh data
- **RTO:** < 1 second
- **Data Impact:** Brief stale read possible

#### 2.4.3 Cache Too Full

**When:** Memory limit reached

**Client Impact:**

```text
Write: Cannot add new entries to cache
Behavior: Entries evicted (LRU policy)
Result: More cache misses, slower queries
Error: None (automatic eviction)
```text

**Detection:**

- Monitoring: Cache memory > 90%
- Automatic: Eviction triggered

**Recovery:**

- **Automatic:** LRU entries evicted
- **Manual:** Increase cache memory
- **RTO:** Immediate (queries work, but slower)

---

### 2.5 Authentication Provider Failures

#### 2.5.1 Auth Provider Unreachable

**When:** OIDC provider down, Auth0 down, corporate SSO down

**Client Impact:**

```text
Request: No valid token
Recovery: Request uses cached/local auth
Behavior: Depends on auth provider type
```text

**Detection:**

- Automatic: Token validation request fails
- Monitoring: Auth provider not responding

**Recovery Strategy (depends on provider):**

**Scenario A: JWT Token (self-contained)**

```text
Token validation: Cached public key used
Auth provider unreachable: Token still validated locally
Impact: NONE (JWT self-contained)
RTO: 0 (no recovery needed)
```text

**Scenario B: OAuth2 Token (external validation)**

```text
Token validation: HTTP request to provider fails
Behavior: Option 1: Cache last 5min, allow
         Option 2: Deny all access
Impact: Depends on policy (typically DENY)
RTO: Until provider recovers (5-30 min)
```text

**Recommended:** Use JWT tokens for resilience.

#### 2.5.2 Token Expiration During Execution

**When:** Long-running query, token expires mid-execution

**Client Impact:**

```text
Auth check: Passed (token valid at start)
Execution: Query runs normally
Result: Query completes successfully
Token expiry: Ignored (already authenticated)
```text

**Detection:** Not detected (query already authorized)

**Recovery:** None needed (query completes)

---

### 2.6 Federation Failures

#### 2.6.1 Subgraph Unavailable

**When:** Federated subgraph is down or unreachable

**Client Impact:**

```text
Query: Needs federated entity
Attempt: HTTP request to subgraph fails
Response: HTTP 504 Gateway Timeout (after 5s wait)
Error: E_FED_SUBGRAPH_UNAVAILABLE_502
Retryable: YES
```text

**Detection:**

- Automatic: HTTP connection fails
- Monitoring: Subgraph health check fails

**Recovery:**

- **Automatic:** Retry with exponential backoff (1s, 2s, 4s)
- **Automatic:** Fallback to stale cache (if available)
- **Max retries:** 3 attempts × 5s = 15s total wait
- **RTO:** < 30 seconds
- **Fallback:** Return stale federated data if cached

**During Subgraph Outage:**

```text
Request 1 (T0): Subgraph unavailable
  → Retry 1 (T1s): Still unavailable
  → Retry 2 (T3s): Still unavailable
  → Return error after 15s total wait

Request 2 (T0): Parallel requests
  → Same retry loop
  → Shared cache prevents redundant retries
```text

#### 2.6.2 Entity Resolution Timeout

**When:** Subgraph responds slowly

**Client Impact:**

```text
Query: Needs federated entity
Wait: 5s timeout
Response: HTTP 504 Gateway Timeout
Error: E_FED_SUBGRAPH_TIMEOUT_503
Retryable: YES
```text

**Detection:**

- Automatic: HTTP request > timeout threshold
- Monitoring: Subgraph response time > SLO

**Recovery:**

- **Automatic:** Retry with backoff
- **Client:** Retry entire query
- **Manual:** Optimize subgraph query, scale out
- **RTO:** < 30 seconds (with retries)

#### 2.6.3 Entity Type Mismatch

**When:** Subgraph returns unexpected entity type

**Client Impact:**

```text
Query: Expects User { name, email }
Subgraph: Returns { name, org_id } (wrong shape)
Error: Type validation fails
Response: HTTP 500 Internal Server Error
Error: E_FED_TYPE_MISMATCH_504
Retryable: NO
```text

**Detection:**

- Automatic: Schema validation fails
- Monitoring: Type mismatches detected

**Recovery:**

- **Manual:** Fix subgraph schema
- **RTO:** Depends on schema update process (5-30 min)
- **Interim:** Disable federation for this entity type

---

### 2.7 Subscription Failures

#### 2.7.1 WebSocket Connection Lost

**When:** Network interruption, client disconnect, server restart

**Client Impact:**

```text
Subscription: Active WebSocket
Event: Network failure
Connection: Closed
Response: Close frame sent to client (code 1006)
Error: E_SUB_CONNECTION_CLOSED_604
Retryable: YES (reconnect)
```text

**Detection:**

- Automatic: Socket close detected
- Automatic: Keep-alive timeout (60s)

**Recovery:**

- **Client:** Reconnect and resubscribe
- **RTO:** < 5 seconds (reconnect + resubscribe)
- **Events during outage:** Buffered in `tb_entity_change_log`
- **Replay:** Client can query from last sequence_number

**Recovery Sequence:**

```text
T0: Connection lost
T1-5s: Client detects and reconnects
T5s: New subscription established
T5+: Resume receiving events from buffer
```text

#### 2.7.2 Event Buffer Overflow

**When:** Too many events, subscriber is slow

**Client Impact:**

```text
Events: Accumulating faster than delivery
Buffer: Fills up (default: 1000 events)
Response: Error sent to subscriber
Error: E_SUB_BUFFER_OVERFLOW_603
Retryable: YES (reconnect and replay from sequence)
```text

**Detection:**

- Monitoring: Buffer usage > 90%
- Automatic: Overflow detected

**Recovery:**

- **Client:** Reconnect and replay from last sequence number
- **Automatic:** Filter events (add WHERE clause) to reduce volume
- **Manual:** Increase buffer size
- **RTO:** < 10 seconds (reconnect)

#### 2.7.3 Event Delivery Failure (Webhooks)

**When:** Webhook endpoint returns error or is slow

**Client Impact:**

```text
Event: Needs delivery to webhook
Request: HTTP POST to endpoint
Response: 500 error or timeout
Behavior: Automatic retry with backoff
Attempts: Up to 5 retries over 10 minutes
```text

**Detection:**

- Automatic: HTTP response code >= 400
- Monitoring: Webhook delivery failures

**Recovery:**

- **Automatic:** Retry with exponential backoff
  - Attempt 1: Immediately
  - Attempt 2: After 1s
  - Attempt 3: After 5s
  - Attempt 4: After 30s
  - Attempt 5: After 5 minutes
- **Final failure:** Event logged, Webhook marked unhealthy
- **RTO:** 10 minutes until all retries exhausted

---

### 2.8 Authorization/Security Failures

#### 2.8.1 Auth Token Invalid

**When:** Malformed, expired, or tampered token

**Client Impact:**

```text
Request: Includes invalid token
Auth: Validation fails
Response: HTTP 401 Unauthorized
Error: E_AUTH_INVALID_TOKEN_201
Retryable: NO (user must re-authenticate)
```text

**Detection:**

- Automatic: Token signature invalid
- Automatic: Token expiration date passed

**Recovery:**

- **Client:** Re-authenticate to get new token
- **RTO:** Depends on auth flow (typically < 1 minute)

#### 2.8.2 Auth Context Stale

**When:** User role/permissions changed after token issued

**Client Impact:**

```text
Request: Token valid but permissions changed
Authorization: Evaluated against stale role
Result: May allow/deny incorrectly
Probability: Low (permissions don't change frequently)
```text

**Detection:**

- Monitoring: Authorization denials after known permission grant
- Manual: Security audit

**Recovery:**

- **Automatic:** Re-fetch auth context on periodic basis (TTL)
- **Manual:** Revoke and re-issue tokens
- **RTO:** 0-1 hour (depends on token refresh interval)

#### 2.8.3 RLS Policy Violation

**When:** Row-level security policy prevents access

**Client Impact:**

```text
Query: Attempts cross-tenant access
RLS: Policy prevents unauthorized access
Response: Query returns empty or error
Error: E_AUTH_ROW_LEVEL_SECURITY_DENIED_204 (if explicit)
Retryable: NO (user doesn't have permission)
```text

**Detection:**

- Automatic: RLS policy evaluation
- Monitoring: RLS denials by policy

**Recovery:**

- **No automatic recovery** (intentional security boundary)
- **Manual:** DBA grants access or user requests permission
- **RTO:** Depends on access request process

---

## 3. Cascading Failures

### 3.1 Database Down → Everything Down

```text
T0: Primary database becomes unavailable
T1: All queries start failing
T2: Cache still works (if recently populated)
T3: Subscriptions: Events not captured
T4: Federation: Dependent services affected
T5-30min: RTO depends on failover setup
```text

### 3.2 Cache Down → Database Load Spikes

```text
T0: Cache server fails
T1: All queries bypass cache
T2: Database CPU/memory spike
T3: Database becomes slow
T4: Queries timeout
T5+: Potential database failure if resources exhausted
```text

### 3.3 Authentication Provider Down → All Writes Blocked

```text
T0: Auth provider unreachable
T1: New token validation fails (OAuth2 tokens)
T2: New user requests return 401
T3: Existing queries continue (token already valid)
T4: Eventually: All tokens expire, all users blocked
```text

### 3.4 Subscription Event Buffer Full → All Events Dropped

```text
T0: Event throughput increases
T1: Buffer fills to capacity
T2: New events cannot be stored
T3: Subscribers miss events
T4: Clients must replay from last sequence_number
```text

---

## 4. Failure Recovery Procedures

### 4.1 Single Instance Crash

**Automatic (no human intervention):**

```text

1. Instance crashes
2. Kubernetes detects unhealthy pod (10-30s)
3. Spins up new pod
4. New pod joins load balancer
5. Existing connections fail (clients retry)
6. New requests route to new pod

Total time: 30-60 seconds
Human intervention: None
Data loss: None
```text

**Manual verification:**

```bash
# Check pod logs for crash reason
kubectl logs pod-name

# If OOM: Increase memory limits
# If out of disk: Clean up logs/cache
# If panic: File bug report
```text

### 4.2 Database Connection Lost

**Automatic:**

```text

1. Query fails with connection error
2. Runtime closes stale connection
3. Reopens connection (exponential backoff)
4. Retries query on new connection
5. Client sees temporary latency spike

Total time: 2-10 seconds
Human intervention: None (check database logs)
Data loss: None
```text

### 4.3 Database Unavailable (Recovery)

**Steps:**

```text

1. Monitor: Detect database not responding (< 5 seconds)
2. Alert: Page on-call DBA
3. Investigate: Check database logs, network connectivity
4. Recovery:
   a. If crashed: Restart database server
   b. If network: Restore network connectivity
   c. If failed: Promote replica (if HA setup)
5. Verify: Run queries, check replication lag
6. Monitor: Watch for issues, gradual traffic increase

Total time: 5-30 minutes (depends on issue)
Human intervention: Required
Data loss: None (if durability persisted)
```text

### 4.4 Complete Data Center Failure

**Steps (assume multi-datacenter setup):**

```text

1. Monitor: Detect all instances/database in region down
2. Alert: Page incident commander
3. Failover:
   a. Update DNS to point to secondary region
   b. Promote secondary database (if replication lag acceptable)
   c. OR: Route to standby region
4. Verify: Monitor traffic, error rates
5. Recovery:
   a. Restore primary region
   b. Rebuild replication to primary
   c. Failback (if needed)
6. Monitor: Watch for issues

Total time: 2-5 minutes (automatic DNS) + manual investigation
Human intervention: Required
Data loss: Depends on replication lag (typically < 1 second)
```text

---

## 5. Resilience Design Patterns

### 5.1 Circuit Breaker Pattern

Used for subgraph/external service calls:

```text
State 1: CLOSED (normal)
  - Requests pass through
  - If error rate > threshold: transition to OPEN

State 2: OPEN (circuit broken)
  - Requests fail immediately (no wait)
  - Error: E_FED_SUBGRAPH_UNAVAILABLE
  - Duration: 30 seconds

State 3: HALF_OPEN (testing recovery)
  - Allow 1 request through
  - If succeeds: transition to CLOSED
  - If fails: transition back to OPEN

Example:
  Error rate: 50% failures
  Threshold: 10% failures
  → Transition to OPEN
  → Requests fail immediately
  → 30s later: Try 1 request (HALF_OPEN)
  → Succeeds: Transition to CLOSED, resume normal
```text

### 5.2 Bulkhead Pattern

Isolate resources to prevent cascading failure:

```text
Pool 1: Queries (max 100 concurrent)
Pool 2: Mutations (max 50 concurrent)
Pool 3: Subscriptions (max 1000 concurrent)

If Pool 1 exhausted:
  - Queries queue or fail
  - Mutations: Still work (own pool)
  - Subscriptions: Still work (own pool)

Prevents: One type of query from starving others
```text

### 5.3 Retry with Exponential Backoff

For transient failures:

```text
Attempt 1: Immediate
Attempt 2: Wait 1s + random(0-100ms)
Attempt 3: Wait 2s + random(0-100ms)
Attempt 4: Wait 4s + random(0-100ms)
Attempt 5: Wait 8s + random(0-100ms)
Max attempts: 5 (total wait: ~15 seconds)

Benefits:
  - Avoids thundering herd
  - Gives system time to recover
  - Client waits reasonably (15s)
  - Success rate increases 90% → 99%+
```text

### 5.4 Graceful Degradation

Accept reduced functionality under load:

```text
Normal load:
  - All features: queries, mutations, subscriptions
  - Latency: < 500ms p99

High load (>80% capacity):
  - Disable: Subscriptions (keep for emergency)
  - Keep: Queries, mutations, auth
  - Latency: < 2s p99

Critical load (>95% capacity):
  - Disable: Mutations, subscriptions
  - Keep: Read queries only
  - Latency: < 5s p99

Extreme load (>99% capacity):
  - Disable: Most features
  - Keep: Critical read queries only
  - Return: 503 Service Unavailable for others

Benefit: Service remains available, not crashed
```text

---

## 6. Failure Testing (Chaos Engineering)

### 6.1 Injected Failures

Use to test resilience:

```text
# Kill 1 instance (of 3)
chaos kill -pod 1

Expected:
  - Requests to pod 1: Fail (< 1s)
  - Requests reroute to pods 2-3
  - No cascading failure
  - System still healthy

# Increase latency on database
chaos network-delay database +500ms

Expected:
  - Query latency increases
  - Queries eventually timeout (after 30s)
  - Retries backoff
  - System recovers when latency drops

# Fill cache
chaos fill-cache 95%

Expected:
  - Cache evicts LRU entries
  - More cache misses
  - Database load increases
  - Queries slower but not failing

# Drop 10% of packets
chaos drop-packets 10%

Expected:
  - Some requests fail
  - Retry logic engages
  - Eventually succeed (after retries)
  - End-to-end latency increases
```text

### 6.2 Failure Acceptance Criteria

After injected failure:

```text
✅ PASS if:
  - No data corruption
  - No data loss (for writes)
  - Requests eventually succeed (or fail gracefully)
  - System recovers when failure removed
  - Alerts triggered appropriately
  - No cascading failures to other systems

❌ FAIL if:
  - Data corrupted
  - Data lost
  - System deadlocked
  - Cascading failure
  - No alerts triggered
  - Recovery time > RTO target
```text

---

## 7. SLO and Error Budgets

### 7.1 Availability SLO

```text
Target: 99.9% availability (three nines)
Definition: Successfully responding to queries
Time unit: Calendar month
Calculation: Uptime / Total time

99.9% of 30 days = 43 minutes downtime/month

Within budget:
  - 30 minutes unplanned outage ✅
  - 13 minutes planned maintenance ✅

Over budget:
  - 50 minutes unplanned outage ❌
  - Budget exhausted, any additional downtime violates SLO
```text

### 7.2 Error Budget Usage

```text
Month: January (31 days, 44,640 minutes)
Budget: 43.2 minutes (99.9% SLO)

Week 1: 5 min downtime → 38.2 min remaining
Week 2: 10 min downtime → 28.2 min remaining
Week 3: 8 min downtime → 20.2 min remaining
Week 4: 12 min downtime → 8.2 min remaining

If Week 4 has additional 2 min outage:
  - Error budget exhausted (10.2 > 8.2)
  - SLO violated for January
  - Requires incident review, not just bug fix
```text

---

## Summary

**FraiseQL failure characteristics:**

✅ **Fast detection:** Failures detected < 5 seconds
✅ **Automatic recovery:** Most failures handled automatically
✅ **Graceful degradation:** Reduced functionality, not complete outage
✅ **Data safety:** No data loss for committed writes
✅ **Clear error codes:** Clients know what failed and why
✅ **Observable:** Traceability via trace IDs and structured logs

**Key RTO targets:**

- Single instance crash: 30-60 seconds
- Database connection: < 10 seconds
- Cache failure: < 5 seconds
- Database unavailable: 5-30 minutes (operational)
- Complete region failure: 2-5 minutes (DNS + failover)

**Golden rule:** FraiseQL fails safely, recovers gracefully, and provides sufficient observability to debug issues.

---

*End of Failure Modes and Recovery*
