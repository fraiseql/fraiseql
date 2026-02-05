# Database Selection Guide

**Status:** ✅ Production Ready
**Audience:** Architects, DevOps, DBAs
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

## Quick Decision

```
PostgreSQL    → Default choice, recommended for most use cases
├─ Why: Best feature support, mature, JSONB, full-text search
│
MySQL 8.0+    → Good for cost-conscious deployments
├─ Why: Cheaper hosting, good performance, simpler operations
│
SQLite        → Local development & testing only
├─ Why: Zero setup, embedded, perfect for prototypes
│
SQL Server    → Enterprise deployments with license
└─ Why: Enterprise support, compatibility with existing infrastructure
```

---

## Comparison Matrix

### Features

| Feature | PostgreSQL | MySQL | SQLite | SQL Server |
|---------|-----------|--------|--------|-----------|
| **Transactions** | ✅ Full ACID | ✅ Full ACID | ✅ Full ACID | ✅ Full ACID |
| **Constraints** | ✅ All types | ⚠️ Basic | ⚠️ Basic | ✅ Full |
| **Window Functions** | ✅ 8.4+ | ✅ 8.0+ | ✅ 3.25+ | ✅ 2012+ |
| **Full-Text Search** | ✅ Native | ✅ Native | ⚠️ Limited | ✅ Native |
| **JSON Support** | ✅ JSONB | ⚠️ JSON | ⚠️ JSON | ✅ JSON |
| **Array Types** | ✅ Native | ❌ No | ❌ No | ❌ No |
| **Foreign Keys** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Indexes** | ✅ Advanced | ✅ Good | ✅ Basic | ✅ Advanced |
| **Partitioning** | ✅ Yes | ✅ Yes | ❌ No | ✅ Yes |
| **Replication** | ✅ Mature | ✅ Mature | ⚠️ Limited | ✅ Mature |

### Performance

| Metric | PostgreSQL | MySQL | SQLite | SQL Server |
|--------|-----------|--------|--------|-----------|
| **Query Speed** | ⚡ Excellent | ⚡ Very Good | ⚡⚡ Local | ⚡ Excellent |
| **Concurrent Writers** | ✅ Excellent | ⚠️ Good (locks) | ❌ Limited | ✅ Excellent |
| **Memory Efficiency** | ✅ Good | ✅ Very Good | ✅ Excellent | ⚠️ Memory hungry |
| **Startup Time** | ⚡ 1-2s | ⚡ 1-2s | ⚡⚡ <100ms | ⚠️ 10-30s |
| **Max Dataset Size** | 🟢 Multi-TB | 🟢 Multi-TB | 🟡 Multi-GB | 🟢 Multi-TB |

### Operational

| Aspect | PostgreSQL | MySQL | SQLite | SQL Server |
|--------|-----------|--------|--------|-----------|
| **Setup Complexity** | Medium | Medium | 🟢 Easy | Complex |
| **Maintenance** | Medium | Low | 🟢 None | Medium |
| **Backup Strategy** | Advanced | Simple | File-based | Advanced |
| **Monitoring Tools** | 🟢 Excellent | Good | Limited | Good |
| **Community Size** | 🟢 Large | 🟢 Large | Medium | Large |
| **Cost** | Free | Free | Free | 💰 Expensive |

---

## Decision Flowchart

### Question 1: Environment?

```
Local Development?
├─ YES → SQLite ✅
│        (Zero setup, perfect for prototyping)
│
└─ NO → Production? (Next question)
```

### Question 2: Team Expertise?

```
Team knows PostgreSQL?
├─ YES → PostgreSQL ✅
│        (Best overall choice)
│
├─ NO: Team knows MySQL well?
│  ├─ YES → MySQL ✅
│  │        (Perfectly fine alternative)
│  │
│  └─ NO → PostgreSQL ✅
│           (Default recommendation)
│
└─ Legacy SQL Server deployments?
   ├─ YES → SQL Server ✅
   │        (Existing infrastructure)
   │
   └─ NO → PostgreSQL or MySQL
```

### Question 3: Specific Needs?

```
Full-text search critical?
├─ YES → PostgreSQL ✅
│        (tsvector built-in)
│
Complex JSON queries?
├─ YES → PostgreSQL ✅
│        (JSONB is superior)
│
Need array types?
├─ YES → PostgreSQL ✅
│        (Native support)
│
Multi-tenant isolation via JSONB?
├─ YES → PostgreSQL ✅
│        (JSONB dimensions)
│
Need lowest cost?
├─ YES → MySQL ✅
│        (Usually cheaper hosting)
│
Greenfield project?
├─ YES → PostgreSQL ✅
│        (Future-proof choice)
│
Existing database?
└─ YES → Use that one ✅
         (Don't change unnecessarily)
```

---

## Detailed Recommendations

### PostgreSQL (Recommended Default)

**Best for:**

- Schema-first applications (FraiseQL strength)
- Complex queries with multi-step JOINs
- Full-text search capabilities needed
- Advanced indexing strategies
- Tenant isolation via JSONB
- Analytics workloads

**Why it wins for FraiseQL:**

- Superior JSONB for multi-tenancy dimensions
- Better indexes for compiled queries
- Window functions mature
- Excellent transaction support (matches FraiseQL's strong consistency model)

**Setup time:** 10-15 minutes (Docker)
**Maintenance:** Medium (monitoring, backups, updates)
**Cost:** Free (licensing-wise)

**Example:**

```bash
# Docker Compose
version: '3.8'
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_PASSWORD: secure_password
    volumes:
      - postgres_data:/var/lib/postgresql/data
volumes:
  postgres_data:
```

---

### MySQL 8.0+ (Cost-Conscious Choice)

**Best for:**

- Organizations with MySQL expertise
- Cost-sensitive deployments (often cheaper hosting)
- Standard OLTP workloads
- Environments already running MySQL

**When to consider:**

- Team comfortable with MySQL
- Simpler operational requirements
- Relational data without complex JSON

**Trade-offs vs PostgreSQL:**

- Slightly slower on complex queries
- Lock contention with heavy writes
- JSONB not as sophisticated
- Full-text search less powerful

**Setup time:** 10-15 minutes (Docker)
**Maintenance:** Low (simpler than PostgreSQL)
**Cost:** Usually cheapest hosting

---

### SQLite (Development Only)

**Best for:**

- Local development
- Testing
- Single-file databases
- Prototyping

**NOT for production:**

- ❌ No true concurrent writes
- ❌ Locks entire database for writers
- ❌ No remote access
- ❌ Limited monitoring

**Setup time:** <1 minute
**Maintenance:** None
**Cost:** Free

**Example:**

```bash
# Create test database (SQLite file)
sqlite3 test.db ".schema"

# Or use in-memory SQLite
export DATABASE_URL="sqlite:///:memory:"
```

---

### SQL Server (Enterprise)

**Best for:**

- Organizations with SQL Server licenses
- Legacy SQL Server deployments
- Existing SQL Server infrastructure
- Enterprise support requirements

**Trade-offs:**

- Expensive licensing
- More complex operations
- Resource-hungry
- Not ideal for small deployments

**Setup time:** 30+ minutes (includes licensing)
**Maintenance:** Medium-High
**Cost:** 💰💰💰 Expensive

---

## Migration Scenarios

### Scenario 1: We're on MySQL, want PostgreSQL

**Effort:** Medium (few hours)
**Downtime:** 10-30 minutes

```bash
# 1. Dump MySQL
mysqldump --all-databases > backup.sql

# 2. Convert schema (usually straightforward)
# Edit backup.sql for PostgreSQL syntax

# 3. Restore to PostgreSQL
psql -U postgres < backup.sql

# 4. Test thoroughly
fraiseql test

# 5. Cutover
# Route connections to PostgreSQL
```

**Risk:** Low if you test thoroughly

### Scenario 2: We're on SQLite, need production database

**Effort:** Low (30 minutes)
**Downtime:** Seconds

```bash
# 1. Create PostgreSQL database
createdb fraiseql

# 2. Export from SQLite
sqlite3 local.db ".dump" > dump.sql

# 3. Convert schema to PostgreSQL format
# Use tool like pgloader or manual edits

# 4. Import to PostgreSQL
psql fraiseql < dump.sql

# 5. Test
fraiseql test

# 6. Cutover
# Update DATABASE_URL environment variable
# Restart application
```

**Risk:** Very low for dev→prod migration

### Scenario 3: Migrate between cloud providers

PostgreSQL maintains consistency across:

- AWS RDS → Google Cloud SQL: Straightforward
- AWS RDS → Azure Database: Straightforward
- Self-hosted → AWS RDS: Use replication

**Tool:** `pg_dump` + `psql` (reliable, battle-tested)

---

## Performance Tuning by Database

### PostgreSQL Optimization

```sql
-- Add indexes for compiled queries
CREATE INDEX idx_query_col ON table(column);

-- Analyze for query planner
ANALYZE table_name;

-- Enable parallel execution
SET max_parallel_workers_per_gather = 4;

-- Connection pooling
-- Use PgBouncer for connection management
```

### MySQL 8.0+ Optimization

```sql
-- Similar indexing
CREATE INDEX idx_query_col ON table(column);

-- Analyze
ANALYZE TABLE table_name;

-- Check execution plan
EXPLAIN SELECT ...

-- Increase buffer pool for workload
SET GLOBAL innodb_buffer_pool_size = 4GB;
```

### Performance Expectations

| Operation | PostgreSQL | MySQL | SQLite |
|-----------|-----------|--------|--------|
| Single row query | 0.5-2ms | 0.5-2ms | <0.1ms |
| Complex join (10 tables) | 5-50ms | 10-100ms | 1-10ms |
| Aggregation (1M rows) | 50-200ms | 100-300ms | 50-150ms |
| Full-text search | 10-50ms | 20-100ms | 100-500ms |

---

## Troubleshooting Database Selection

### "We chose MySQL but need PostgreSQL features"

**Options:**

1. Migrate to PostgreSQL (1-2 hours)
2. Implement feature differently (app-layer JSON parsing)
3. Wait for MySQL to add feature (may never happen)

**Recommendation:** Migrate if feature is critical

### "PostgreSQL is too complex to operate"

**Solutions:**

1. Use managed service (AWS RDS, Heroku)
2. Use monitoring tools (pgAdmin, Grafana)
3. Hire DevOps/DBA expertise
4. Consider MySQL if operational simplicity critical

### "SQLite was fine for dev, but doesn't scale to prod"

**This is expected.** Plan for migration:

```bash
# Timeline
Week 1: Set up PostgreSQL/MySQL
Week 2: Create schema, test
Week 3: Mirror data, validate
Week 4: Cutover and monitor
```

### "We're uncertain between PostgreSQL and MySQL"

**Recommendation:** **Choose PostgreSQL** unless you have:

- Existing MySQL infrastructure
- Team prefers MySQL
- Cost is absolutely critical

PostgreSQL's advantages in schema-first design (FraiseQL specialty) outweigh the complexity trade-off.

---

## See Also

- **[Production Deployment](./production-deployment.md)** - Database setup for prod
- **[Consistency Model](./consistency-model.md)** - How consistency varies by database
- **[Configuration](../configuration/)** - Database-specific configuration
- **[Architecture](../architecture/database/database-targeting.md)** - Technical database architecture

---

**Remember:** Database choice is important but not permanent. Most migrations take hours, not days. Choose a solid database and optimize operations.
