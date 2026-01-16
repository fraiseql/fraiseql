# FraiseQL E2E Testing Guide

## Overview

End-to-end (E2E) testing for FraiseQL validates the complete pipeline from schema authoring through compilation to runtime execution. This guide covers the E2E testing infrastructure, test execution, and troubleshooting.

## Architecture

```
┌────────────────────────────────────────────────────┐
│             E2E Test Orchestrator                   │
│                  (Makefile)                         │
└─────────────────┬──────────────────────────────────┘
                  │
    ┌─────────────┼─────────────┬─────────────┐
    │             │             │             │
┌───▼──┐  ┌──────▼────┐  ┌────▼────┐  ┌────▼────┐
│Python│  │TypeScript │  │   Go    │  │  Java   │
│ venv │  │   npm     │  │  go mod │  │  mvn    │
└───┬──┘  └──────┬────┘  └────┬────┘  └────┬────┘
    │           │             │             │
    └───────────┼─────────────┼─────────────┘
                │
        ┌───────▼────────┐
        │  Docker        │
        │  Databases     │
        │  (test)        │
        └────────────────┘
```

## Quick Start

### Run All E2E Tests

```bash
make e2e-all
```

This runs sequential tests for Python, TypeScript, and Go.

### Run Specific Language Test

```bash
make e2e-python     # Python tests
make e2e-typescript # TypeScript tests
make e2e-go        # Go tests
```

### Check Infrastructure Status

```bash
make e2e-status
```

Output shows which languages are available:
```
Languages ready:
  ✅ Python
  ✅ TypeScript/Node
  ✅ Go
  ❌ Java
  ❌ PHP
```

### Setup E2E Infrastructure

```bash
make e2e-setup
```

This starts Docker containers for:
- PostgreSQL 16
- MySQL 8.3
- SQLite (local)

### Clean Up

```bash
make e2e-clean
```

Stops containers and removes temporary files.

## Test Pipeline

### Phase 1: Schema Authoring

Each language generator creates a schema:

**Python**:
```python
@fraiseql_type
class User:
    id: int
    name: str

@fraiseql_query(sql_source="v_users")
def users() -> list[User]:
    pass

fraiseql_schema.export_schema("schema.json")
```

**TypeScript**:
```typescript
@Type()
class User {
  id!: number;
  name!: string;
}

@Query(sql_source = "v_users")
users(): User[] { return []; }

ExportSchema("schema.json");
```

**Go**:
```go
type User struct {
    ID   int    `fraiseql:"id"`
    Name string `fraiseql:"name"`
}

fraiseql.ExportSchema("schema.json")
```

### Phase 2: JSON Validation

Verify schema JSON structure:
- ✅ Valid JSON syntax
- ✅ Required fields present
- ✅ Type references valid
- ✅ No circular dependencies

### Phase 3: CLI Compilation

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

Produces optimized execution plan with:
- SQL templates
- Type validation
- Query optimization
- Database integration metadata

### Phase 4: Runtime Execution

Start server with compiled schema:

```bash
fraiseql-server --schema schema.compiled.json --port 4000
```

## Test Structure

### Python E2E Test

Location: `tests/e2e/python_e2e_test.py`

```python
def test_python_e2e_basic_schema():
    """Test basic schema authoring and export."""
    # Step 1: Define schema
    @fraiseql_type
    class User:
        id: int
        name: str

    # Step 2: Export to JSON
    fraiseql_schema.export_schema("schema.json")

    # Step 3: Verify JSON structure
    with open("schema.json") as f:
        schema = json.load(f)
    assert "types" in schema
    assert schema["types"][0]["name"] == "User"

    # Step 4: Try CLI compilation
    result = subprocess.run(
        ["fraiseql-cli", "compile", "schema.json"],
        capture_output=True
    )
    assert result.returncode == 0
```

### Running Directly

```bash
cd fraiseql-python
source .venv/bin/activate
python tests/e2e/python_e2e_test.py
```

## Makefile Targets

### e2e-setup

Starts Docker test databases:
- PostgreSQL 16
- MySQL 8.3
- SQLite

```bash
make e2e-setup
```

### e2e-all

Runs sequential tests for all languages:

```bash
make e2e-all
```

Equivalent to: `e2e-python → e2e-typescript → e2e-go`

### e2e-python, e2e-typescript, e2e-go, e2e-java, e2e-php

Runs E2E test for specific language:

```bash
make e2e-python
```

Output:
```
========== PYTHON E2E TEST ==========
✅ Python environment ready

Running E2E tests...
✅ test_python_e2e_basic_schema passed
✅ test_python_e2e_analytics_schema passed

✅ Python E2E tests passed
```

### e2e-clean

Stops Docker containers and removes temp files:

```bash
make e2e-clean
```

### e2e-status

Checks infrastructure readiness:

```bash
make e2e-status
```

Output shows available languages and Docker status.

## Continuous Integration

### GitHub Actions Workflow

The project includes automated E2E testing in `.github/workflows/e2e-tests.yml`:

```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Set up test environment
        run: make e2e-setup
      - name: Run E2E tests
        run: make e2e-all
      - name: Cleanup
        run: make e2e-clean
```

## Test Coverage

| Language | Test Count | Coverage |
|----------|-----------|----------|
| Python | 34+ | Basic schema, analytics |
| TypeScript | 10+ | Schema registry, decorators |
| Go | 45+ | Type reflection, schema export |
| Java | Pending | Annotations, schema generation |
| PHP | Pending | Attributes, schema export |

## Troubleshooting

### Docker Not Available

If Docker isn't installed:

```bash
# Tests still run, but without database
make e2e-python
```

CLI compilation is tested regardless of Docker availability.

### Python Environment Not Activated

```
Error: No module named 'fraiseql'
```

Solution:
```bash
cd fraiseql-python
source .venv/bin/activate
cd ..
make e2e-python
```

### TypeScript Tests Failing

```
Error: Cannot find module 'jest'
```

Solution:
```bash
cd fraiseql-typescript
npm install
cd ..
make e2e-typescript
```

### CLI Not Found

```
fraiseql-cli: command not found
```

Solution:
```bash
cargo build --release -p fraiseql-cli
export PATH="$(pwd)/target/release:$PATH"
make e2e-all
```

### Schema Compilation Warnings

```
⚠️  Warnings (2):
   Query 'posts' returns a list but has no sql_source
```

This is expected during development. Warnings don't block compilation. Use `sql_source` for production queries.

## Test Scenarios

### Scenario 1: Basic Schema

Tests:
- Type definition
- Field mapping
- Query definition
- JSON export
- CLI compilation

### Scenario 2: Analytics Schema

Tests:
- Fact table definition
- Measure specification
- Dimension configuration
- Aggregate query definition
- JSON export
- CLI compilation

### Scenario 3: Complex Schema

Tests:
- Multiple types with relationships
- Nested queries
- Mutations
- Error handling
- SQL source validation

## Performance Metrics

Typical test execution times:

| Language | Setup | Test | Total |
|----------|-------|------|-------|
| Python | 5s | 2s | 7s |
| TypeScript | 3s | 1s | 4s |
| Go | 2s | 1s | 3s |
| **Total** | - | - | **~15s** |

All tests run sequentially, so total time is the sum of individual times.

## Debugging Tests

### Verbose Output

```bash
make e2e-python  # Already shows detailed output
```

### Step-by-Step Execution

```bash
# Python
cd fraiseql-python
source .venv/bin/activate
python tests/e2e/python_e2e_test.py

# TypeScript
cd fraiseql-typescript
npm test

# Go
cd fraiseql-go
go test ./fraiseql/... -v
```

### Check Generated Schemas

```bash
# Python generates to current directory
python -c "from fraiseql import schema; schema.export_schema('/tmp/py_test.json')"
ls -la /tmp/py_test.json
cat /tmp/py_test.json | python -m json.tool
```

### Inspect CLI Output

```bash
fraiseql-cli compile /tmp/py_test.json --verbose
```

## Best Practices

### 1. Run Tests Before Committing

```bash
make e2e-all && git commit
```

### 2. Isolate Test Output

Tests create temporary files in `/tmp/`. Clean up after:

```bash
make e2e-clean
```

### 3. Test in Isolation

Run individual language tests to isolate issues:

```bash
make e2e-python  # If this fails, focus on Python
make e2e-typescript  # If this fails, focus on TypeScript
```

### 4. Verify CLI First

Before running full E2E tests:

```bash
cargo build --release -p fraiseql-cli
export PATH="$(pwd)/target/release:$PATH"
fraiseql-cli --version
```

### 5. Check Infrastructure Status

Before running tests:

```bash
make e2e-status
```

## Integration with CI/CD

### Local Development

```bash
# Before committing
make e2e-all
git add .
git commit -m "feat: Add new feature"
```

### GitHub Actions

Tests run automatically on:
- Push to any branch
- Pull requests
- Scheduled nightly builds (optional)

## See Also

- [Language Generators Guide](./language-generators.md)
- [CLI Schema Format Guide](./cli-schema-format.md)
- [Makefile E2E Targets](../Makefile)
- [E2E Strategy Document](../.claude/E2E_TESTING_STRATEGY.md)
