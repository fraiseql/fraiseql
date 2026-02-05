# Schema Design Best Practices for FraiseQL

**Status:** ✅ Production Ready
**Audience:** Architects, Developers
**Reading Time:** 30-40 minutes
**Last Updated:** 2026-02-05

Best practices and patterns for designing performant, maintainable FraiseQL schemas with compile-time optimization.

---

## Overview

FraiseQL schemas compile to optimized SQL at build time, enabling deterministic query planning and performance. Effective schema design leverages this compilation process while avoiding common pitfalls.

**Key principle**: Think in terms of **compiled SQL execution**, not GraphQL field resolution.

---

## 1. View Type Selection: v_*vs tv_* vs va_*vs ta_*

### Decision Matrix

| View Type | Computation | Storage | Performance | Use Case | Indexing |
|---|---|---|---|---|---|
| **v_*** (Logical) | Per-query | None | Slow (computed) | Simple fields, real-time | N/A |
| **tv_*** (Table-backed) | Periodic refresh | Materialized table | Fast (pre-computed) | Complex aggregations | Native indexes |
| **va_*** (Arrow logical) | Per-query | None (Arrow) | Slow | Analytics, columnar | N/A |
| **ta_*** (Arrow table-backed) | Periodic refresh | Arrow files | Fast | Analytics export | Arrow indexes |

### When to Use Each

#### **v_* (Logical Views)** — For Simple, Real-Time Data

**Use when:**

- Simple computed fields (concatenation, math)
- Data changes frequently
- Storage overhead not acceptable
- Real-time accuracy critical

**Example:**

```python
@fraiseql.type
class UserProfile:
    """Logical view - computed per query."""
    id: ID
    first_name: str
    last_name: str
    full_name: str = field(computed="CONCAT(first_name, ' ', last_name)")
    age: int = field(computed="YEAR(NOW()) - YEAR(birth_date)")
```

**Performance characteristics:**

- Query latency: 50-200ms (depends on computation)
- Storage: None (computed in view)
- Scalability: Degrades linearly with row count

**When NOT to use:**

- ❌ Aggregating millions of rows (GROUP BY on large table)
- ❌ Complex joins (>3 tables)
- ❌ Machine learning inference
- ❌ Expensive calculations (trigonometry, encoding)

#### **tv_* (Table-Backed Views)** — For Complex, Pre-Computed Data

**Use when:**

- Complex aggregations (GROUP BY, JOINs)
- Computation expensive (complex math, ML)
- Performance more important than freshness
- Refresh cycle acceptable (hourly/daily)

**Example:**

```python
@fraiseql.type
class UserStats:
    """Table-backed view - materialized and refreshed daily."""
    id: ID
    post_count: int
    comment_count: int
    like_count: int
    avg_post_length: Decimal
    updated_at: DateTime
```

**Materialization strategy:**

```sql
-- Materialization query (runs hourly)
CREATE TABLE tv_user_stats AS
SELECT
    u.id,
    COUNT(DISTINCT p.id) as post_count,
    COUNT(DISTINCT c.id) as comment_count,
    COUNT(DISTINCT l.id) as like_count,
    AVG(LENGTH(p.content)) as avg_post_length,
    NOW() as updated_at
FROM users u
LEFT JOIN posts p ON u.id = p.user_id
LEFT JOIN comments c ON p.id = c.post_id
LEFT JOIN likes l ON c.id = l.comment_id
GROUP BY u.id;
```

**Performance characteristics:**

- Query latency: 1-10ms (table lookup, indexed)
- Storage: ~10-20% of source data
- Scalability: Constant (O(1) lookup)
- Refresh lag: 1 hour (configurable)

**Refresh strategies:**

- **Full refresh**: Recompute entire table daily
- **Incremental refresh**: Only update changed users
- **Real-time refresh**: Update on each mutation (expensive)

#### **va_* (Arrow Logical Views)** — For Analytics Queries

**Use when:**

- Analytics/OLAP workloads (not OLTP)
- Columnar data format preferred (Pandas, Polars, DuckDB)
- Batch export needed
- JSON format too verbose

**Example:**

```python
@fraiseql.type
class ProductAnalytics:
    """Arrow logical projection - columnar format."""
    product_id: ID
    date: Date
    units_sold: int
    revenue: Decimal
    cost: Decimal
```

**Query:**

```python
import pyarrow.flight as flight
import pandas as pd

client = flight.connect("grpc://localhost:30000")
reader = client.do_get(flight.Ticket(b"ProductAnalytics"))
df = reader.read_pandas()  # Zero-copy to pandas!
```

**Performance characteristics:**

- Query latency: 50-500ms (batch scan)
- Data transfer: 10-20x smaller than JSON
- Deserialization: Zero-copy to Arrow/Pandas
- Use case: Exporting 100M+ rows

#### **ta_* (Arrow Table-Backed Views)** — For Pre-Materialized Analytics

**Use when:**

- Analytics table very large (100M+ rows)
- Batch queries common
- Pre-materialization acceptable
- Long-term data warehouse export

**Example:**

```python
@fraiseql.type
class SalesDataWarehouse:
    """Arrow table-backed materialization."""
    date: Date
    region: str
    product_id: ID
    units_sold: int
    revenue: Decimal
```

---

## 2. Naming Conventions

### Table Naming

**Pattern:** `{entity_type}_{qualifier}`

| Pattern | Meaning | Example |
|---|---|---|
| `users` | Base table | `users` |
| `v_{entity}` | Logical view | `v_user_profile` |
| `tv_{entity}_{qualifier}` | Materialized view | `tv_user_stats_daily` |
| `tb_{entity}_change_log` | Change Data Capture | `tb_user_change_log` |
| `va_{entity}` | Arrow logical | `va_product_analytics` |
| `ta_{entity}_{qualifier}` | Arrow materialized | `ta_sales_warehouse_monthly` |

### Field Naming

**Conventions:**

```python
@fraiseql.type
class User:
    # IDs: Use full entity name or standard suffixes
    id: ID                          # Primary key
    tenant_id: ID                   # Foreign key (explicit)
    organization_id: ID             # Foreign key

    # Timestamps: Always with timezone
    created_at: DateTime            # When created
    updated_at: DateTime            # Last update
    deleted_at: DateTime | None     # Soft delete

    # Status: Use enums
    status: UserStatus              # enum (active, inactive, banned)

    # Booleans: Use "is_" or "has_" prefix
    is_active: bool                 # Current state
    has_verified_email: bool        # Capability

    # Counts: Use "count" or "total_"
    post_count: int                 # Number of posts
    total_followers: int            # Total followers

    # Amounts: Use Decimal for money
    account_balance: Decimal        # Never float!
    price_per_unit: Decimal

    # Relationships: Use noun, not verb
    organization: Organization      # Not: organizationOf
    creator: User                   # Not: createdBy
```

### Enum Naming

**Pattern:** `{Entity}{Property}`

```python
class UserStatus(enum.Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    BANNED = "banned"

class OrderStatus(enum.Enum):
    PENDING = "pending"
    CONFIRMED = "confirmed"
    SHIPPED = "shipped"
    DELIVERED = "delivered"
```

---

## 3. Field Type Selection

### Strings: When to Use What

| Type | When to Use | Example | Range |
|---|---|---|---|
| `str` | Short text (< 255 chars) | username, email, name | ✅ Most fields |
| `Text` | Long text (> 255 chars) | description, bio, content | ✅ Descriptions, content |
| `Email` | Email addresses | <contact@example.com> | ✅ Always validate |
| `UUID` | Unique identifiers | 550e8400-e29b-41d4 | ✅ Recommended for IDs |
| `Slug` | URL-safe names | "my-product", "my-post" | ✅ URLs |
| `Url` | Web addresses | <https://example.com> | ✅ Links |

**Anti-pattern:**

```python
# ❌ Wrong: String ID instead of UUID
id: str = "abc123"

# ✅ Correct: UUID for identifiers
id: UUID
```

### Numbers: Precision Matters

| Type | When to Use | Example | Precision |
|---|---|---|---|
| `int` | Integers (counts, IDs) | user_count, age | 64-bit |
| `float` | Scientific/non-financial | temperature, ratio | IEEE 754 (~7 digits) |
| `Decimal` | Money, accounting | price, balance | Arbitrary (use!) |
| `BigInt` | Very large integers | transaction_id | 128-bit |

**Anti-pattern:**

```python
# ❌ Wrong: Float for money (precision loss!)
account_balance: float = 99.99

# ✅ Correct: Decimal for money
account_balance: Decimal = Decimal("99.99")
```

### Dates & Times

| Type | When to Use | Example | Timezone |
|---|---|---|---|
| `Date` | Date only (no time) | 2026-02-05 | None |
| `DateTime` | Date + time | 2026-02-05T10:30:00Z | Always UTC |
| `Time` | Time only (no date) | 10:30:00 | None |

**Best practice:**

```python
@fraiseql.type
class Event:
    id: ID
    created_at: DateTime  # Always use DateTime (includes time)
    event_date: Date      # Use Date if time not needed
    event_time: Time      # Rare; use DateTime instead
```

---

## 4. Relationship Design

### One-to-Many Relationships

**Pattern:** Foreign key + List type

```python
@fraiseql.type
class User:
    id: ID
    name: str
    posts: List[Post]  # One-to-many: User has many Posts

@fraiseql.type
class Post:
    id: ID
    user_id: ID  # Foreign key
    user: User   # Back-reference (optional)
    content: str
```

**Performance consideration:**

- Eager-load related entities to avoid N+1
- Use table-backed view if aggregation expensive

### Many-to-Many Relationships

**Pattern:** Join table (use TV for performance)

```python
@fraiseql.type
class User:
    id: ID
    name: str
    groups: List[Group]  # Many-to-many via join table

@fraiseql.type
class Group:
    id: ID
    name: str
    members: List[User]

# Join table (hidden from GraphQL, used by TV)
# CREATE TABLE user_groups (
#     user_id UUID,
#     group_id UUID,
#     joined_at TIMESTAMP,
#     PRIMARY KEY (user_id, group_id)
# );
```

**Implementation with table-backed view:**

```sql
CREATE TABLE tv_user_groups AS
SELECT
    u.id,
    JSONB_AGG(
        JSONB_BUILD_OBJECT(
            'id', g.id,
            'name', g.name,
            'joined_at', ug.joined_at
        )
    ) as groups
FROM users u
LEFT JOIN user_groups ug ON u.id = ug.user_id
LEFT JOIN groups g ON ug.group_id = g.id
GROUP BY u.id;
```

### Self-Referential Relationships

**Pattern:** Foreign key to same table

```python
@fraiseql.type
class Category:
    id: ID
    name: str
    parent_id: ID | None  # Can be null (root category)
    parent: Category | None  # Back-reference
    children: List[Category]  # Subcategories
```

**Query limitation:** Prevent infinite recursion

```toml
[fraiseql.validation]
max_query_depth = 10  # Prevent Category -> Category -> Category...
```

---

## 5. Index Design for Performance

### When to Add Indexes

**ADD indexes for:**

- ✅ All foreign keys (`user_id`, `org_id`)
- ✅ Fields in WHERE clauses (filters)
- ✅ @key fields in federation
- ✅ Frequently sorted fields (ORDER BY)
- ✅ Unique fields (UNIQUE constraint is an index)
- ✅ High cardinality fields (many distinct values)

**AVOID indexing:**

- ❌ Very low cardinality fields (boolean, status with 3 values)
- ❌ Fields that are never filtered
- ❌ Non-selective indexes (>50% of rows match)
- ❌ Oversized TEXT fields (use full-text search instead)

### Index Strategies by Query Pattern

**Pattern: Simple WHERE clause**

```sql
-- Query: users WHERE created_at >= '2026-01-01'
CREATE INDEX idx_users_created_at ON users(created_at);
```

**Pattern: Composite filters**

```sql
-- Query: users WHERE tenant_id = ? AND is_active = true
CREATE INDEX idx_users_tenant_active ON users(tenant_id, is_active);
```

**Pattern: Foreign key joins**

```sql
-- Query: posts WHERE user_id = ?
CREATE INDEX idx_posts_user_id ON posts(user_id);
```

**Pattern: Full-text search**

```sql
-- Query: products WHERE name ILIKE '%search%'
CREATE INDEX idx_products_name_trgm ON products USING GIST(name gist_trgm_ops);  -- PostgreSQL
```

---

## 6. Computed Fields: When & How

### Computed Field Patterns

**Pattern 1: Simple concatenation (use v_*)**

```python
@fraiseql.type
class User:
    first_name: str
    last_name: str
    full_name: str = field(computed="CONCAT(first_name, ' ', last_name)")
```

**Pattern 2: Complex aggregation (use tv_*)**

```python
@fraiseql.type
class User:
    id: ID
    post_count: int  # Materialized (updated hourly)
    comment_count: int
    total_engagement: int  # = post_count + comment_count
```

**Pattern 3: Conditional logic (use CASE)**

```python
@fraiseql.type
class Order:
    id: ID
    status: OrderStatus
    status_label: str = field(
        computed="""
        CASE status
            WHEN 'pending' THEN 'Waiting for payment'
            WHEN 'confirmed' THEN 'Order confirmed'
            WHEN 'shipped' THEN 'In transit'
            WHEN 'delivered' THEN 'Delivered'
        END
        """
    )
```

---

## 7. Authorization & Security in Schema

### Field-Level Authorization

**Pattern:** Mark sensitive fields as authorized

```python
@fraiseql.type
class User:
    id: ID
    name: str
    email: str = field(authorize={Roles.SELF, Roles.ADMIN})
    salary: Decimal = field(authorize={Roles.HR})
    password_hash: str = field(authorize=set())  # Never readable
```

### Row-Level Security

**Pattern:** Use WHERE clause for multi-tenancy

```python
@fraiseql.type
class Post:
    where: Where = fraiseql.where(
        fk_org=fraiseql.context.org_id,  # Only user's org
        is_public=True or fk_user=fraiseql.context.user_id  # Public or own posts
    )

    id: ID
    content: str
    user_id: ID
    is_public: bool
```

---

## 8. Backward Compatibility & Schema Evolution

### Adding Fields (✅ Safe)

```python
# Old schema
@fraiseql.type
class User:
    id: ID
    name: str

# New schema (clients still work!)
@fraiseql.type
class User:
    id: ID
    name: str
    email: str  # ← New field (clients ignore it)
```

### Removing Fields (❌ Breaking)

```python
# Old schema
@fraiseql.type
class User:
    id: ID
    name: str
    legacy_field: str

# New schema (breaks clients expecting legacy_field!)
@fraiseql.type
class User:
    id: ID
    name: str
```

**Safe alternative: Deprecate first**

```python
@fraiseql.type
class User:
    id: ID
    name: str
    legacy_field: str = field(
        deprecated="Use 'name' field instead. Removing in v2.1",
        deprecation_reason="Use 'name' field"
    )
```

### Renaming Fields (❌ Breaking)

**Workaround: Add alias**

```python
@fraiseql.type
class User:
    id: ID
    name: str
    full_name: str = field(alias="name")  # Support both names
```

---

## 9. Testing Schema Performance

### Load Testing View Performance

```bash
# Generate test data
INSERT INTO users (id, name, ...) SELECT ... FROM generate_series(1, 1000000);

# Time logical view query
EXPLAIN ANALYZE SELECT * FROM v_user_profile LIMIT 100;

# If > 100ms, switch to table-backed view (tv_user_profile)
```

### Index Effectiveness

```sql
-- Check if index is used
EXPLAIN SELECT * FROM users WHERE created_at >= '2026-01-01';
-- Should show "Index Scan" not "Seq Scan"

-- Check index size
SELECT pg_size_pretty(pg_relation_size('idx_users_created_at'));
```

---

## 10. Monitoring Schema Health

### Query for Unused Indexes

```sql
-- PostgreSQL: Find unused indexes
SELECT schemaname, tablename, indexname
FROM pg_indexes
WHERE indexname NOT IN (
    SELECT indexname FROM pg_stat_user_indexes
    WHERE idx_scan > 0
)
ORDER BY tablename, indexname;
```

### Query for Missing Indexes

```sql
-- Check slow queries
SELECT query, calls, mean_time FROM pg_stat_statements
WHERE mean_time > 100  -- Queries > 100ms
ORDER BY mean_time DESC;

-- Analyze missing indexes from slow queries
```

---

## 11. Schema Documentation

### Document Each Type

```python
@fraiseql.type
class User:
    """
    User account and profile information.

    Fields:
    - id: Unique user identifier (UUID)
    - email: User's email (unique, case-insensitive)
    - name: User's display name
    - created_at: Account creation timestamp
    - posts: User's published posts (1-to-many)

    Indexes:
    - email (unique)
    - created_at (for pagination)

    Row-Level Security:
    - Users can only see public posts or their own posts
    - Email visible to authenticated users only

    Related:
    - Post (1-to-many relationship)
    - Organization (many-to-one)
    """
    id: ID
    email: str
    name: str
    created_at: DateTime
    posts: List[Post]
```

### Document View Materialization

```python
@fraiseql.type
class UserStats:
    """
    User engagement statistics (materialized daily).

    Materialization:
    - Refresh: Daily at 2 AM UTC
    - Source: Aggregate from posts, comments, likes tables
    - Latency: ~1-24 hours (yesterday's data)
    - Storage: ~5GB for 10M users

    Use cases:
    - User ranking/leaderboards
    - Engagement trending
    - NOT for real-time stats

    Updated field:
    - Shows when data was last materialized
    - Use for cache invalidation
    """
```

---

## See Also

**Related Guides:**

- **[Common Gotchas](./common-gotchas.md)** — Schema pitfalls to avoid
- **[Performance Tuning Runbook](../operations/performance-tuning-runbook.md)** — Optimizing schema performance
- **[View Selection Guide](./view-selection-performance-testing.md)** — Testing view performance
- **[Common Patterns](./PATTERNS.md)** — Pattern implementations using best practices

**Architecture & Specifications:**

- **[Schema Compilation Pipeline](../architecture/core/compilation-phases.md)** — How schemas compile to SQL
- **[WHERE Type Generation](../architecture/database/database-targeting.md)** — Filter operator compilation
- **[Scalar Types Reference](../reference/scalars.md)** — All available scalar types
- **[Specs: Schema Conventions](../specs/schema-conventions.md)** — Naming conventions

**Security:**

- **[RBAC & Field Authorization](../enterprise/rbac.md)** — Field-level access control
- **[Production Security Checklist](./production-security-checklist.md)** — Security hardening

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
