# Dynamic Filter Generation: Trade-offs and Recommendations

## Why We Don't Recommend Dynamic Filter Generation

While dynamic filter generation (like `safe_create_where_type`) seems appealing for reducing boilerplate, it has several significant drawbacks that make explicit filter building a better choice for most applications.

## The Problems with Dynamic Filter Generation

### 1. Poor Developer Experience (DX)

#### GraphQL API Becomes Complex
```graphql
# Dynamic generation creates nested, operator-based APIs
query {
  machines(where: { 
    status: { eq: "active" },
    capacity: { gt: 100, lte: 500 },
    name: { contains: "printer" }
  }) {
    id
  }
}

# vs. Clean, intuitive API with explicit filters
query {
  machines(where: { 
    status: "active",
    capacityMin: 100,
    capacityMax: 500,
    nameContains: "printer"
  }) {
    id
  }
}
```

The nested operator syntax is:
- Harder to read and write
- More verbose
- Less discoverable in GraphQL explorers
- Confusing for frontend developers

### 2. Type Safety Lost

Dynamic generation using `dict[str, Any]` loses all type information:

```python
# With dynamic generation - No IDE help!
where = MachineWhere(
    status={"eq": "active"},      # Is "eq" valid for status?
    capacity={"gte": "hundred"},   # Wrong type - runtime error!
    name={"containz": "printer"}   # Typo - runtime error!
)

# With explicit WhereInput - Full type safety
where = MachineWhereInput(
    status="active",              # IDE knows this is a string
    capacity_min=100,             # IDE knows this is an int
    name_contains="printer"       # IDE autocompletes field names
)
```

### 3. Runtime Errors Instead of Compile-Time

Dynamic generation validates operators at runtime:

```python
# This compiles but fails at runtime
where = PersonWhere(age={"greater_than": 25})  # Invalid operator!

# With explicit types, IDE/mypy catches this immediately
where = PersonWhereInput(age_gt=25)  # Field doesn't exist - IDE error
```

### 4. Debugging Nightmare

When something goes wrong with dynamic filters:

```python
# Dynamic generation - What SQL does this generate?
where = ComplexWhere(
    field1={"in": [1, 2, 3]},
    field2={"gte": date(2024, 1, 1), "lt": date(2024, 12, 31)},
    field3={"isnull": False}
)
# Developer has to trace through dynamic generation code

# Explicit filters - Clear and debuggable
filters = {
    "field1": [1, 2, 3],
    "field2__gte": date(2024, 1, 1),
    "field2__lt": date(2024, 12, 31),
    "field3__isnull": False
}
# Developer sees exactly what's happening
```

### 5. Frontend Integration Issues

Frontend TypeScript/JavaScript developers struggle with dynamic APIs:

```typescript
// Dynamic API - Complex types needed
interface MachineWhere {
  status?: { eq?: string; neq?: string; in?: string[] };
  capacity?: { gt?: number; gte?: number; lt?: number; lte?: number };
  // ... for every field and operator combination
}

// Explicit API - Simple, clean types
interface MachineWhereInput {
  status?: string;
  statusIn?: string[];
  capacityMin?: number;
  capacityMax?: number;
}
```

### 6. Limited Flexibility

Dynamic generation enforces rigid patterns:

```python
# What if you need custom logic?
# Dynamic generation can't handle:
# - Business rule validation
# - Complex date range logic
# - Multi-field dependencies
# - Custom operators

# With explicit filters, you have full control:
def _build_machine_filters(where: MachineWhereInput) -> dict:
    filters = {}
    
    # Custom business logic
    if where.is_available and where.status != "maintenance":
        filters["status"] = ["active", "idle"]
    
    # Complex date handling
    if where.available_in_days:
        filters["next_maintenance__gte"] = date.today() + timedelta(days=where.available_in_days)
    
    return filters
```

### 7. Performance Implications

Dynamic generation adds overhead:

```python
# Dynamic: Parse operators, validate, generate SQL for EVERY query
where = MachineWhere(status={"eq": "active"})
sql = where.to_sql()  # Runtime parsing and generation

# Explicit: Direct mapping, minimal overhead
filters = {"status": "active"}  # Direct to database
```

### 8. Testing Complexity

Testing dynamic filters is harder:

```python
# Dynamic: Need to test operator combinations
def test_machine_where():
    # Test every operator for every field type
    assert MachineWhere(status={"eq": "active"}).to_sql() == "..."
    assert MachineWhere(status={"neq": "active"}).to_sql() == "..."
    assert MachineWhere(status={"in": ["a", "b"]}).to_sql() == "..."
    # Combinatorial explosion of test cases

# Explicit: Simple, focused tests
def test_machine_filters():
    where = MachineWhereInput(status="active", capacity_min=100)
    filters = build_filters(where)
    assert filters == {"status": "active", "capacity__gte": 100}
```

### 9. Migration and Evolution Challenges

When requirements change:

```python
# Dynamic: Changing operator behavior affects ALL types
# Want to add case-insensitive search? Modify core generation logic

# Explicit: Change only what you need
if where.name_contains:
    # Easy to add ILIKE for just this field
    filters["name__ilike"] = f"%{where.name_contains}%"
```

### 10. Documentation and Onboarding

New developers understand explicit patterns immediately:

```python
# Explicit: "Oh, I see how this works"
@fraiseql.query
async def machines(info, where: MachineWhereInput = None):
    filters = {}
    if where:
        if where.status:
            filters["status"] = where.status
        if where.capacity_min:
            filters["capacity__gte"] = where.capacity_min
    return await db.find("machines", **filters)

# Dynamic: "Wait, what's safe_create_where_type? How do operators work?"
MachineWhere = safe_create_where_type(Machine)
# Need to read documentation to understand the magic
```

## When Dynamic Generation Might Make Sense

To be fair, dynamic filter generation can be useful in specific scenarios:

1. **Admin Interfaces**: Where power users need every possible filter combination
2. **Generic CRUD APIs**: When building framework-level tools
3. **Data Export Tools**: Where flexibility matters more than UX
4. **Internal Tools**: Where developers are the primary users

## The Recommended Approach: Explicit with Helpers

The best balance is explicit patterns with smart helpers:

```python
# 1. Clear WhereInput types
@fraise_input
class MachineWhereInput:
    status: str | None = None
    capacity_min: int | None = None
    capacity_max: int | None = None
    name_contains: str | None = None

# 2. Reusable filter builder (not dynamic generation!)
def build_filters(where: Any, base: dict = None) -> dict:
    """Simple, explicit filter building."""
    filters = base or {}
    if not where:
        return filters
    
    # Convert to dict if needed
    where_dict = vars(where) if hasattr(where, '__dict__') else where
    
    # Direct mapping - no magic
    for key, value in where_dict.items():
        if value is not None:
            filters[key] = value
    
    return filters

# 3. Clean query implementation
@fraiseql.query
async def machines(info, where: MachineWhereInput = None):
    filters = build_filters(where, {"tenant_id": info.context.get("tenant_id")})
    return await db.find("machines", **filters)
```

This gives you:
- ✅ Type safety
- ✅ IDE support
- ✅ Simple debugging
- ✅ Clear GraphQL API
- ✅ Flexibility for custom logic
- ✅ Minimal boilerplate
- ✅ Easy to test
- ✅ Great performance

## Conclusion

Dynamic filter generation seems like a good idea ("write less code!") but in practice creates more problems than it solves. The explicit approach with smart helpers gives you the best of both worlds: minimal boilerplate without sacrificing clarity, type safety, or flexibility.

Remember: **Explicit is better than implicit** - The Zen of Python

The small amount of "boilerplate" you write for explicit filters pays huge dividends in:
- Developer experience
- Maintainability  
- Performance
- Type safety
- Debuggability

That's why FraiseQL recommends the explicit pattern with reusable helpers rather than dynamic generation.