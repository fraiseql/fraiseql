<!-- Skip to main content -->
---
title: Database Schema Migration Guide
description: Step-by-step guide for migrating existing database schemas to FraiseQL v2.0.0-alpha.1.
keywords: ["debugging", "implementation", "best-practices", "deployment", "schema", "tutorial"]
tags: ["documentation", "reference"]
---

# Database Schema Migration Guide

**Status:** ✅ Production Ready
**Audience:** DevOps, Database Administrators, Architects
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Step-by-step guide for migrating existing database schemas to FraiseQL v2.0.0-alpha.1.

---

## Overview

This guide covers migrating from:

- **Legacy GraphQL servers** (Apollo Server, Hasura, PostGraphile, etc.)
- **Existing SQL databases** (PostgreSQL, MySQL, SQLite, SQL Server)
- **Monolithic schemas** to **federated architectures**

**Key principle:** Schema migration is a **data structure change**, not a data migration. Your existing data stays in place; you're restructuring how FraiseQL accesses it.

---

## Pre-Migration Planning

### 1. Assess Current Architecture

**Answer these questions:**

- [ ] Current database: PostgreSQL / MySQL / SQLite / SQL Server?
- [ ] Total tables: < 50 / 50-200 / 200-1000 / > 1000?
- [ ] Database size: < 1GB / 1-10GB / 10-100GB / > 100GB?
- [ ] Peak QPS (queries per second): < 100 / 100-1000 / > 1000?
- [ ] Uptime requirement: Best-effort / 99% / 99.9% / 99.99%?
- [ ] Multi-region deployment: Yes / No?
- [ ] Federation needed: Yes / No?

### 2. Create Migration Plan

**Template:**

```markdown
<!-- Code example in MARKDOWN -->
## Migration Plan: [Project Name]

### Timeline
- Phase 1 (Week 1): Preparation and schema analysis
- Phase 2 (Week 2): FraiseQL schema development
- Phase 3 (Week 3): Integration testing
- Phase 4 (Week 4): Staging deployment
- Phase 5 (Week 5): Production cutover

### Rollback Plan
- Keep old GraphQL server running during testing
- Traffic: 10% to FraiseQL, 90% to old server (Week 1)
- Then 50/50 (Week 2)
- Then 100% FraiseQL (Week 3)

### Team
- Schema Designer: [Name]
- DevOps Lead: [Name]
- QA Lead: [Name]
- Database Admin: [Name]
```text
<!-- Code example in TEXT -->

### 3. Audit Current Schema

**Generate schema export:**

```bash
<!-- Code example in BASH -->
# PostgreSQL
pg_dump --schema-only $DATABASE_URL > schema.sql

# MySQL
mysqldump --no-data $DATABASE > schema.sql

# SQLite
sqlite3 $DATABASE ".schema" > schema.sql

# SQL Server
sqlcmd -S $SERVER -d $DATABASE -i "schema.sql" -x
```text
<!-- Code example in TEXT -->

---

## Phase 1: Analyze Existing Schema

### Step 1.1: Document Tables & Views

**Create inventory:**

```bash
<!-- Code example in BASH -->
# PostgreSQL: List all tables
SELECT tablename FROM pg_tables WHERE schemaname='public';

# MySQL: List all tables
SHOW TABLES;

# SQLite: List all tables
.tables

# SQL Server: List all tables
SELECT name FROM sys.tables;
```text
<!-- Code example in TEXT -->

**Output format:**

```text
<!-- Code example in TEXT -->
TABLE_NAME | COLUMNS | ROWS | SIZE | INDEXES | PK | NOTES
users      | 12      | 2M   | 500MB | 3      | id | Active users table
posts      | 8       | 10M  | 2GB   | 4      | id | Need tv_* materialization
```text
<!-- Code example in TEXT -->

### Step 1.2: Identify Access Patterns

**Analyze queries:**

```sql
<!-- Code example in SQL -->
-- Find most frequent queries
SELECT query, calls FROM pg_stat_statements
ORDER BY calls DESC LIMIT 20;

-- Find slow queries
SELECT query, mean_time FROM pg_stat_statements
WHERE mean_time > 100
ORDER BY mean_time DESC LIMIT 20;
```text
<!-- Code example in TEXT -->

**Use this to decide:**

- Which fields should have indexes
- Which views need materialization (tv_*)
- Which queries need optimization

### Step 1.3: Map Relationships

**Create relationship diagram:**

```text
<!-- Code example in TEXT -->
Users (id, name, email)
  ├─ 1:M → Posts (id, user_id, content)
  │          ├─ 1:M → Comments (id, post_id, text)
  │          └─ M:M → Tags (join: post_tags)
  ├─ M:M → Groups (join: user_groups)
  └─ M:1 ← Organizations (org_id)

Organizations (id, name)
  ├─ 1:M → Users
  └─ 1:M → Teams
```text
<!-- Code example in TEXT -->

---

## Phase 2: Create FraiseQL Schema

### Step 2.1: Skeleton Schema

**Create minimal schema for all tables:**

```python
<!-- Code example in Python -->
# schema.py
from FraiseQL import type, key, field, where, context
from typing import Optional, List
from datetime import datetime
from decimal import Decimal

@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    email: str
    created_at: datetime
    updated_at: datetime
    is_active: bool

@type
class Post:
    id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    content: str
    created_at: datetime

@type
class Organization:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    created_at: datetime
```text
<!-- Code example in TEXT -->

### Step 2.2: Add Relationships

**Add 1:M and M:M relationships:**

```python
<!-- Code example in Python -->
@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    email: str
    created_at: datetime
    # NEW: Relationships
    posts: List[Post]  # 1:M relationship
    organization: Organization  # M:1 relationship
    groups: List[Group]  # M:M relationship

@type
class Post:
    id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    content: str
    created_at: datetime
    # NEW: Relationships
    user: User  # M:1 back-reference
    comments: List[Comment]  # 1:M relationship

@type
class Organization:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    created_at: datetime
    # NEW: Relationships
    users: List[User]  # 1:M reverse
```text
<!-- Code example in TEXT -->

### Step 2.3: Add Row-Level Security

**Add multi-tenancy filtering:**

```python
<!-- Code example in Python -->
@type
class Post:
    where: Where = FraiseQL.where(
        fk_organization=FraiseQL.context.org_id,  # Only user's org
    )

    id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    content: str
    user: User
    comments: List[Comment]

@type
class Comment:
    where: Where = FraiseQL.where(
        # Nested RLS: posts -> comments
        post_id__in=FraiseQL.subquery(
            "SELECT id FROM posts WHERE fk_organization = ?",
            [FraiseQL.context.org_id]
        )
    )

    id: UUID  # UUID v4 for GraphQL ID
    post_id: UUID  # UUID v4 for GraphQL ID
    content: str
```text
<!-- Code example in TEXT -->

### Step 2.4: Add Authorization

**Add field-level access control:**

```python
<!-- Code example in Python -->
@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    email: str = field(
        authorize={Roles.SELF, Roles.ADMIN, Roles.HR}
    )
    salary: Decimal = field(
        authorize={Roles.HR}
    )
    password_hash: str = field(
        authorize=set()  # Never readable
    )
```text
<!-- Code example in TEXT -->

### Step 2.5: Optimize with Views

**Add materialized views (tv_*) for performance:**

```python
<!-- Code example in Python -->
@type
class UserStats:
    """Materialized daily - fast lookups for aggregations."""
    id: UUID  # UUID v4 for GraphQL ID
    post_count: int
    comment_count: int
    avg_likes_per_post: Decimal
    updated_at: datetime

# Materialization SQL
CREATE TABLE tv_user_stats AS
SELECT
    u.id,
    COUNT(DISTINCT p.id) as post_count,
    COUNT(DISTINCT c.id) as comment_count,
    AVG(l.like_count) as avg_likes_per_post,
    NOW() as updated_at
FROM users u
LEFT JOIN posts p ON u.id = p.user_id
LEFT JOIN comments c ON p.id = c.post_id
LEFT JOIN (
    SELECT post_id, COUNT(*) as like_count
    FROM likes
    GROUP BY post_id
) l ON p.id = l.post_id
GROUP BY u.id;
```text
<!-- Code example in TEXT -->

---

## Phase 3: Integration Testing

### Step 3.1: Set Up Staging Environment

**Clone production database:**

```bash
<!-- Code example in BASH -->
# PostgreSQL
pg_dump $PROD_DATABASE | psql $STAGING_DATABASE

# MySQL
mysqldump $PROD_DATABASE | mysql $STAGING_DATABASE

# SQLite
cp $PROD_DATABASE $STAGING_DATABASE
```text
<!-- Code example in TEXT -->

### Step 3.2: Compile FraiseQL Schema

```bash
<!-- Code example in BASH -->
# Install FraiseQL CLI
cargo install FraiseQL-cli

# Compile schema
FraiseQL compile schema.py --config FraiseQL.toml

# Verify compilation
ls -la schema.compiled.json
```text
<!-- Code example in TEXT -->

### Step 3.3: Start FraiseQL Server

```bash
<!-- Code example in BASH -->
# Start with staging database
FRAISEQL_DATABASE_URL=postgresql://staging_db FraiseQL serve

# Test GraphQL endpoint
curl -X POST http://localhost:5000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'
```text
<!-- Code example in TEXT -->

### Step 3.4: Query Compatibility Testing

**Validate old queries work in FraiseQL:**

```graphql
<!-- Code example in GraphQL -->
# Old query (from current server)
query {
  users {
    id
    name
    email
    posts {
      id
      title
      created_at
    }
  }
}

# Should return same data in FraiseQL
# Verify: Response structure, field names, values, types
```text
<!-- Code example in TEXT -->

**Test harness:**

```python
<!-- Code example in Python -->
# test_migration.py
import requests
import json

OLD_SERVER = "http://old-server:3000/graphql"
NEW_SERVER = "http://localhost:5000/graphql"

queries = [
    '{ users { id name } }',
    '{ posts(first: 100) { id title user { name } } }',
    '{ organizations { id users { id posts { id } } } }',
]

for query in queries:
    old_result = requests.post(OLD_SERVER, json={"query": query}).json()
    new_result = requests.post(NEW_SERVER, json={"query": query}).json()

    assert old_result["data"] == new_result["data"], f"Query mismatch: {query}"

print("✅ All queries compatible!")
```text
<!-- Code example in TEXT -->

### Step 3.5: Performance Baseline

**Measure query performance before cutover:**

```bash
<!-- Code example in BASH -->
# Run load test on old server
wrk -t4 -c100 -d60s \
  -s load_test.lua \
  http://old-server:3000/graphql

# Record: latency (P50, P95, P99), throughput, errors

# Run same load test on FraiseQL
wrk -t4 -c100 -d60s \
  -s load_test.lua \
  http://localhost:5000/graphql

# Compare: Should be similar or faster
```text
<!-- Code example in TEXT -->

---

## Phase 4: Staged Cutover

### Step 4.1: Traffic Splitting (Week 1)

**Route 10% of traffic to FraiseQL:**

```nginx
<!-- Code example in NGINX -->
# nginx configuration
upstream old_server {
    server old-server:3000;
}

upstream new_server {
    server localhost:5000;
}

server {
    listen 443 ssl;

    location /graphql {
        # 90% to old server, 10% to new
        if ($random < 0.1) {
            proxy_pass http://new_server;
        }
        proxy_pass http://old_server;
    }
}
```text
<!-- Code example in TEXT -->

**Monitor:**

- [ ] FraiseQL error rate < 0.1%
- [ ] Response latency acceptable
- [ ] No data inconsistencies
- [ ] No unauthorized access

### Step 4.2: Increase Traffic (Week 2)

```nginx
<!-- Code example in NGINX -->
# 50% to each server
if ($random < 0.5) {
    proxy_pass http://new_server;
}
proxy_pass http://old_server;
```text
<!-- Code example in TEXT -->

**Monitor:**

- [ ] Error rate < 0.5%
- [ ] Performance stable
- [ ] No customer complaints

### Step 4.3: Full Cutover (Week 3)

```nginx
<!-- Code example in NGINX -->
# 100% to FraiseQL
proxy_pass http://new_server;
```text
<!-- Code example in TEXT -->

**Post-cutover monitoring:**

- [ ] Error rate < 0.1%
- [ ] Latency acceptable
- [ ] All metrics normal
- [ ] Rollback plan ready if needed

---

## Phase 5: Production Validation

### Step 5.1: Health Checks

```bash
<!-- Code example in BASH -->
# Check server health
curl http://localhost:5000/health

# Check database connectivity
curl -X POST http://localhost:5000/graphql \
  -d '{"query": "{ users { id } }"}'

# Check rate limiting
for i in {1..1000}; do
  curl http://localhost:5000/graphql &
done
wait
```text
<!-- Code example in TEXT -->

### Step 5.2: Monitoring Setup

**Set up observability:**

```yaml
<!-- Code example in YAML -->
# Prometheus metrics
fraiseql_queries_total{method="query", status="success"}
fraiseql_query_duration_seconds{method="query", quantile="0.95"}
fraiseql_errors_total{error_code="E_*"}
fraiseql_db_connections{state="active"}

# Alert thresholds
error_rate > 1%
response_latency_p95 > 500ms
db_connection_exhaustion > 80%
```text
<!-- Code example in TEXT -->

### Step 5.3: Rollback Plan

**If issues arise:**

1. **Immediate:** Redirect 100% traffic back to old server
2. **Investigate:** Debug issue in staging
3. **Fix:** Update schema, redeploy to staging
4. **Re-test:** Validate fix
5. **Retry:** Staged cutover again

```bash
<!-- Code example in BASH -->
# Emergency rollback (1 minute RTO)
nginx -s reload  # Change configuration
# Verify: Traffic going to old server
```text
<!-- Code example in TEXT -->

---

## Validation Checklist

### Pre-Migration

- [ ] Schema audit complete
- [ ] Relationship diagram documented
- [ ] Access patterns identified
- [ ] Migration plan approved by team
- [ ] Rollback plan documented

### Schema Development

- [ ] All tables mapped to FraiseQL types
- [ ] All relationships defined
- [ ] Authorization policies configured
- [ ] Row-level security implemented
- [ ] Views optimized (v_*, tv_*)
- [ ] Indexes identified and created

### Testing

- [ ] Staging database cloned from production
- [ ] FraiseQL schema compiles successfully
- [ ] Query compatibility tests pass
- [ ] Performance baseline established
- [ ] Load testing passed (10x peak load)
- [ ] Error handling tested
- [ ] Authorization tested

### Cutover

- [ ] Traffic splitting configured
- [ ] Monitoring alerts set up
- [ ] On-call team briefed
- [ ] Rollback tested
- [ ] Communication plan ready
- [ ] Stakeholders notified

### Post-Migration

- [ ] Error rate < 0.1%
- [ ] Latency within acceptable range
- [ ] All health checks passing
- [ ] No customer-facing issues reported
- [ ] Old server decommissioned
- [ ] Documentation updated

---

## Common Issues & Solutions

### Issue: Data Type Mismatches

**Symptom:** Query returns error or unexpected values.

**Cause:** GraphQL type doesn't match database column type.

**Solution:**

```python
<!-- Code example in Python -->
# Wrong
@type
class Product:
    price: float  # ❌ Float loses precision!

# Correct
@type
class Product:
    price: Decimal  # ✅ Always use Decimal for money
```text
<!-- Code example in TEXT -->

### Issue: Relationship Not Loading

**Symptom:** Query returns null for relationship field.

**Cause:** Foreign key mismatch or missing relationship definition.

**Solution:**

```python
<!-- Code example in Python -->
# Make sure foreign key exists
@type
class Post:
    user_id: str  # Must exist
    user: User    # Relationship must be defined

# Verify in database
SELECT COUNT(*) FROM posts WHERE user_id IS NULL;
```text
<!-- Code example in TEXT -->

### Issue: Authorization Denying All Queries

**Symptom:** All queries return "Unauthorized" even for public data.

**Cause:** Row-level security WHERE clause too restrictive.

**Solution:**

```python
<!-- Code example in Python -->
@type
class Post:
    where: Where = FraiseQL.where(
        # This might be too restrictive!
        is_public=True
    )
```text
<!-- Code example in TEXT -->

**Fix:**

```python
<!-- Code example in Python -->
@type
class Post:
    where: Where = FraiseQL.where(
        # OR condition: public OR owned by user
        is_public=True or fk_user=FraiseQL.context.user_id
    )
```text
<!-- Code example in TEXT -->

---

## Performance Tuning Post-Migration

### Step 1: Identify Slow Queries

```sql
<!-- Code example in SQL -->
-- PostgreSQL
SELECT query, calls, mean_time FROM pg_stat_statements
WHERE mean_time > 100
ORDER BY mean_time DESC LIMIT 20;
```text
<!-- Code example in TEXT -->

### Step 2: Add Indexes

```sql
<!-- Code example in SQL -->
-- From slow queries, identify columns in WHERE clauses
CREATE INDEX idx_posts_user_id ON posts(user_id);
CREATE INDEX idx_posts_created_at ON posts(created_at);
```text
<!-- Code example in TEXT -->

### Step 3: Materialize Expensive Views

```python
<!-- Code example in Python -->
@type
class UserStats:
    """Changed from v_user_stats (logical) to tv_user_stats (materialized)."""
    id: UUID  # UUID v4 for GraphQL ID
    post_count: int
    total_engagement: int
```text
<!-- Code example in TEXT -->

### Step 4: Enable Query Caching

```toml
<!-- Code example in TOML -->
[FraiseQL.caching]
enabled = true
default_ttl_seconds = 300
```text
<!-- Code example in TEXT -->

---

## See Also

**Related Guides:**

- **[Schema Design Best Practices](./schema-design-best-practices.md)** — Designing effective schemas
- **[Common Gotchas](./common-gotchas.md)** — Pitfalls to avoid during migration
- **[Performance Tuning Runbook](../operations/performance-tuning-runbook.md)** — Optimizing post-migration
- **[Production Deployment](./production-deployment.md)** — Deployment procedures
- **[View Selection Guide](./view-selection-performance-testing.md)** — Optimizing view types

**Architecture & Reference:**

- **[Authorization & RBAC](../enterpri../../guides/authorization-quick-start.md)** — Row-level security setup
- **[Federation Guide](../integrations/federation/guide.md)** — Multi-database migration
- **[Schema Compilation](../architecture/core/compilation-phases.md)** — How schemas compile

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
