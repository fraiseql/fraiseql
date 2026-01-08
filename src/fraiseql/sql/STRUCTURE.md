# SQL Module Structure

**Location**: `src/fraiseql/sql/`
**Purpose**: Query generation (WHERE, ORDER BY, SELECT)
**Stability**: Core - changes affect all queries
**Test Coverage**: 50+ SQL generation tests in `tests/unit/sql/` + `tests/integration/database/sql/`

## Overview

The `sql` module transforms GraphQL queries into efficient PostgreSQL SQL. It handles WHERE clause generation, ordering, and query optimization.

## Module Organization

### `where_generator.py`
**Responsibility**: Core WHERE clause generation logic
**Size**: Large (~8-12KB)
**Public API**:
- `WhereGenerator`: Main generator class
- `generate_where()`: Generate WHERE clause

**Key Methods**:
- `add_condition()`: Add WHERE condition
- `build()`: Build final WHERE clause
- `optimize()`: Optimize for performance

**Supports**:
- Basic operators: =, !=, <, >, <=, >=
- Array operators: IN, NOT IN, ANY, ALL
- String operators: LIKE, ILIKE, regexp
- Null checks: IS NULL, IS NOT NULL

**Depends On**: Operators, PostgreSQL knowledge
**Used By**: SQL generator, query execution

**When to Modify**:
- Adding new operators
- Changing WHERE clause format
- Optimizing performance

---

### `graphql_where_generator.py`
**Responsibility**: Convert GraphQL input types to WHERE clauses
**Size**: Medium (~5-8KB)
**Public API**:
- `GraphQLWhereGenerator`: GraphQL-to-WHERE converter
- `convert_input()`: Convert GraphQL input

**Process**:
```
GraphQL Input Type (JSON)
    ↓
Validation
    ↓
SQL WHERE Clause
    ↓
Database
```

**Depends On**: Where generator, input validation
**Used By**: Query execution

---

### `order_by_generator.py`
**Responsibility**: Generate ORDER BY clauses
**Size**: Small-Medium (~4-6KB)
**Public API**:
- `OrderByGenerator`: ORDER BY generator
- `generate_order_by()`: Generate clause

**Supports**:
- Single field ordering
- Multiple field ordering
- ASC/DESC direction
- NULLS FIRST/LAST

**Depends On**: Type system, field validation
**Used By**: Query execution

---

### `sql_generator.py`
**Responsibility**: Complete SQL query builder
**Size**: Large (~10-15KB)
**Public API**:
- `SQLGenerator`: Main SQL builder
- `generate_query()`: Build complete query

**Coordinates**:
1. SELECT clause (field projection)
2. FROM clause (table selection)
3. WHERE clause (filtering)
4. ORDER BY clause (sorting)
5. LIMIT/OFFSET (pagination)
6. JOIN clauses (relationships)

**Depends On**:
- Where generator
- Order by generator
- Field analysis
- Table schema

**Used By**: Repository, query execution

---

### `operators/` (Strategy Pattern)

Implements operator dispatch pattern. Each operator type has dedicated handler.

#### `operators/core/`
**Responsibility**: Basic comparison operators
**Files**:
- `equals.py` - = operator
- `not_equals.py` - != operator
- `greater_than.py` - > operator
- `less_than.py` - < operator
- `greater_equal.py` - >= operator
- `less_equal.py` - <= operator

**Pattern**: Each file implements `Operator` interface

---

#### `operators/postgresql/`
**Responsibility**: PostgreSQL-specific operators
**Files**:
- `jsonb.py` - JSONB operators (@>, ?, #>)
- `ltree.py` - LTree operators (<@, @>)
- `regex.py` - Regular expression (~, ~*)
- `text_search.py` - Full-text search (@@)
- `array.py` - Array operators

**Why separate**: PostgreSQL-specific syntax not portable

---

#### `operators/array/`
**Responsibility**: Array filtering operators
**Files**:
- `contains.py` - Array contains element
- `overlaps.py` - Arrays overlap
- `any.py` - ANY operator
- `all.py` - ALL operator

---

#### `operators/advanced/`
**Responsibility**: Advanced operators
**Files**:
- `network.py` - IP/network operators (<<, >>)
- `geospatial.py` - Geometric operators (<->, @)
- `range.py` - Range operators (<@, @>)
- `temporal.py` - Date/time operators

---

#### `operators/fallback/`
**Responsibility**: Fallback implementations
**Usage**: When PG-specific operator unavailable
**Files**:
- `default.py` - Default implementation

---

### `where/` (WHERE Clause Utilities)

Utilities for WHERE clause normalization and processing.

#### `where/core/`
**Files**:
- `normalizer.py` - Normalize WHERE clauses
- `validator.py` - Validate WHERE syntax
- `optimizer.py` - Optimize WHERE execution

#### `where/operators/`
**Files**:
- `operator_dispatcher.py` - Route to correct operator
- `operator_registry.py` - Operator lookup

---

## Operator Strategy Pattern

### Adding a New Operator

**Step 1**: Understand operator category
- Basic comparison → `operators/core/`
- PostgreSQL specific → `operators/postgresql/`
- Array operation → `operators/array/`
- Advanced → `operators/advanced/`

**Step 2**: Create operator file

```python
"""Greater than operator."""

from fraiseql.sql.operators.base import BaseOperator

class GreaterThanOperator(BaseOperator):
    """PostgreSQL > operator."""

    NAME = "gt"  # GraphQL name
    SQL_OPERATOR = ">"

    def generate_clause(self, field: str, value: Any) -> str:
        """Generate SQL clause."""
        return f"{field} > %s", [value]
```

**Step 3**: Register operator

```python
# In operators/__init__.py or dispatcher
OPERATORS = {
    "gt": GreaterThanOperator,
    ...
}
```

**Step 4**: Write tests

```python
# tests/unit/sql/test_operators.py
def test_greater_than_operator():
    op = GreaterThanOperator()
    clause, params = op.generate_clause("age", 18)
    assert clause == "age > %s"
    assert params == [18]
```

**Step 5**: Document operator

```python
"""Greater than operator.

Generates SQL > condition.

Example:
    age: {gt: 18}  # age > 18

Supported types:
    - Integer
    - Float
    - Date
"""
```

---

## Dependencies

### Internal Dependencies
```
sql_generator.py
├── where_generator.py
├── order_by_generator.py
├── graphql_where_generator.py
├── operators/
└── field analysis

where_generator.py
├── operators/[all types]
└── validation

operators/
├── core/ (basic comparisons)
├── postgresql/ (PG specific)
├── array/ (array operations)
├── advanced/ (advanced types)
└── fallback/ (defaults)
```

### External Dependencies
- `psycopg`: PostgreSQL driver
- `graphql-core`: GraphQL parsing

---

## Guidelines for SQL Module

1. **Parameterized queries**: Always use parameters, never string concatenation
2. **PostgreSQL dialect**: Target PostgreSQL 13+
3. **Performance**: Avoid N+1 queries
4. **Testing**: Test with real database
5. **Validation**: Validate all inputs

### Example: Safe Query Building

```python
# ✅ CORRECT - Parameterized
def build_where(field: str, value: str) -> tuple[str, list]:
    return f"{field} = %s", [value]

# ❌ WRONG - String concatenation
def build_where(field: str, value: str) -> str:
    return f"{field} = '{value}'"  # SQL injection!
```

---

## Common Questions

**Q: How do I add support for a new operator?**
A: Follow "Adding a New Operator" above. Choose appropriate category, implement interface, register, test.

**Q: Why is the module structured with separate generators?**
A: Clear separation of concerns - WHERE, ORDER BY, SELECT are distinct responsibilities.

**Q: How do I optimize a slow query?**
A: Check `sql_generator.py` output, verify indexes exist, consider field projection optimization.

**Q: Can I use custom SQL in queries?**
A: Not recommended for security. Instead, use proper operators and let SQL generator create safe queries.

---

## Performance Optimization

### Index Recommendations

For common operations, suggest indexes:
- WHERE clauses: Index on filtered fields
- ORDER BY: Index on sort fields
- JOINs: Index on foreign keys

### Query Optimization Tips

1. **Field projection**: Only select needed fields
2. **Pagination**: Always paginate large result sets
3. **WHERE optimization**: Filter early
4. **Index usage**: Verify EXPLAIN ANALYZE

---

## Testing SQL Module

### Unit Tests
```bash
# Test SQL generation (no database)
pytest tests/unit/sql/
```

### Integration Tests
```bash
# Test with real database
pytest tests/integration/database/sql/
```

### Regression Tests
```bash
# Test specific fixes
pytest tests/regression/
```

---

## Refactoring Roadmap

### v2.0 (Current)
- ✅ Document structure
- ✅ Establish operator pattern
- ✅ Define guidelines

### v2.1+
- 📋 Consider operator consolidation
- 📋 Evaluate query optimizer
- 📋 Performance profiling

---

## See Also

- **Main documentation**: `docs/ORGANIZATION.md`
- **Related tests**: `tests/unit/sql/`, `tests/integration/database/sql/`
- **WHERE normalization**: `src/fraiseql/where_normalization.py`
- **Database module**: `src/fraiseql/db.py`
- **CQRS**: `src/fraiseql/cqrs/`

---

**Last Updated**: January 8, 2026
**Stability**: Core
**Test Coverage**: 50+ unit tests, 25+ integration tests
