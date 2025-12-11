# Where Clause Syntax Comparison: WhereType vs Dict

Quick reference comparing FraiseQL's two where clause syntaxes.

## Quick Decision Guide

| Your Situation | Use This Syntax |
|----------------|-----------------|
| Writing GraphQL resolvers | **WhereType** (type safety) |
| Building query helpers | **WhereType** (IDE autocomplete) |
| Repository layer | **Dict** (flexibility) |
| Dynamic queries from user input | **Dict** (runtime flexibility) |
| Testing with quick filters | **Dict** (less boilerplate) |
| Complex nested queries | Either (preference) |

---

## Basic Filtering

### Simple Field Filter

**WhereType:**
```python
from fraiseql.sql import StringFilter, BooleanFilter

where = UserWhereInput(
    name=StringFilter(contains="John"),
    is_active=BooleanFilter(eq=True)
)
```

**Dict:**
```python
where = {
    "name": {"contains": "John"},
    "is_active": {"eq": True}
}
```

---

## Nested Object Filtering

### Filter by Related Object

**WhereType:**
```python
where = AssignmentWhereInput(
    status=StringFilter(eq="active"),
    device=DeviceWhereInput(
        is_active=BooleanFilter(eq=True),
        name=StringFilter(contains="server")
    )
)
```

**Dict:**
```python
where = {
    "status": {"eq": "active"},
    "device": {
        "is_active": {"eq": True},
        "name": {"contains": "server"}
    }
}
```

**Generated SQL (both):**
```sql
WHERE data->>'status' = 'active'
  AND data->'device'->>'is_active' = 'true'
  AND data->'device'->>'name' ILIKE '%server%'  -- icontains operator (case-insensitive)
```

---

## Logical Operators

### AND Operator

**WhereType:**
```python
where = UserWhereInput(
    AND=[
        UserWhereInput(age=IntFilter(gte=18)),
        UserWhereInput(age=IntFilter(lte=65)),
        UserWhereInput(is_active=BooleanFilter(eq=True))
    ]
)
```

**Dict:**
```python
where = {
    "AND": [
        {"age": {"gte": 18}},
        {"age": {"lte": 65}},
        {"is_active": {"eq": True}}
    ]
}
```

### OR Operator

**WhereType:**
```python
where = UserWhereInput(
    OR=[
        UserWhereInput(role=StringFilter(eq="admin")),
        UserWhereInput(role=StringFilter(eq="moderator"))
    ]
)
```

**Dict:**
```python
where = {
    "OR": [
        {"role": {"eq": "admin"}},
        {"role": {"eq": "moderator"}}
    ]
}
```

### NOT Operator

**WhereType:**
```python
where = UserWhereInput(
    NOT=UserWhereInput(
        is_active=BooleanFilter(eq=False)
    )
)
```

**Dict:**
```python
where = {
    "NOT": {
        "is_active": {"eq": False}
    }
}
```

---

## Complex Nested Logic

### Nested AND/OR/NOT

**WhereType:**
```python
where = UserWhereInput(
    AND=[
        UserWhereInput(age=IntFilter(gte=21)),
        UserWhereInput(
            OR=[
                UserWhereInput(department=StringFilter(eq="engineering")),
                UserWhereInput(role=StringFilter(eq="admin"))
            ]
        ),
        UserWhereInput(
            NOT=UserWhereInput(tags=ArrayFilter(contains="inactive"))
        )
    ]
)
```

**Dict:**
```python
where = {
    "AND": [
        {"age": {"gte": 21}},
        {
            "OR": [
                {"department": {"eq": "engineering"}},
                {"role": {"eq": "admin"}}
            ]
        },
        {
            "NOT": {"tags": {"contains": "inactive"}}
        }
    ]
}
```

---

## Multiple Nested Fields

### Filter by Multiple Properties of Same Nested Object

**WhereType:**
```python
where = AssignmentWhereInput(
    device=DeviceWhereInput(
        is_active=BooleanFilter(eq=True),
        name=StringFilter(contains="router"),
        location=StringFilter(eq="datacenter-1"),
        cpu_count=IntFilter(gte=4)
    )
)
```

**Dict:**
```python
where = {
    "device": {
        "is_active": {"eq": True},
        "name": {"contains": "router"},
        "location": {"eq": "datacenter-1"},
        "cpu_count": {"gte": 4}
    }
}
```

---

## CamelCase Support

### Automatic Conversion

**WhereType:**
```python
# Field names use snake_case in WhereInput types
where = DeviceWhereInput(
    is_active=BooleanFilter(eq=True),  # is_active
    device_name=StringFilter(contains="server")  # device_name
)
```

**Dict:**
```python
# Dict accepts both camelCase AND snake_case
where = {
    "isActive": {"eq": True},       # ✅ Auto-converts to is_active
    "deviceName": {"contains": "server"}  # ✅ Auto-converts to device_name
}

# OR use snake_case directly
where = {
    "is_active": {"eq": True},
    "device_name": {"contains": "server"}
}
```

**Key Difference:** Dict syntax accepts camelCase input and auto-converts, making it ideal for GraphQL client inputs.

---

## Dynamic Query Building

### Runtime Filter Construction

**WhereType:**
```python
def build_filter(criteria: dict) -> UserWhereInput:
    filters = []

    if criteria.get("min_age"):
        filters.append(
            UserWhereInput(age=IntFilter(gte=criteria["min_age"]))
        )

    if criteria.get("department"):
        filters.append(
            UserWhereInput(department=StringFilter(eq=criteria["department"]))
        )

    if filters:
        return UserWhereInput(AND=filters)
    return UserWhereInput()
```

**Dict (simpler):**
```python
def build_filter(criteria: dict) -> dict:
    where = {}

    if criteria.get("min_age"):
        where["age"] = {"gte": criteria["min_age"]}

    if criteria.get("department"):
        where["department"] = {"eq": criteria["department"]}

    return where
```

**Winner:** Dict is simpler for dynamic queries.

---

## Usage in Resolvers

### GraphQL Query Resolver

**WhereType:**
```python
import fraiseql

@fraiseql.query
async def active_assignments(
    info,
    device_name: str | None = None
) -> list[Assignment]:
    db = info.context["db"]

    filters = [
        AssignmentWhereInput(status=StringFilter(eq="active"))
    ]

    if device_name:
        filters.append(
            AssignmentWhereInput(
                device=DeviceWhereInput(
                    name=StringFilter(contains=device_name)
                )
            )
        )

    where = AssignmentWhereInput(AND=filters) if len(filters) > 1 else filters[0]
    return await db.find("assignments", where=where)
```

**Dict:**
```python
import fraiseql

@fraiseql.query
async def active_assignments(
    info,
    device_name: str | None = None
) -> list[Assignment]:
    db = info.context["db"]

    where = {"status": {"eq": "active"}}

    if device_name:
        where["device"] = {"name": {"contains": device_name}}

    return await db.find("assignments", where=where)
```

**Winner:** Dict is more concise for conditional filters.

---

## Common Operators

### String Operators

| Operator | WhereType | Dict |
|----------|-----------|------|
| Equals | `StringFilter(eq="value")` | `{"eq": "value"}` |
| Contains | `StringFilter(contains="val")` | `{"contains": "val"}` |
| Starts with | `StringFilter(startswith="val")` | `{"startswith": "val"}` |
| Ends with | `StringFilter(endswith="val")` | `{"endswith": "val"}` |
| In list | `StringFilter(in_=["a", "b"])` | `{"in": ["a", "b"]}` |
| Is null | `StringFilter(isnull=True)` | `{"isnull": True}` |

### Numeric Operators

| Operator | WhereType | Dict |
|----------|-----------|------|
| Equals | `IntFilter(eq=5)` | `{"eq": 5}` |
| Greater than | `IntFilter(gt=5)` | `{"gt": 5}` |
| Greater/equal | `IntFilter(gte=5)` | `{"gte": 5}` |
| Less than | `IntFilter(lt=5)` | `{"lt": 5}` |
| Less/equal | `IntFilter(lte=5)` | `{"lte": 5}` |
| In list | `IntFilter(in_=[1, 2, 3])` | `{"in": [1, 2, 3]}` |

### Array Operators

| Operator | WhereType | Dict |
|----------|-----------|------|
| Contains | `ArrayFilter(contains="tag")` | `{"contains": "tag"}` |
| Overlaps | `ArrayFilter(overlaps=["a", "b"])` | `{"overlaps": ["a", "b"]}` |
| Length equals | `ArrayFilter(len_eq=3)` | `{"len_eq": 3}` |
| Length > | `ArrayFilter(len_gt=5)` | `{"len_gt": 5}` |

---

## Best Practices

### Use WhereType When:

✅ **Type safety is important**
```python
# IDE will catch typos and type errors
where = UserWhereInput(
    naem=StringFilter(eq="John")  # ❌ IDE error: no attribute 'naem'
)
```

✅ **Building reusable query helpers**
```python
def get_active_users_filter() -> UserWhereInput:
    """Reusable filter with type hints."""
    return UserWhereInput(is_active=BooleanFilter(eq=True))
```

✅ **Complex queries benefit from autocomplete**
```python
# Full IDE autocomplete for nested objects
where = PostWhereInput(
    author=AuthorWhereInput(  # ← IDE shows all AuthorWhereInput fields
        department=StringFilter(eq="engineering")
    )
)
```

### Use Dict When:

✅ **Building filters dynamically**
```python
# Easy to add/remove fields at runtime
where = {}
if user_active is not None:
    where["is_active"] = {"eq": user_active}
```

✅ **Working with GraphQL client input**
```python
# Accept camelCase from frontend
where = graphql_variables["where"]  # Already a dict
results = await db.find("users", where=where)
```

✅ **Quick tests and scripts**
```python
# Less boilerplate for simple queries
await db.find("users", where={"age": {"gt": 18}})
```

---

## Summary

| Aspect | WhereType | Dict |
|--------|-----------|------|
| **Type Safety** | ✅ Full | ⚠️ Runtime only |
| **IDE Support** | ✅ Autocomplete | ❌ No autocomplete |
| **Syntax Verbosity** | More verbose | More concise |
| **Dynamic Queries** | Possible but awkward | Natural and easy |
| **Learning Curve** | Steeper | Gentler |
| **Best For** | Type-safe resolvers | Dynamic queries |
| **GraphQL Client Compat** | Manual conversion | Direct usage |
| **CamelCase Input** | Manual conversion | Auto-conversion |

**Bottom Line:** Use WhereType for type safety in static queries, Dict for flexibility in dynamic queries. Both support the same operators and nested filtering capabilities!

---

## See Also

- **[Where Input Types - Full Guide](../advanced/where-input-types.md)** - Complete documentation
- **[Dict-Based Nested Filtering](../examples/dict-based-nested-filtering.md)** - Dict syntax deep-dive
- **[Filter Operators Reference](../advanced/filter-operators.md)** - All available operators
- **[Advanced Filtering Examples](../examples/advanced-filtering.md)** - Real-world use cases
