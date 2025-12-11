# Temporal (Time-Related) Integration Tests

Integration tests for date, datetime, and daterange operators.

## Tests

### DateRange Tests
- `test_daterange_filtering.py` - DateRange end-to-end filtering
- `test_daterange_operations.py` - DateRange operator validation (contains_date, overlaps, etc.)

## Running Tests

```bash
# All temporal tests
uv run pytest tests/integration/database/sql/where/temporal/ -v
```

## Coverage

- DateRange operators: contains_date, overlaps, adjacent, etc.
- Date comparisons
- Timestamp handling
