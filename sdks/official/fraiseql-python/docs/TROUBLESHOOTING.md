# Troubleshooting Guide

Common issues and solutions when using FraiseQL.

## Import Errors

### Error: `ModuleNotFoundError: No module named 'fraiseql'`

**Problem**: FraiseQL package is not installed.

**Solution**:

```bash
pip install fraiseql
```

Or verify installation:

```bash
python -c "import fraiseql; print(fraiseql.__version__)"
```

---

## Type Annotation Errors

### Error: `ValueError: missing type annotation for parameter 'X'`

**Problem**: Function parameter doesn't have a type annotation.

**Solution**: Add type annotations to all function parameters:

```python
# ❌ WRONG
@fraiseql.query
def users(limit) -> list[User]:
    pass

# ✅ CORRECT
@fraiseql.query
def users(limit: int = 10) -> list[User]:
    pass
```

### Error: `ValueError: missing return type annotation`

**Problem**: Function is missing return type annotation.

**Solution**: Add return type annotation:

```python
# ❌ WRONG
@fraiseql.query
def users(limit: int = 10):
    pass

# ✅ CORRECT
@fraiseql.query
def users(limit: int = 10) -> list[User]:
    pass
```

### Error: `ValueError: Union types other than | None are not supported`

**Problem**: Using unsupported union types.

**Solution**: FraiseQL only supports `T | None` for nullable types. Use concrete types:

```python
# ❌ WRONG
@fraiseql.query
def users(filter: str | int) -> list[User]:  # Union not supported
    pass

# ✅ CORRECT (Option 1: Pick one type)
@fraiseql.query
def users(id: int) -> list[User]:
    pass

# ✅ CORRECT (Option 2: Use two separate queries)
@fraiseql.query
def users_by_id(id: int) -> list[User]:
    pass

@fraiseql.query
def users_by_name(name: str) -> list[User]:
    pass
```

---

## Schema Export Issues

### Error: `FileNotFoundError: No such file or directory: 'schema.json'`

**Problem**: Schema file doesn't exist after export.

**Solution**: Make sure you called `export_schema()` and that the directory exists:

```python
# Make sure this runs
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")

# Run the script
python your_schema.py
```

### Error: Empty or invalid schema.json

**Problem**: Schema exported but has no types/queries.

**Solution**: Verify decorators were applied before export:

```python
# ❌ WRONG - Decorator after export
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")

@fraiseql.type
class User:
    id: int

# ✅ CORRECT - Decorators before export
@fraiseql.type
class User:
    id: int

if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

---

## Type Mapping Issues

### Error: `ValueError: Unsupported type: <class 'datetime.datetime'>`

**Problem**: Using unsupported Python types like `datetime`.

**Solution**: Convert to string or basic types:

```python
# ❌ WRONG
from datetime import datetime

@fraiseql.type
class Event:
    occurred_at: datetime  # Not supported

# ✅ CORRECT
@fraiseql.type
class Event:
    occurred_at: str  # ISO 8601 string
```

### Error: `ValueError: List type must have element type: list`

**Problem**: Using bare `list` without element type.

**Solution**: Always specify element type:

```python
# ❌ WRONG
@fraiseql.type
class Post:
    tags: list  # No element type

# ✅ CORRECT
@fraiseql.type
class Post:
    tags: list[str]  # Element type specified
```

---

## Decorator Issues

### Decorator Not Applied

**Problem**: Type/query/mutation not appearing in schema.

**Solution**: Ensure the decorator is called correctly:

```python
# ❌ WRONG - Forgot parentheses for mutation
@fraiseql.mutation  # Should be @fraiseql.mutation(sql_source="...")
def create_user(name: str) -> User:
    pass

# ✅ CORRECT
@fraiseql.mutation(sql_source="fn_create_user")
def create_user(name: str) -> User:
    pass
```

### Error: `@fraiseql.type` on a non-class

**Problem**: Using `@fraiseql.type` on a function.

**Solution**: `@fraiseql.type` is for classes, not functions:

```python
# ❌ WRONG
@fraiseql.type
def my_function():
    pass

# ✅ CORRECT
@fraiseql.type
class MyType:
    field: int
```

---

## Registry Issues

### Error: `ValueError: Type 'User' referenced but not registered`

**Problem**: Type is used before being defined.

**Solution**: Define types before using them in queries:

```python
# ❌ WRONG - User used before defined
@fraiseql.query
def users() -> list[User]:
    pass

@fraiseql.type
class User:
    id: int

# ✅ CORRECT
@fraiseql.type
class User:
    id: int

@fraiseql.query
def users() -> list[User]:
    pass
```

### Duplicate type/query names

**Problem**: Registering the same type/query twice.

**Solution**: Use unique names:

```python
# ❌ WRONG - Two @fraiseql.type User classes
@fraiseql.type
class User:
    id: int

@fraiseql.type
class User:  # Duplicate!
    id: int
    email: str

# ✅ CORRECT
@fraiseql.type
class UserBasic:
    id: int

@fraiseql.type
class UserExtended:
    id: int
    email: str
```

---

## SQL Configuration

### Error: Missing `sql_source` parameter

**Problem**: Query or mutation doesn't specify SQL source.

**Solution**: Add `sql_source` parameter:

```python
# ❌ WRONG
@fraiseql.query
def users() -> list[User]:
    pass

# ✅ CORRECT
@fraiseql.query(sql_source="v_users")
def users() -> list[User]:
    pass
```

### Error: Invalid `operation` parameter

**Problem**: Mutation has invalid operation type.

**Solution**: Use valid operation types:

```python
# Valid options
@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
@fraiseql.mutation(sql_source="fn_update_user", operation="UPDATE")
@fraiseql.mutation(sql_source="fn_delete_user", operation="DELETE")
@fraiseql.mutation(sql_source="fn_custom", operation="CUSTOM")
```

---

## Fact Table Issues

### Error: `ValueError: Fact table name must start with 'tf_'`

**Problem**: Fact table name doesn't follow naming convention.

**Solution**: Fact table names must start with `tf_`:

```python
# ❌ WRONG
@fraiseql.fact_table(table_name="sales")

# ✅ CORRECT
@fraiseql.fact_table(table_name="tf_sales")
```

### Error: Invalid measure column

**Problem**: Measure doesn't exist as a field.

**Solution**: Measure names must match class fields:

```python
# ❌ WRONG
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "nonexistent_column"]
)
@fraiseql.type
class Sale:
    id: int
    revenue: float
    # missing nonexistent_column

# ✅ CORRECT
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue"]  # Matches field
)
@fraiseql.type
class Sale:
    id: int
    revenue: float
```

---

## Command Line Issues

### Error: `fraiseql-cli: command not found`

**Problem**: `fraiseql-cli` is not installed or not in PATH.

**Solution**: Install the CLI separately:

```bash
pip install fraiseql-cli
```

Or use Python module:

```bash
python -m fraiseql.cli compile schema.json
```

### Error: `Invalid schema file`

**Problem**: Schema JSON is malformed.

**Solution**: Verify schema is valid JSON:

```bash
python -m json.tool schema.json
```

---

## Python Version Issues

### Error: `SyntaxError: invalid syntax` with `|` operator

**Problem**: Using Python < 3.10, which doesn't support `X | Y` syntax.

**Solution**: Upgrade Python to 3.10+:

```bash
python --version  # Check version
```

If you need to use Python 3.9, use `Union` instead:

```python
from typing import Union, Optional

# Instead of `str | None`, use:
@fraiseql.type
class User:
    name: str
    bio: Optional[str]  # Python 3.9 compatible
```

---

## Testing

### Decorators don't work in tests

**Problem**: Decorators registered in one test affect others.

**Solution**: Clear the registry between tests:

```python
import fraiseql
from fraiseql.registry import SchemaRegistry

def test_first():
    SchemaRegistry.clear()  # Start fresh

    @fraiseql.type
    class User:
        id: int

    # Test logic

def test_second():
    SchemaRegistry.clear()  # Clear from previous test

    @fraiseql.type
    class Product:
        id: int

    # Test logic
```

---

## Performance

### Schema export is slow

**Problem**: Export takes a long time.

**Solution**: Reduce number of types/queries/mutations, or ensure no circular dependencies.

### JSON file is large

**Problem**: Generated schema.json is very large.

**Solution**: This is expected with many types/queries. Compress for transfer:

```bash
gzip -9 schema.json  # Creates schema.json.gz
```

---

## Still Stuck?

Try these steps:

1. **Check the examples**: See [EXAMPLES.md](EXAMPLES.md)
2. **Read the docs**: See [GETTING_STARTED.md](GETTING_STARTED.md)
3. **Check decorators reference**: See [DECORATORS_REFERENCE.md](DECORATORS_REFERENCE.md)
4. **Run tests**: `pytest tests/ -v` to verify your installation
5. **Enable debug output**:

   ```python
   import logging
   logging.basicConfig(level=logging.DEBUG)
   ```

6. **Check GitHub issues**: <https://github.com/yourusername/fraiseql/issues>

---

## Common Patterns

### Pattern: Multi-step types

Define related types together:

```python
@fraiseql.type
class Address:
    street: str
    city: str

@fraiseql.type
class User:
    id: int
    name: str
    address: Address  # Reference other type
```

### Pattern: Optional fields

Use `| None` for nullable fields:

```python
@fraiseql.type
class User:
    id: int
    name: str
    bio: str | None  # Optional
```

### Pattern: List fields

Use `list[T]` for arrays:

```python
@fraiseql.type
class Post:
    id: int
    title: str
    tags: list[str]  # List of strings
```

### Pattern: Separate read/write

Define queries and mutations separately:

```python
@fraiseql.type
class Product:
    id: int
    name: str
    price: float

# Read
@fraiseql.query(sql_source="v_product")
def product(id: int) -> Product | None:
    pass

# Write
@fraiseql.mutation(sql_source="fn_update_product", operation="UPDATE")
def update_product(id: int, price: float) -> Product:
    pass
```

---

## FAQ

**Q: Can I use FraiseQL without GraphQL?**
A: No, FraiseQL generates GraphQL schemas. Use the Rust server to execute.

**Q: Can I use custom Python logic in decorators?**
A: No, decorators only introspect Python types. SQL logic is defined in your database.

**Q: Does FraiseQL support subscriptions?**
A: Not yet. Currently supports queries and mutations only.

**Q: Can I modify generated schema?**
A: Not directly. Regenerate from Python definitions instead.

**Q: What about validation?**
A: Validation happens at compile time (fraiseql-cli). No runtime validation in Python SDK.

**Q: Can FraiseQL work with other databases?**
A: Yes, via SQL views and functions. Supports PostgreSQL, MySQL, SQLite, SQL Server.
