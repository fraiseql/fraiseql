# FraiseQL 1.3.2 Release Notes

**Release Date**: January 7, 2025
**Type**: Feature Release

## Overview

This release adds **9 utility methods** to `FraiseQLRepository`, providing clean, type-safe APIs for common database operations. These methods return Python types (not `RustResponseBytes`) and enable dynamic queries with filters - perfect for interactive dashboards, analytics, and validation logic.

**All methods follow the "everything in DB" pattern**: PostgreSQL performs all computation, Python methods are thin wrappers. This enables frontends to leverage SQL server capabilities for dynamic queries without pre-computing every possible aggregation.

## ✨ New Methods

| Method | Returns | Purpose |
|--------|---------|---------|
| `count()` | `int` | Count records matching filters |
| `exists()` | `bool` | Check if any records exist (faster than count) |
| `sum()` | `float` | Sum a numeric field |
| `avg()` | `float` | Average of a numeric field |
| `min()` | `Any` | Minimum value (preserves type: Decimal, datetime, etc.) |
| `max()` | `Any` | Maximum value (preserves type) |
| `distinct()` | `list[Any]` | Get unique values for a field |
| `pluck()` | `list[Any]` | Extract single field from records |
| `aggregate()` | `dict[str, Any]` | Multiple aggregations in one query |
| `batch_exists()` | `dict[Any, bool]` | Check multiple IDs in one query |

**Total**: 56 new unit tests, all passing ✅

## 📖 Usage Examples

### count() - Count Records
```python
# Count all users
total = await db.count("v_users")
# Returns: 1523

# Count with filters
active = await db.count("v_users", where={"status": {"eq": "active"}})
# Returns: 842

# GraphQL resolver
@query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    db = info.context["db"]
    return await db.count("v_users", where=where)
```

### exists() - Fast Existence Check
```python
# Check if email exists (validation)
if await db.exists("v_users", where={"email": {"eq": input.email}}):
    raise GraphQLError("Email already exists")

# Check if user has pending orders
has_pending = await db.exists(
    "v_orders",
    where={"user_id": {"eq": user_id}, "status": {"eq": "pending"}}
)
# Returns: True or False (faster than count() > 0)
```

### sum() - Sum Numeric Field
```python
# Total revenue
total_revenue = await db.sum("v_orders", "amount")
# Returns: 125000.50

# Total for completed orders only
completed_revenue = await db.sum(
    "v_orders",
    "amount",
    where={"status": {"eq": "completed"}}
)
# Returns: 98750.25

# GraphQL resolver with dynamic filters
@query
async def revenue_total(
    info,
    date_from: datetime,
    date_to: datetime,
    status: str | None = None
) -> float:
    db = info.context["db"]
    where = {"created_at": {"gte": date_from, "lte": date_to}}
    if status:
        where["status"] = {"eq": status}
    return await db.sum("v_orders", "amount", where=where)
```

### avg(), min(), max() - Aggregations
```python
# Average order value
avg_order = await db.avg("v_orders", "amount")
# Returns: 250.00

# Price range
min_price = await db.min("v_products", "price")  # Returns: 9.99
max_price = await db.max("v_products", "price")  # Returns: 999.99

# Earliest/latest dates (preserves datetime type)
first_order = await db.min("v_orders", "created_at")  # Returns: datetime
last_order = await db.max("v_orders", "created_at")   # Returns: datetime

# Dashboard stats
@query
async def dashboard_stats(info) -> DashboardStats:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    return DashboardStats(
        total_revenue=await db.sum("v_orders", "amount", tenant_id=tenant_id),
        avg_order=await db.avg("v_orders", "amount", tenant_id=tenant_id),
        total_orders=await db.count("v_orders", tenant_id=tenant_id),
    )
```

### distinct() - Get Unique Values
```python
# Get all product categories (for filter dropdown)
categories = await db.distinct("v_products", "category")
# Returns: ["books", "clothing", "electronics"]

# Get order statuses for tenant
statuses = await db.distinct("v_orders", "status", tenant_id=tenant_id)
# Returns: ["cancelled", "completed", "pending"]

# GraphQL resolver for filter options
@query
async def product_filter_options(info) -> ProductFilterOptions:
    """Dynamic filter options based on actual data."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    return ProductFilterOptions(
        categories=await db.distinct("v_products", "category", tenant_id=tenant_id),
        brands=await db.distinct("v_products", "brand", tenant_id=tenant_id),
        colors=await db.distinct("v_products", "color", tenant_id=tenant_id),
    )
```

### pluck() - Extract Single Field
```python
# Get all user IDs (more efficient than fetching full objects)
user_ids = await db.pluck("v_users", "id")
# Returns: [uuid1, uuid2, uuid3, ...]

# Get emails for active users
emails = await db.pluck(
    "v_users",
    "email",
    where={"status": {"eq": "active"}}
)
# Returns: ["user1@example.com", "user2@example.com", ...]

# Bulk operation: Send emails to segment
@mutation
async def send_campaign_email(info, segment: str) -> EmailCampaignResult:
    db = info.context["db"]

    # Get just emails (efficient - only transfers email field)
    emails = await db.pluck(
        "v_users",
        "email",
        where={"segment": {"eq": segment}, "subscribed": {"eq": True}}
    )

    # Send emails in batches
    for batch in chunk(emails, 100):
        await send_email_batch(batch, "campaign_template")

    return EmailCampaignResult(total_sent=len(emails))
```

### aggregate() - Multiple Aggregations
```python
# Multiple metrics in one query (efficient!)
stats = await db.aggregate(
    "v_orders",
    aggregations={
        "total_revenue": "SUM(amount)",
        "avg_order": "AVG(amount)",
        "max_order": "MAX(amount)",
        "min_order": "MIN(amount)",
        "order_count": "COUNT(*)",
        "unique_customers": "COUNT(DISTINCT customer_id)",
    },
    where={"status": {"eq": "completed"}},
    tenant_id=tenant_id
)
# Returns: {
#     "total_revenue": 125000.50,
#     "avg_order": 250.00,
#     "max_order": 1500.00,
#     "min_order": 10.00,
#     "order_count": 500,
#     "unique_customers": 287
# }

# GraphQL resolver
@query
async def sales_analytics(
    info,
    period: str,  # "today", "week", "month", "year"
    category: str | None = None
) -> SalesAnalytics:
    """Multi-metric analytics computed by PostgreSQL in real-time."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    date_cutoff = {
        "today": datetime.now().replace(hour=0, minute=0),
        "week": datetime.now() - timedelta(days=7),
        "month": datetime.now() - timedelta(days=30),
        "year": datetime.now() - timedelta(days=365),
    }[period]

    where = {"created_at": {"gte": date_cutoff}}
    if category:
        where["category"] = {"eq": category}

    stats = await db.aggregate(
        "v_orders",
        aggregations={
            "total_revenue": "SUM(amount)",
            "avg_order_value": "AVG(amount)",
            "total_orders": "COUNT(*)",
            "unique_customers": "COUNT(DISTINCT customer_id)",
        },
        where=where,
        tenant_id=tenant_id,
    )

    return SalesAnalytics(**stats)
```

### batch_exists() - Batch ID Validation
```python
# Validate multiple IDs in ONE query (not N queries)
user_ids = [uuid1, uuid2, uuid3]
existence = await db.batch_exists("v_users", user_ids)
# Returns: {uuid1: True, uuid2: False, uuid3: True}

# Find missing IDs
missing = [id for id, exists in existence.items() if not exists]
if missing:
    raise GraphQLError(f"Users not found: {missing}")

# Mutation with bulk validation
@mutation
async def assign_users_to_project(
    info,
    project_id: UUID,
    user_ids: list[UUID]
) -> AssignmentResult:
    """Assign multiple users to project with validation."""
    db = info.context["db"]

    # Validate all users exist in ONE SQL query (efficient)
    existence = await db.batch_exists("v_users", user_ids)

    missing = [id for id, exists in existence.items() if not exists]
    if missing:
        raise GraphQLError(f"Users not found: {missing}")

    # All validated - proceed with assignment
    # ...
```

## 🎯 Use Cases: When to Use Utility Methods vs Pre-Computed Views

| Scenario | Use Utility Methods ✅ | Use Pre-Computed View 🏗️ |
|----------|----------------------|--------------------------|
| **User applies filters** | `db.sum("v_orders", "amount", where={...})` | N/A - can't pre-compute all filter combos |
| **Dynamic date ranges** | `db.count("v_orders", where={"created_at": {"gte": user_date}})` | N/A - infinite possible date ranges |
| **Interactive dashboards** | Stats change based on user selections | N/A - pre-computing all combos = exponential storage |
| **Filter dropdowns** | `db.distinct("v_products", "category")` | Could work, but DISTINCT is simpler |
| **Validation** | `db.exists("v_users", where={"email": email})` | N/A - validating arbitrary user input |
| **Bulk operations** | `db.pluck("v_orders", "id", where={...})` | N/A - don't know IDs in advance |
| **Fixed daily stats** | N/A - stats change constantly | `v_daily_revenue` materialized view ✅ |
| **Cached aggregations** | N/A - need stale data for performance | `v_user_stats` with triggers ✅ |

**Rule of Thumb**:
- Use **utility methods** when queries are **dynamic** (user-driven filters, date ranges, etc.)
- Use **pre-computed views** when results are **static** (always same calculation, queried frequently)

## 📦 Implementation Details

### Code Changes

**File**: `src/fraiseql/db.py`

**New Methods**: All 9 methods added (lines 845-1396)
- `exists()` (line 845)
- `sum()` (line 906)
- `avg()` (line 968)
- `min()` (line 1027)
- `max()` (line 1077)
- `distinct()` (line 1127)
- `pluck()` (line 1181)
- `aggregate()` (line 1252)
- `batch_exists()` (line 1327)

### Design Principles

**✅ Database Does the Work**:
- All computation happens in PostgreSQL
- Python methods are **thin wrappers** around SQL operations
- Aligns with "everything in DB" pattern

**✅ SQL Injection Protection**:
- All table names use `psycopg.sql.Identifier`
- All field names use `psycopg.sql.Identifier`
- Schema-qualified tables (`schema.table`) handled correctly
- LIMIT/OFFSET use `Literal` for safe parameterization

**✅ Datatype Handling**:
- `sum()` and `avg()`: Convert PostgreSQL `Decimal` → Python `float` for consistency
- `min()` and `max()`: Return value **as-is** to preserve original type (Decimal, datetime, etc.)
- `distinct()` and `pluck()`: Return values as-is (preserves all datatypes)
- `batch_exists()`: Returns boolean dict (type-agnostic for IDs)

**✅ Consistent API**:
- Same filter syntax as `find()` and `count()`
- Support `where`, `tenant_id`, and other kwargs
- Predictable parameter ordering

### Test Coverage

**File**: `tests/unit/db/test_db_utility_methods.py`

**Test Suite**: 56/56 tests passing ✅

Coverage includes:
- **exists()**: 8 tests (method existence, return types, filters, SQL generation)
- **sum()**: 8 tests (return types, null handling, filters, field parameter validation)
- **avg()**: 4 tests (return types, null handling, SQL generation)
- **min()**: 4 tests (return types, null handling, SQL generation)
- **max()**: 4 tests (return types, null handling, SQL generation)
- **distinct()**: 7 tests (return types, empty results, filters, ordering)
- **pluck()**: 7 tests (return types, empty results, filters, limit support)
- **aggregate()**: 6 tests (multiple aggregations, filters, empty dict handling)
- **batch_exists()**: 8 tests (all exist, some missing, none exist, custom field)

### Regression Testing

**Results**: All existing tests pass ✅
- 104/104 total db unit tests passing
- 56 new + 48 existing
- Zero breaking changes

## 🔄 Migration Guide

### No Breaking Changes

This is an **additive feature** - no migration required:
- ✅ Existing code continues to work
- ✅ No API changes to existing methods
- ✅ Backward compatible
- ✅ `CQRSRepository.count()` still available for legacy code

### For Users Previously Using Workarounds

**Before (CQRSRepository workaround)**:
```python
from fraiseql import CQRSRepository

@query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    repo = CQRSRepository(info.context["connection"])
    return await repo.count(User, where=where)  # Legacy API
```

**After (clean modern API)**:
```python
@query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    db = info.context["db"]
    return await db.count("v_users", where=where)  # ✅ Modern API
```

**Before (raw SQL workaround)**:
```python
@query
async def total_revenue(info) -> float:
    db = info.context["db"]
    async with db._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute("SELECT SUM(amount) FROM v_orders")
            result = await cursor.fetchone()
            return float(result[0]) if result else 0.0
```

**After (clean API)**:
```python
@query
async def total_revenue(info) -> float:
    db = info.context["db"]
    return await db.sum("v_orders", "amount")  # ✅ Clean & type-safe
```

## 🏗️ Development Process

### TDD Methodology

All methods developed following strict Test-Driven Development:

1. **RED Phase**: Wrote 56 failing tests defining expected behavior
2. **GREEN Phase**: Implemented minimal code to pass all tests
3. **REFACTOR Phase**: Optimized and cleaned up implementation
4. **QA Phase**: Verified full test suite (104/104 passing, zero regressions)

### Code Quality

- ✅ Full test coverage (56 unit tests)
- ✅ Comprehensive docstrings
- ✅ Type hints on all methods
- ✅ Follows project patterns
- ✅ SQL injection protection
- ✅ Performance optimized

## 🔗 Related Issues

- **Resolves**: #116 - Add count() method to FraiseQLRepository
- **Resolves**: #117 - Add utility methods for API consistency
- **Related**: #114 - User discovered count() limitation

## 📞 Support

For questions or issues:
- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Documentation: https://docs.fraiseql.com

---

**Full Changelog**: https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md
