# Quick Reference - WHERE Integration Tests

## Common Commands

### Run All WHERE Tests
```bash
uv run pytest tests/integration/database/sql/where/ -v
```

### Run By Category
```bash
# Network tests
uv run pytest tests/integration/database/sql/where/network/ -v

# LTree tests
uv run pytest tests/integration/database/sql/where/specialized/ -v

# DateRange tests
uv run pytest tests/integration/database/sql/where/temporal/ -v

# Coordinate tests
uv run pytest tests/integration/database/sql/where/spatial/ -v
```

### Run Specific Test
```bash
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py -v
```

### Run With Pattern
```bash
# All IP-related tests
uv run pytest tests/integration/database/sql/where/ -k "ip" -v

# All MAC-related tests
uv run pytest tests/integration/database/sql/where/ -k "mac" -v

# All LTree tests
uv run pytest tests/integration/database/sql/where/ -k "ltree" -v
```

### Debug Single Test
```bash
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py::test_function_name -vvs
```

### Run Tests with Coverage
```bash
uv run pytest tests/integration/database/sql/where/ --cov=fraiseql.sql --cov-report=html
```

## Test Categories

| Category | Path | Count | Purpose |
|----------|------|-------|---------|
| **Network** | `network/` | 8 files | IP, MAC, hostname, email, port |
| **Specialized** | `specialized/` | 2 files | LTree, fulltext (PostgreSQL types) |
| **Temporal** | `temporal/` | 2 files | Date, datetime, daterange |
| **Spatial** | `spatial/` | 1 file | Coordinates, distance |
| **Mixed** | `<root>` | 2-4 files | Cross-cutting tests |

## Adding New Tests

### Step 1: Choose Category
- Network operators → `network/`
- PostgreSQL types → `specialized/`
- Time-related → `temporal/`
- Coordinates → `spatial/`
- Multi-type → root

### Step 2: Name Test File
- End-to-end: `test_<type>_filtering.py`
- Operations: `test_<type>_operations.py`
- Bugs: `test_<type>_bugs.py`

### Step 3: Run Tests
```bash
uv run pytest <your-new-test-file> -v
```

## Troubleshooting

### Tests Not Found
```bash
# Verify pytest can discover tests
uv run pytest tests/integration/database/sql/where/ --collect-only

# Check __init__.py files
find tests/integration/database/sql/where -name "__init__.py"
```

### Import Errors
- Verify `__init__.py` in all directories
- Check fixtures in parent `conftest.py`
- Ensure running from project root

### Fixture Not Found
- Fixtures in `tests/integration/database/conftest.py`
- pytest auto-discovers from parent directories

## Need Help?
- Full docs: `README.md`
- Migration details: `migration-history.md`
- Contributing: `../../../../../../CONTRIBUTING.md`
