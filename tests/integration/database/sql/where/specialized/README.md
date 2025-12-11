# PostgreSQL Specialized Type Integration Tests

Integration tests for PostgreSQL-specific types like ltree and fulltext search.

## Tests

### LTree Tests
- `test_ltree_filtering.py` - LTree hierarchical path filtering
- `test_ltree_operations.py` - LTree operator validation (ancestor_of, matches_lquery, etc.)

## Running Tests

```bash
# All specialized tests
uv run pytest tests/integration/database/sql/where/specialized/ -v

# LTree only
uv run pytest tests/integration/database/sql/where/specialized/test_ltree_filtering.py -v
```

## Coverage

- LTree hierarchical operators
- Pattern matching (lquery, ltxtquery)
- Path manipulation operations
- Array operations
