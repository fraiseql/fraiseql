# GraphQL Query DoS Runbook

**Last Updated**: 2025-12-29
**Severity**: CRITICAL
**MTTR Target**: 10 minutes

---

## 📋 Overview

This runbook guides you through detecting, mitigating, and preventing GraphQL query-based Denial of Service (DoS) attacks. GraphQL's flexible query language can be exploited to create expensive queries that consume excessive resources.

---

## 🚨 Symptoms

### Primary Indicators
- Sudden spike in query execution time
- High CPU usage (> 90%)
- Database connection pool exhaustion
- Query timeout errors
- Application unresponsiveness
- Memory usage climbing rapidly

### Prometheus Metrics to Monitor

```promql
# Query execution time spike
avg(rate(fraiseql_graphql_query_duration_seconds_sum[5m])
    / rate(fraiseql_graphql_query_duration_seconds_count[5m])) > 10

# High query timeout rate
rate(fraiseql_errors_total{error_code="QUERY_TIMEOUT"}[5m]) > 1

# Database CPU usage
rate(pg_stat_database_blks_read[5m]) > 1000

# Expensive query patterns (deep nesting, large limits)
fraiseql_graphql_query_complexity_total > 1000
```

### Structured Logs Examples

```json
{
  "timestamp": "2025-12-29T13:45:10.123Z",
  "level": "WARNING",
  "event": "graphql.expensive_query",
  "message": "Query complexity exceeds threshold",
  "context": {
    "user_id": "user_attack",
    "ip_address": "192.0.2.50",
    "query_complexity": 1500,
    "max_complexity": 1000,
    "query_depth": 8,
    "max_depth": 5,
    "query_hash": "abc123def456"
  },
  "trace_id": "trace_dos123"
}
```

```json
{
  "timestamp": "2025-12-29T13:45:15.456Z",
  "level": "ERROR",
  "event": "database.query_timeout",
  "message": "Query exceeded timeout limit",
  "context": {
    "user_id": "user_attack",
    "query_duration_ms": 30000,
    "timeout_ms": 30000,
    "query_type": "SELECT",
    "table_name": "users",
    "rows_examined": 1000000
  },
  "trace_id": "trace_dos456"
}
```

```json
{
  "timestamp": "2025-12-29T13:45:20.789Z",
  "level": "CRITICAL",
  "event": "security.dos_detected",
  "message": "Potential DoS attack detected",
  "context": {
    "user_id": "user_attack",
    "ip_address": "192.0.2.50",
    "expensive_queries_count": 10,
    "time_window_seconds": 60,
    "action": "automatic_block"
  },
  "trace_id": "trace_dos789"
}
```

---

## 🔍 Diagnostic Steps

### Step 1: Identify Attack Pattern

**Via Prometheus - Query Complexity**:
```promql
# Top users by query complexity
topk(10,
  sum by (user_id) (fraiseql_graphql_query_complexity_total)
)

# Queries with deep nesting
count(fraiseql_graphql_query_depth > 5)

# Large result set requests
count(fraiseql_graphql_query_limit > 1000)
```

**Via Structured Logs**:
```bash
# Find expensive queries in last 10 minutes
jq -r 'select(.event == "graphql.expensive_query") |
  "\(.timestamp) \(.context.user_id) complexity=\(.context.query_complexity) depth=\(.context.query_depth)"' \
  /var/log/fraiseql/app.log | tail -50

# Find query timeouts
jq -r 'select(.event == "database.query_timeout") |
  "\(.timestamp) \(.context.user_id) \(.context.query_duration_ms)ms"' \
  /var/log/fraiseql/app.log | tail -50

# Identify attack source
jq -r 'select(.event == "graphql.expensive_query") |
  .context.user_id + " " + .context.ip_address' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn | head -10
```

### Step 2: Analyze Query Patterns

**Common Attack Patterns**:

**1. Deep Nesting (Circular Queries)**:
```graphql
# Attack query - deeply nested relationships
query {
  users {
    posts {
      comments {
        author {
          posts {
            comments {
              author {
                posts {
                  # ... continues deep
                }
              }
            }
          }
        }
      }
    }
  }
}
```

**2. Large Batch Requests**:
```graphql
# Attack query - requesting huge result sets
query {
  users(limit: 10000) {
    id
    name
    posts(limit: 10000) {
      id
      comments(limit: 10000) {
        id
      }
    }
  }
}
# Potentially returns 10,000 * 10,000 * 10,000 = 1 trillion items!
```

**3. Expensive Field Combinations**:
```graphql
# Attack query - combining expensive computed fields
query {
  users {
    id
    expensiveCalculation1  # Takes 1s per user
    expensiveCalculation2  # Takes 1s per user
    expensiveCalculation3  # Takes 1s per user
  }
}
```

**Extract Query from Logs**:
```bash
# Get actual query that caused issue
jq -r 'select(.event == "graphql.expensive_query" and .context.user_id == "user_attack") |
  .context.query // .context.query_hash' \
  /var/log/fraiseql/app.log | head -1
```

### Step 3: Check System Resources

**Database Load**:
```sql
-- Check active queries
SELECT
  pid,
  usename,
  query_start,
  state,
  left(query, 100) AS query_preview
FROM pg_stat_activity
WHERE state = 'active'
ORDER BY query_start;

-- Check database CPU and I/O
SELECT
  datname,
  blks_read,
  blks_hit,
  tup_returned,
  tup_fetched
FROM pg_stat_database
WHERE datname = current_database();
```

**Application Resources**:
```promql
# CPU usage
rate(process_cpu_seconds_total[5m])

# Memory usage
process_resident_memory_bytes

# Active connections
fraiseql_db_connections_active
```

### Step 4: Determine Attack vs. Misconfiguration

**Attack Indicators**:
- Same user/IP sending many expensive queries
- Queries with unusual complexity
- Sudden pattern change (normal → expensive)
- Distributed source IPs (coordinated attack)

**Misconfiguration Indicators**:
- Frontend sending overly broad queries
- Integration fetching unnecessary data
- Missing pagination in production code
- Recent deployment with bad queries

```bash
# Check if recent deployment
git log --since="1 hour ago" --oneline

# Check if queries match known frontend patterns
jq -r 'select(.event == "graphql.expensive_query") | .context.query_hash' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn

# High count of same query_hash = likely misconfiguration
# Many different query_hashes = likely attack
```

---

## 🔧 Resolution Steps

### Immediate Actions (< 5 minutes)

#### 1. Enable Emergency Query Complexity Limits

```python
# Apply emergency limits (if not already configured)
from fraiseql import FraiseQL
from fraiseql.security import QueryComplexityAnalyzer

app = FraiseQL()

# Emergency strict limits
complexity_analyzer = QueryComplexityAnalyzer(
    max_complexity=500,     # Reduce from 1000
    max_depth=3,            # Reduce from 5
    max_list_size=100,      # Reduce from 1000
)

app.add_plugin(complexity_analyzer)
```

**Environment Variable Override**:
```bash
# Apply immediately without code changes
export FRAISEQL_MAX_QUERY_COMPLEXITY=500
export FRAISEQL_MAX_QUERY_DEPTH=3
export FRAISEQL_MAX_LIST_SIZE=100

# Restart application
systemctl restart fraiseql
```

#### 2. Block Attacking User/IP

```python
from fraiseql.security import BlockList

# Block user
await BlockList.add_user(
    user_id="user_attack",
    duration_seconds=3600,
    reason="GraphQL DoS attack"
)

# Block IP
await BlockList.add_ip(
    ip_address="192.0.2.50",
    duration_seconds=3600,
    reason="GraphQL DoS attack"
)
```

**Via Admin API**:
```bash
curl -X POST http://localhost:8000/admin/blocks \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user_attack",
    "ip_address": "192.0.2.50",
    "duration_seconds": 3600,
    "reason": "GraphQL DoS attack"
  }'
```

#### 3. Kill Long-Running Queries

```sql
-- Find long-running GraphQL queries
SELECT
  pid,
  query_start,
  state,
  query
FROM pg_stat_activity
WHERE state = 'active'
  AND query_start < now() - interval '10 seconds'
  AND query LIKE '%graphql%';

-- Kill specific queries
SELECT pg_cancel_backend(<pid>);

-- Force terminate if cancel doesn't work
SELECT pg_terminate_backend(<pid>);
```

### Short-Term Fixes (5-15 minutes)

#### 1. Implement Query Depth Limiting

```python
from fraiseql import FraiseQL
from fraiseql.validation import DepthLimitRule

app = FraiseQL()

# Enforce maximum query depth
depth_limit = DepthLimitRule(
    max_depth=5,  # Prevents deeply nested queries
    ignore_introspection=True,  # Allow schema introspection
)

app.add_validation_rule(depth_limit)
```

**Configuration**:
```python
# config/security.py
GRAPHQL_SECURITY = {
    "max_depth": 5,
    "max_complexity": 1000,
    "max_list_size": 1000,
    "timeout_seconds": 30,
}
```

#### 2. Implement Query Complexity Analysis

```python
from fraiseql.validation import ComplexityLimitRule

# Calculate complexity based on field costs
complexity_limit = ComplexityLimitRule(
    max_complexity=1000,

    # Assign cost to expensive fields
    field_costs={
        "User.posts": 10,           # Expensive relationship
        "Post.comments": 10,         # Expensive relationship
        "User.statistics": 50,       # Expensive computation
    },

    # List multipliers
    list_multiplier=10,  # Multiply cost by list size
)

app.add_validation_rule(complexity_limit)
```

**How Complexity is Calculated**:
```python
# Example query
"""
query {
  users(limit: 100) {     # 100 items
    posts(limit: 50) {     # 100 * 50 = 5,000 items
      comments(limit: 10)  # 5,000 * 10 = 50,000 items
    }
  }
}
"""

# Complexity calculation:
# users: 1 * 100 = 100
# posts: 10 (field cost) * 100 * 50 = 50,000
# comments: 10 (field cost) * 5,000 * 10 = 500,000
# Total: ~550,000 (BLOCKED if max_complexity=1000)
```

#### 3. Implement Query Timeout Enforcement

```python
from fraiseql import FraiseQL

app = FraiseQL()

# Set query timeout (already implemented in FraiseQL)
@app.context_value
async def get_context(request):
    return {
        "db": app.db.get_connection(
            query_timeout=10,  # 10 seconds for GraphQL queries
            user_id=request.user.id,
        )
    }
```

**PostgreSQL-level Timeout** (already configured):
```python
# In db.py:119-133
async def get_connection(self, query_timeout: int = 30):
    conn = await self.pool.getconn()
    timeout_ms = query_timeout * 1000
    await conn.execute(f"SET statement_timeout = {timeout_ms}")
    return conn
```

#### 4. Implement Pagination Enforcement

```python
from fraiseql import FraiseQL

app = FraiseQL()

@app.query.field("users")
async def resolve_users(
    info,
    limit: int = 100,
    offset: int = 0
):
    # Enforce maximum limit
    if limit > 1000:
        raise ValueError("Limit cannot exceed 1000")

    # Enforce pagination for large offsets
    if offset > 10000:
        raise ValueError("Use cursor-based pagination for large offsets")

    query = "SELECT * FROM users LIMIT $1 OFFSET $2"
    return await info.context["db"].fetch(query, limit, offset)
```

### Long-Term Solutions (1+ days)

#### 1. Implement Query Allowlisting

```python
# Allow only pre-approved queries (strictest security)
from fraiseql.security import QueryAllowList

allowed_queries = QueryAllowList()

# Register allowed queries
allowed_queries.add(
    name="GetUsers",
    query="""
        query GetUsers($limit: Int!) {
          users(limit: $limit) {
            id
            name
            email
          }
        }
    """,
    max_limit=100,
)

# Only allowed queries can execute
app.add_plugin(allowed_queries)
```

#### 2. Implement Persisted Queries

```python
from fraiseql.security import PersistedQueries

# Clients send query hash instead of full query
persisted_queries = PersistedQueries(
    storage="redis://localhost:6379",
    allow_automatic_persisting=False,  # Manual approval only
)

app.add_plugin(persisted_queries)

# Client sends:
# { "extensions": { "persistedQuery": { "sha256Hash": "abc123..." } } }
# Instead of full query text
```

#### 3. Implement Query Cost Budget

```python
from fraiseql.security import CostLimiter

# Users have limited query budget
cost_limiter = CostLimiter(
    storage="redis://localhost:6379",

    # Tier-based budgets
    budgets={
        "free": 1000,      # 1000 complexity points per hour
        "pro": 10000,      # 10000 complexity points per hour
        "enterprise": 100000,
    },

    window_seconds=3600,  # 1-hour window
)

app.add_plugin(cost_limiter)
```

#### 4. Implement DataLoader (N+1 Prevention)

```python
from fraiseql.dataloaders import DataLoader

class CommentLoader(DataLoader):
    async def batch_load_fn(self, post_ids):
        # Load all comments in single query
        query = """
            SELECT * FROM comments
            WHERE post_id = ANY($1)
        """
        rows = await db.fetch(query, post_ids)

        # Group by post_id
        comments_by_post = {}
        for row in rows:
            comments_by_post.setdefault(row['post_id'], []).append(row)

        return [comments_by_post.get(pid, []) for pid in post_ids]

# Use in resolver
@app.type.field("Post", "comments")
async def resolve_comments(post, info):
    loader = info.context["comment_loader"]
    return await loader.load(post.id)

# Converts N+1 queries into 1 query!
```

---

## 📊 Monitoring & Alerts

### Prometheus Alert Rules

```yaml
# alerts/graphql_dos.yml
groups:
  - name: graphql_dos
    interval: 30s
    rules:
      - alert: GraphQLQueryDurationSpike
        expr: |
          avg(rate(fraiseql_graphql_query_duration_seconds_sum[5m])
              / rate(fraiseql_graphql_query_duration_seconds_count[5m])) > 10
        for: 3m
        labels:
          severity: critical
        annotations:
          summary: "GraphQL query duration spike (avg > 10s)"
          description: "Average query time: {{ $value }}s"

      - alert: GraphQLQueryTimeouts
        expr: |
          rate(fraiseql_errors_total{error_code="QUERY_TIMEOUT"}[5m]) > 1
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "High GraphQL query timeout rate"
          description: "{{ $value }} timeouts/sec"

      - alert: GraphQLComplexityViolations
        expr: |
          rate(fraiseql_graphql_query_complexity_exceeded_total[5m]) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Queries exceeding complexity limits"
          description: "{{ $value }} complex queries/sec"

      - alert: GraphQLDoSDetected
        expr: |
          sum by (user_id) (
            rate(fraiseql_errors_total{error_code="QUERY_TIMEOUT"}[5m])
          ) > 5
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Potential GraphQL DoS from user {{ $labels.user_id }}"
          description: "{{ $value }} timeouts/sec from single user"
```

### Grafana Dashboard Panels

**1. Query Duration Histogram**:
```promql
histogram_quantile(0.95,
  rate(fraiseql_graphql_query_duration_seconds_bucket[5m])
)
```

**2. Query Complexity Distribution**:
```promql
histogram_quantile(0.95,
  rate(fraiseql_graphql_query_complexity_bucket[5m])
)
```

**3. Top Expensive Queries (by user)**:
```promql
topk(10,
  sum by (user_id) (
    rate(fraiseql_graphql_query_duration_seconds_sum[5m])
  )
)
```

**4. Timeout Rate**:
```promql
rate(fraiseql_errors_total{error_code="QUERY_TIMEOUT"}[5m])
```

---

## 🔍 Verification

After applying mitigations, verify the attack is stopped:

### 1. Check Query Duration

```promql
# Should return to normal (< 1s)
avg(rate(fraiseql_graphql_query_duration_seconds_sum[5m])
    / rate(fraiseql_graphql_query_duration_seconds_count[5m]))
```

### 2. Check Timeout Rate

```promql
# Should drop to near zero
rate(fraiseql_errors_total{error_code="QUERY_TIMEOUT"}[5m])
```

### 3. Verify Blocked User

```bash
# Attempt query as blocked user
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer $BLOCKED_USER_TOKEN" \
  -d '{"query": "{ users { id } }"}'

# Should return 403 Forbidden
```

### 4. Check Logs

```bash
# Verify no recent expensive queries
jq -r 'select(.event == "graphql.expensive_query")' \
  /var/log/fraiseql/app.log | tail -10

# Should see no recent events
```

---

## 📝 Post-Incident Review

After resolving the incident:

1. **Analyze Attack**:
   - Query patterns used
   - Attack source (IPs, users)
   - Entry point (API, frontend)
   - Duration and impact

2. **Implement Permanent Protections**:
   - Deploy query complexity limits
   - Implement query depth limiting
   - Add query allowlisting (if feasible)
   - Enable persisted queries

3. **Update Monitoring**:
   - Add alerts for complexity violations
   - Monitor query patterns
   - Track query costs by user

4. **Security Improvements**:
   - Review authentication
   - Implement query budgets
   - Add WAF rules if applicable

---

## 📚 Related Resources

- [GraphQL Security Best Practices](../security.md#graphql-security)
- [Query Complexity Analysis](../../performance/performance-guide.md)
- [Database Performance](./database-performance-degradation.md)
- [Rate Limiting](./rate-limiting-triggered.md)

---

## 🆘 Escalation

If attack persists after following this runbook:

1. **Gather Evidence**:
   - Sample attack queries
   - User/IP information
   - Metrics screenshots
   - Database query logs

2. **Escalate To**:
   - Security Team (for attack investigation)
   - Infrastructure Team (for DDoS mitigation)
   - Engineering Team (for code-level fixes)

3. **Emergency Contact**:
   - Security On-call: [Contact info]
   - Infrastructure On-call: [Contact info]
   - Engineering Manager: [Contact info]

---

**Version**: 1.0
**Last Tested**: 2025-12-29
**Next Review**: 2026-03-29
