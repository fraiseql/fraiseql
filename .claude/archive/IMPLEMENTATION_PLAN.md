# FraiseQL Language Generators - Complete Implementation Plan

**Plan Date**: January 16, 2026
**Target Completion**: January 19-20, 2026 (3-4 days)
**Total Effort**: 16-18 hours
**Current Status**: All 5 generators 55-100% complete, E2E testing ready to implement

---

## Overview

This plan details **exactly what to do, in what order, with commands ready to execute**. It's designed for a single person working sequentially, though parallel work is possible.

---

## PHASE 1: QUICK FIXES (Day 1 - 5-6 hours)

### Objective

Get all 5 languages with passing tests + understand CLI issue

### Task 1.1: Python - Install Package (5 minutes)

**Current Issue**: ModuleNotFoundError on import

**Commands**:

```bash
# Navigate to project
cd /home/lionel/code/fraiseql

# Install in editable mode
pip install -e fraiseql-python/

# Verify installation
python -c "import fraiseql; print('✅ fraiseql imported successfully')"
```

**Verify Tests**:

```bash
cd fraiseql-python
python -m pytest tests/ -v

# Expected output:
# test_types.py::test_int_conversion PASSED
# test_types.py::test_list_conversion PASSED
# test_decorators.py::test_type_decorator PASSED
# ... (7 total)
# ======================== 7 passed in 0.23s ========================
```

**Success Criteria**:

- ✅ All 7 tests passing
- ✅ No ModuleNotFoundError
- ✅ Import works: `from fraiseql import type as fraiseql_type`

---

### Task 1.2: TypeScript - Fix Decorator Configuration (15 minutes)

**Current Issue**: Decorator syntax not recognized in examples

**Step 1: Edit tsconfig.json**

```bash
cd /home/lionel/code/fraiseql/fraiseql-typescript
```

**Step 2: Add flags to compilerOptions**

Open `fraiseql-typescript/tsconfig.json` and verify these lines exist in `compilerOptions`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
    "moduleResolution": "node",
    "strict": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  }
}
```

**Key additions**:

- `"experimentalDecorators": true` ← ADD THIS
- `"emitDecoratorMetadata": true` ← ADD THIS

**Step 3: Update npm build script**

Open `fraiseql-typescript/package.json` and ensure scripts section has:

```json
{
  "scripts": {
    "build": "tsc",
    "test": "jest",
    "test:e2e": "jest --testPathPattern=e2e",
    "example:basic": "tsx --experimental-decorators examples/basic_schema.ts",
    "example:analytics": "tsx --experimental-decorators examples/analytics_schema.ts"
  }
}
```

**Verify Tests**:

```bash
cd fraiseql-typescript
npm test

# Expected output:
# PASS  tests/registry.test.ts
#   ✓ should register type (2 ms)
#   ✓ should retrieve type (1 ms)
#   ... (10 total)
# Test Suites: 1 passed, 1 total
# Tests: 10 passed, 10 total
```

**Verify Examples**:

```bash
npm run example:basic
# Should output JSON schema to stdout

npm run example:analytics
# Should output analytics schema JSON to stdout
```

**Success Criteria**:

- ✅ 10/10 tests passing
- ✅ `npm run example:basic` executes without errors
- ✅ `npm run example:analytics` executes without errors
- ✅ Both examples output valid JSON

---

### Task 1.3: Java - Install Maven (10 minutes)

**Current Issue**: Maven not installed

**Step 1: Check if Maven exists**

```bash
which mvn
mvn --version
```

**If NOT installed** (most likely):

```bash
# On Arch Linux
sudo pacman -S maven

# On Ubuntu/Debian
sudo apt-get install maven

# On macOS
brew install maven

# Verify installation
mvn --version
```

**Expected output**:

```
Apache Maven 3.x.x
Maven home: /usr/share/java/maven
Java version: 17.x.x
```

**Step 2: Verify Tests Can Run**

```bash
cd /home/lionel/code/fraiseql/fraiseql-java

# Download dependencies (takes a few minutes)
mvn dependency:download-sources -q

# Run tests
mvn test

# Expected output:
# [INFO] Tests run: 82, Failures: 0, Errors: 0, Skipped: 0
# [INFO] BUILD SUCCESS
```

**Success Criteria**:

- ✅ Maven installed and in PATH
- ✅ `mvn --version` works
- ✅ Tests executable (82/82 tests)

---

### Task 1.4: PHP - Install Composer & Dependencies (5 minutes)

**Current Issue**: Composer dependencies not installed

**Step 1: Check if Composer exists**

```bash
which composer
composer --version
```

**If NOT installed**:

```bash
# On Arch Linux
sudo pacman -S composer

# On Ubuntu/Debian
sudo apt-get install composer

# On macOS
brew install composer

# Verify installation
composer --version
```

**Step 2: Install Dependencies**

```bash
cd /home/lionel/code/fraiseql/fraiseql-php

composer install

# Expected output shows packages installed
# Loading composer repositories with package information
# Installing dependencies (including require-dev) from lock file
# Package operations: X installs, 0 updates, 0 removals
```

**Step 3: Verify Tests Can Run**

```bash
vendor/bin/phpunit tests/ -v

# Expected output:
# PHPUnit 11.0.4 by Sebastian Bergmann
# Tests: 40+, Assertions: 100+, Time: 0.23s, Memory: 10MB
# OK (40 tests, 40 assertions)
```

**Success Criteria**:

- ✅ Composer installed
- ✅ Dependencies in vendor/ directory
- ✅ Tests executable

---

### Task 1.5: Go - Verify Still Working (5 minutes)

**Current Issue**: None (should already be working)

**Verify Tests**:

```bash
cd /home/lionel/code/fraiseql/fraiseql-go

go test ./fraiseql/... -v

# Expected output:
# === RUN   TestTypeConversion
# --- PASS: TestTypeConversion (0.00s)
# ...
# ok      fraiseql/fraiseql       0.234s
# PASS  45/45 tests
```

**Verify Examples**:

```bash
# Test basic schema generation
go run examples/basic_schema.go > /tmp/go_schema.json
cat /tmp/go_schema.json | jq . | head -20

# Should output JSON with "types", "queries", "mutations"
```

**Success Criteria**:

- ✅ 45/45 tests passing
- ✅ Examples generate valid JSON
- ✅ No regressions

---

### Task 1.6: CLI Investigation (2-4 hours)

**Current Issue**: fraiseql-cli rejects generated schemas

**Step 1: Generate Test Schemas**

```bash
# Python schema
pip install -e fraiseql-python/ 2>/dev/null
cd fraiseql-python
python -c "
import fraiseql
from fraiseql import type as fraiseql_type, query as fraiseql_query, schema as fraiseql_schema

@fraiseql_type
class User:
    id: int
    name: str

@fraiseql_query(sql_source='v_user')
def users(limit: int = 10) -> list[User]:
    pass

fraiseql_schema.export_schema('/tmp/python_schema.json')
print('✅ Python schema generated')
" 2>/dev/null || echo "⚠️  Python schema generation failed"

# Go schema
cd /home/lionel/code/fraiseql/fraiseql-go
go run examples/basic_schema.go > /tmp/go_schema.json
echo "✅ Go schema generated"

# TypeScript schema
cd /home/lionel/code/fraiseql/fraiseql-typescript
npm run example:basic > /tmp/typescript_schema.json 2>/dev/null
echo "✅ TypeScript schema generated"

# Java schema (via test)
cd /home/lionel/code/fraiseql/fraiseql-java
mvn test -Dtest="*SchemaTest" -q 2>/dev/null || echo "⚠️  Java schema generation needs verification"

# PHP schema (via test)
cd /home/lionel/code/fraiseql/fraiseql-php
vendor/bin/phpunit tests/ --filter "Export" -q 2>/dev/null || echo "⚠️  PHP schema generation needs verification"
```

**Step 2: Examine Generated Schema Format**

```bash
# Check schema structure
echo "=== Python Schema ==="
cat /tmp/python_schema.json | jq 'keys' 2>/dev/null || cat /tmp/python_schema.json | head -20

echo "=== Go Schema ==="
cat /tmp/go_schema.json | jq 'keys' 2>/dev/null || cat /tmp/go_schema.json | head -20

echo "=== TypeScript Schema ==="
cat /tmp/typescript_schema.json | jq 'keys' 2>/dev/null || cat /tmp/typescript_schema.json | head -20
```

**Step 3: Review fraiseql-cli Schema Parser**

```bash
# Find and examine CLI schema parser
cd /home/lionel/code/fraiseql

# Search for schema validation/parsing code
find . -name "*.rs" -type f | xargs grep -l "schema\|compile" | grep -E "(cli|compiler)" | head -10

# Look at main CLI entry point
cat crates/fraiseql-cli/src/main.rs | head -50

# Look at compile command
cat crates/fraiseql-cli/src/commands/compile.rs 2>/dev/null | head -100 || \
find . -name "compile.rs" | grep cli

# Look at schema validation
grep -n "fn.*schema\|fn.*validate" crates/fraiseql-cli/src/*.rs 2>/dev/null | head -20
```

**Step 4: Try CLI Compilation**

```bash
# Try with Go schema (most likely to work)
fraiseql-cli compile /tmp/go_schema.json

# Capture full error output
fraiseql-cli compile /tmp/go_schema.json 2>&1 | tee /tmp/cli_error.log

# Check what format fraiseql-cli expects
fraiseql-cli compile --help
fraiseql-cli validate --help  # If validate command exists
```

**Step 5: Document Findings**

Create `/tmp/cli_investigation.md`:

```markdown
# CLI Schema Format Investigation

## Generated Schema Format (from generators)
```

[Paste actual schema structure here]

```

## fraiseql-cli Error Message
```

[Paste exact error here]

```

## Key Differences Found
- [Difference 1]
- [Difference 2]
- [Difference 3]

## Proposed Fix
Option A: Fix generators to match CLI expectations
Option B: Fix CLI to accept generator output
Option C: Create transformer layer

## Recommendation
[Your analysis here]
```

**Success Criteria**:

- ✅ Generated 4+ test schemas
- ✅ Examined schema structures
- ✅ Reviewed CLI parser code
- ✅ Documented findings
- ✅ Identified fix strategy

---

### Phase 1 Summary Check

Before moving to Phase 2, verify:

```bash
# Python tests
cd /home/lionel/code/fraiseql/fraiseql-python && python -m pytest tests/ -q

# TypeScript tests and examples
cd /home/lionel/code/fraiseql/fraiseql-typescript && npm test && npm run example:basic > /dev/null

# Java tests
cd /home/lionel/code/fraiseql/fraiseql-java && mvn test -q 2>&1 | grep -E "BUILD|Tests"

# PHP tests
cd /home/lionel/code/fraiseql/fraiseql-php && vendor/bin/phpunit tests/ -q 2>&1 | tail -1

# Go tests
cd /home/lionel/code/fraiseql/fraiseql-go && go test ./fraiseql/... -q

# CLI investigation documented
[ -f /tmp/cli_investigation.md ] && echo "✅ CLI investigation documented" || echo "❌ Missing investigation"
```

**Expected Output**:

```
✅ Python: 7 passed
✅ TypeScript: 10 passed + examples work
✅ Java: 82 passed
✅ PHP: 40+ passed
✅ Go: 45 passed
✅ CLI investigation documented
```

---

## PHASE 2: E2E TESTING INFRASTRUCTURE (Days 2-3 - 8-9 hours)

### Objective

Create complete E2E testing infrastructure with Makefile and GitHub Actions

### Task 2.1: Create E2E Test Files (4 hours)

All test code is provided in `E2E_TESTING_STRATEGY.md`. Copy-paste and create:

**Python E2E Test** (`tests/e2e/python_e2e_test.py`)

- Location: `/home/lionel/code/fraiseql/tests/e2e/python_e2e_test.py`
- Copy code from: E2E_TESTING_STRATEGY.md → "Python E2E Test" section
- Contains: 2 test functions (basic schema, analytics)
- Run: `pytest tests/e2e/python_e2e_test.py -v`

**TypeScript E2E Test** (`fraiseql-typescript/tests/e2e/e2e.test.ts`)

- Location: `/home/lionel/code/fraiseql/fraiseql-typescript/tests/e2e/e2e.test.ts`
- Copy code from: E2E_TESTING_STRATEGY.md → "TypeScript E2E Test" section
- Contains: 3 test functions
- Run: `npm run test:e2e` (from fraiseql-typescript/)

**Java E2E Test** (`fraiseql-java/src/test/java/com/fraiseql/E2ETest.java`)

- Location: `/home/lionel/code/fraiseql/fraiseql-java/src/test/java/com/fraiseql/E2ETest.java`
- Copy code from: E2E_TESTING_STRATEGY.md → "Java E2E Test" section
- Contains: 2 test methods
- Run: `mvn test -Dtest="E2ETest"`

**Go E2E Test** (`fraiseql-go/fraiseql/e2e_test.go`)

- Location: `/home/lionel/code/fraiseql/fraiseql-go/fraiseql/e2e_test.go`
- Copy code from: E2E_TESTING_STRATEGY.md → "Go E2E Test" section
- Contains: 2 test functions
- Run: `go test ./fraiseql/... -run TestE2E -v`

**PHP E2E Test** (`fraiseql-php/tests/e2e/E2ETest.php`)

- Location: `/home/lionel/code/fraiseql/fraiseql-php/tests/e2e/E2ETest.php`
- Copy code from: E2E_TESTING_STRATEGY.md → "PHP E2E Test" section
- Contains: 3 test methods
- Run: `vendor/bin/phpunit tests/e2e/`

**Creation Steps**:

```bash
# Create directories
mkdir -p /home/lionel/code/fraiseql/tests/e2e
mkdir -p /home/lionel/code/fraiseql/fraiseql-typescript/tests/e2e
mkdir -p /home/lionel/code/fraiseql/fraiseql-java/src/test/java/com/fraiseql
mkdir -p /home/lionel/code/fraiseql/fraiseql-go/fraiseql/e2e_tests
mkdir -p /home/lionel/code/fraiseql/fraiseql-php/tests/e2e

# For each test, create the file with content from E2E_TESTING_STRATEGY.md
# Use your editor or the Write tool

echo "✅ All test files created"
```

**Success Criteria**:

- ✅ All 5 test files created
- ✅ Each file contains complete test code
- ✅ Tests are syntactically valid (no compile errors)
- ✅ Tests are discoverable by test runners

---

### Task 2.2: Implement Makefile E2E Targets (2 hours)

**Location**: Update `/home/lionel/code/fraiseql/Makefile`

**Add to Makefile** (copy from E2E_TESTING_STRATEGY.md → "Implementation: Makefile Test Targets" section):

The code defines:

- `e2e-setup` - Start Docker containers
- `e2e-all` - Run all 5 languages
- `e2e-python`, `e2e-typescript`, `e2e-java`, `e2e-go`, `e2e-php` - Individual language tests
- `e2e-clean` - Stop Docker and cleanup
- `e2e-status` - Check infrastructure status

**Quick setup**:

```bash
cd /home/lionel/code/fraiseql

# Append to existing Makefile
cat >> Makefile << 'EOF'

# ============================================================================
# E2E Testing - All Languages
# ============================================================================

.PHONY: e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-clean e2e-status

## Setup: Start Docker databases
e2e-setup:
 @echo "🔧 Starting E2E test infrastructure..."
 docker compose -f docker-compose.test.yml up -d
 @echo "Waiting for databases..."
 sleep 5
 docker compose -f docker-compose.test.yml ps
 @echo "✅ E2E infrastructure ready"

## Run all E2E tests
e2e-all: e2e-setup e2e-python e2e-typescript e2e-java e2e-go e2e-php
 @echo "✅ All E2E tests completed!"

## E2E: Python
e2e-python:
 @echo "========== PYTHON E2E =========="
 python -m pip install -q -e fraiseql-python/
 cd fraiseql-python && python -m pytest tests/e2e/ -v
 @echo "✅ Python E2E passed"

## E2E: TypeScript
e2e-typescript:
 @echo "========== TYPESCRIPT E2E =========="
 cd fraiseql-typescript && npm ci -q && npm run test:e2e
 @echo "✅ TypeScript E2E passed"

## E2E: Java
e2e-java:
 @echo "========== JAVA E2E =========="
 cd fraiseql-java && mvn test -Dtest="*E2ETest" -q
 @echo "✅ Java E2E passed"

## E2E: Go
e2e-go:
 @echo "========== GO E2E =========="
 cd fraiseql-go && go test ./fraiseql/... -run TestE2E -v
 @echo "✅ Go E2E passed"

## E2E: PHP
e2e-php:
 @echo "========== PHP E2E =========="
 cd fraiseql-php && composer install -q
 vendor/bin/phpunit tests/e2e/ -v
 @echo "✅ PHP E2E passed"

## Cleanup: Stop Docker
e2e-clean:
 @echo "🧹 Cleaning up..."
 docker compose -f docker-compose.test.yml down -v
 @echo "✅ Cleanup complete"

## Status: Check E2E infrastructure
e2e-status:
 @echo "Docker Compose Status:"
 docker compose -f docker-compose.test.yml ps

EOF

echo "✅ Makefile updated with E2E targets"
```

**Test Makefile targets**:

```bash
cd /home/lionel/code/fraiseql

# Test setup
make e2e-setup
make e2e-status

# Test individual languages (one at a time, quick)
make e2e-go      # Go is fastest

# Cleanup
make e2e-clean
```

**Success Criteria**:

- ✅ `make e2e-setup` starts Docker containers
- ✅ `make e2e-status` shows running containers
- ✅ `make e2e-go` runs Go E2E tests
- ✅ `make e2e-clean` stops containers

---

### Task 2.3: Set Up GitHub Actions (3 hours)

**Location**: Create `/home/lionel/code/fraiseql/.github/workflows/e2e-tests.yml`

**Steps**:

1. Create directory: `mkdir -p .github/workflows`
2. Copy workflow from E2E_TESTING_STRATEGY.md → "GitHub Actions CI/CD Pipeline" section
3. Save as `.github/workflows/e2e-tests.yml`

**File contains**:

- Python job with pip caching
- TypeScript job with npm caching
- Java job with Maven caching
- Go job with module caching
- PHP job with Composer caching
- PostgreSQL and MySQL services
- CLI integration test job
- Summary report job

**Quick setup**:

```bash
cd /home/lionel/code/fraiseql

# Create the workflow file (copy content from E2E_TESTING_STRATEGY.md)
mkdir -p .github/workflows

# Use your editor or the Write tool to create:
# .github/workflows/e2e-tests.yml
# [paste full content from E2E_TESTING_STRATEGY.md]

# Verify syntax
cat .github/workflows/e2e-tests.yml | head -20
```

**Test locally** (optional, requires act):

```bash
# Install act to test workflows locally (optional)
# https://github.com/nektos/act
act -j test-go  # Test Go job only
```

**Verify on GitHub**:

```bash
# Commit the workflow
cd /home/lionel/code/fraiseql
git add .github/workflows/e2e-tests.yml
git commit -m "ci: Add E2E testing pipeline for all 5 languages"
git push

# Watch workflow on GitHub:
# https://github.com/yourusername/fraiseql/actions
```

**Success Criteria**:

- ✅ Workflow file created and valid YAML
- ✅ Pushed to GitHub
- ✅ Workflow appears in GitHub Actions
- ✅ All jobs trigger on push/PR

---

### Phase 2 Summary Check

Before moving to Phase 3:

```bash
# Verify test files exist
[ -f tests/e2e/python_e2e_test.py ] && echo "✅ Python E2E test" || echo "❌ Missing"
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "✅ TypeScript E2E test" || echo "❌ Missing"
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "✅ Java E2E test" || echo "❌ Missing"
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "✅ Go E2E test" || echo "❌ Missing"
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "✅ PHP E2E test" || echo "❌ Missing"

# Verify Makefile
grep -q "e2e-all" Makefile && echo "✅ Makefile E2E targets" || echo "❌ Missing"

# Verify GitHub Actions
[ -f .github/workflows/e2e-tests.yml ] && echo "✅ GitHub Actions workflow" || echo "❌ Missing"

# Test one E2E target locally
make e2e-go 2>&1 | tail -5
```

---

## PHASE 3: CLI INTEGRATION FIX (Day 4 - 1-2 hours)

### Objective

Resolve CLI schema format issue so all 5 languages compile

### Task 3.1: Analyze CLI Schema Parser (1 hour)

Based on findings from Phase 1, Task 1.6:

**Review generated schema structure**:

```bash
# Look at what was generated
cat /tmp/go_schema.json | jq 'keys' 2>/dev/null
cat /tmp/python_schema.json | jq 'keys' 2>/dev/null

# Compare with CLI expectations
# (review code from Phase 1.6 investigation)
```

**Identify missing/extra fields**:

```bash
# Example: Check if all generators have these top-level keys
for schema in /tmp/*_schema.json; do
  echo "=== $(basename $schema) ==="
  cat "$schema" | jq 'keys' 2>/dev/null || echo "Invalid JSON"
done
```

**Decision Tree**:

1. **If all generators output same format and CLI rejects**: Fix CLI
2. **If generators output different formats**: Standardize generators
3. **If format valid but needs transformation**: Create transformer

---

### Task 3.2: Implement Fix (1-2 hours)

**Option A: Fix Generators** (if CLI format is correct)

All generators follow similar pattern. Example fix:

```bash
# Python (fraiseql-python/src/fraiseql/schema.py)
# Update ExportSchema() function to include any missing fields

# TypeScript (fraiseql-typescript/src/schema.ts)
# Update ExportSchema() function

# Java (fraiseql-java/src/main/java/com/fraiseql/core/SchemaFormatter.java)
# Update formatAsJson() method

# Go (fraiseql-go/fraiseql/schema.go)
# Update ExportSchema() function

# PHP (fraiseql-php/src/JsonSchema.php)
# Update export() method
```

**Option B: Fix CLI** (if generators are correct)

```bash
# Update fraiseql-cli/src/compile.rs
# Adjust schema validation/parsing to accept generator format

# Or update fraiseql-cli/src/schema/parser.rs
# if separate parser module exists
```

**Option C: Create Transformer** (if both formats are valid)

```bash
# Create: fraiseql-cli/src/schema/transformer.rs
# Function: fn transform_authoring_schema(input: Value) -> Result<Value>

# Update: fraiseql-cli/src/commands/compile.rs
# Use transformer before compilation
```

---

### Task 3.3: Verify Fix (30 minutes)

**Test all 5 language schemas**:

```bash
cd /home/lionel/code/fraiseql

# Generate fresh schemas
python -c "from fraiseql import schema; schema.export_schema('/tmp/py_test.json')" 2>/dev/null || echo "⚠️ Python"
cd fraiseql-go && go run examples/basic_schema.go > /tmp/go_test.json && cd -
cd fraiseql-typescript && npm run example:basic > /tmp/ts_test.json 2>/dev/null && cd -
# Java/PHP: Generate via tests

# Compile with CLI
for schema in /tmp/*_test.json; do
  echo "Testing: $(basename $schema)"
  fraiseql-cli compile "$schema" && echo "✅ Success" || echo "❌ Failed"
done
```

**Verify schema.compiled.json**:

```bash
# Check output exists and is valid
ls -lh schema.compiled.json*
file schema.compiled.json*
jq . schema.compiled.json* | head -20
```

**Success Criteria**:

- ✅ All 5 schemas compile without errors
- ✅ schema.compiled.json generated for each
- ✅ Compiled schemas are valid JSON
- ✅ Compiled schemas contain expected fields

---

### Task 3.4: Update E2E Tests for Runtime (1 hour)

Now that CLI works, update E2E tests to verify end-to-end:

```bash
# In each E2E test file, enable runtime execution tests:

# Python: tests/e2e/python_e2e_test.py
# Add section to test:
#   1. Load compiled schema
#   2. Start fraiseql-server
#   3. Send GraphQL query
#   4. Validate response

# TypeScript: fraiseql-typescript/tests/e2e/e2e.test.ts
# Add similar runtime execution tests

# (same for Java, Go, PHP)
```

**Example** (Python):

```python
def test_runtime_execution():
    """Test compiled schema with fraiseql-server."""
    # 1. Compile schema
    compiled_path = fraiseql_cli.compile(schema_path)

    # 2. Start server with compiled schema
    server = fraiseql_server.start(compiled_path)

    # 3. Send query
    response = requests.post(
        "http://localhost:8080/graphql",
        json={"query": "{ users { id name } }"}
    )

    # 4. Validate
    assert response.status_code == 200
    assert "data" in response.json()

    # Cleanup
    server.stop()
```

---

### Phase 3 Summary Check

```bash
# Verify all schemas compile
fraiseql-cli compile /tmp/go_schema.json && echo "✅ Go compiles"
fraiseql-cli compile /tmp/python_schema.json && echo "✅ Python compiles"
fraiseql-cli compile /tmp/typescript_schema.json && echo "✅ TypeScript compiles"
fraiseql-cli compile /tmp/java_schema.json && echo "✅ Java compiles"
fraiseql-cli compile /tmp/php_schema.json && echo "✅ PHP compiles"

# Check compiled output
ls -la schema.compiled.json

# Run E2E tests to verify
make e2e-all
```

---

## PHASE 4: DOCUMENTATION & FINALIZATION (Day 4-5 - 2-3 hours)

### Task 4.1: Update Main README

**Location**: `/home/lionel/code/fraiseql/README.md`

**Add section** after introduction:

```markdown
## Language Generators

FraiseQL supports schema authoring in 5 languages:

| Language | Status | Installation | Getting Started |
|----------|--------|--------------|-----------------|
| **Python** | ✅ Production | `pip install fraiseql` | [Guide](fraiseql-python/README.md) |
| **TypeScript** | ✅ Production | `npm install fraiseql` | [Guide](fraiseql-typescript/README.md) |
| **Java** | ✅ Production | Maven/Gradle | [Guide](fraiseql-java/README.md) |
| **Go** | ✅ Production | `go get github.com/fraiseql/fraiseql-go` | [Guide](fraiseql-go/README.md) |
| **PHP** | ✅ Production | Composer | [Guide](fraiseql-php/README.md) |

### Quick Example (Python)

```python
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str

@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    pass

# Export schema
fraiseql.export_schema("schema.json")

# Compile with CLI
# fraiseql-cli compile schema.json

# Run server
# fraiseql-server schema.compiled.json
```

See [Language Generators Documentation](docs/language-generators.md) for details.

```

---

### Task 4.2: Create Language Generators Documentation

**Location**: `/home/lionel/code/fraiseql/docs/language-generators.md`

**Contents**:
```markdown
# Language Generators

FraiseQL supports schema authoring in 5 languages. Each language provides:

- Type decorators/attributes
- Query and mutation builders
- JSON schema export
- Complete documentation and examples

## Available Languages

### Python (pip)
- Status: Production Ready
- Installation: `pip install fraiseql`
- Features: Decorators, type conversion, analytics support
- Documentation: [fraiseql-python/README.md](../fraiseql-python/README.md)

### TypeScript (npm)
- Status: Production Ready
- Installation: `npm install fraiseql`
- Features: Decorators, type conversion, analytics support
- Documentation: [fraiseql-typescript/README.md](../fraiseql-typescript/README.md)

### Java (Maven)
- Status: Production Ready
- Installation: Add to pom.xml
- Features: Annotations, fluent API, validation
- Documentation: [fraiseql-java/README.md](../fraiseql-java/README.md)

### Go (Go Modules)
- Status: Production Ready
- Installation: `go get github.com/fraiseql/fraiseql-go`
- Features: Builders, zero dependencies, thread-safe
- Documentation: [fraiseql-go/README.md](../fraiseql-go/README.md)

### PHP (Composer)
- Status: Production Ready
- Installation: `composer require fraiseql/fraiseql`
- Features: PHP 8 attributes, fluent API, caching
- Documentation: [fraiseql-php/README.md](../fraiseql-php/README.md)

## How It Works

```

Your Code (Python/TypeScript/Java/Go/PHP)
    ↓
@fraiseql decorators/attributes
    ↓
Export to schema.json
    ↓
fraiseql-cli compile
    ↓
schema.compiled.json (optimized SQL templates)
    ↓
fraiseql-server
    ↓
GraphQL API

```

## Testing

All language generators have comprehensive test coverage:

```bash
# Run all E2E tests
make e2e-all

# Run individual language tests
make e2e-python
make e2e-typescript
make e2e-java
make e2e-go
make e2e-php
```

## Contributing

To add a new language:

1. Create fraiseql-{language}/ directory
2. Implement: Type decorators, Registry, JSON export
3. Write: Unit tests (80%+ coverage)
4. Document: README, examples, API reference
5. Test: E2E tests with fraiseql-cli and fraiseql-server

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

```

---

### Task 4.3: Update CONTRIBUTING.md

**Add section** for language generator development:

```markdown
## Language Generator Development

To work on language generators:

```bash
# 1. Choose your language
cd fraiseql-{language}/

# 2. Make changes
# Edit src/, tests/, docs/

# 3. Run tests
# Python: pytest tests/ -v
# TypeScript: npm test
# Java: mvn test
# Go: go test ./...
# PHP: vendor/bin/phpunit

# 4. Run E2E tests
cd ..
make e2e-{language}

# 5. Commit with message
git commit -m "feat({language}): description"
```

## Testing Infrastructure

FraiseQL uses:

- **Docker Compose**: PostgreSQL, MySQL, pgvector for test databases
- **Makefile**: Orchestrates E2E tests across all languages
- **GitHub Actions**: Automated testing on push/PR
- **Language-specific frameworks**: pytest, Jest, JUnit, go test, PHPUnit

See [E2E Testing](docs/e2e-testing.md) for detailed setup.

```

---

### Task 4.4: Create E2E Testing Documentation

**Location**: `/home/lionel/code/fraiseql/docs/e2e-testing.md`

**Contents**:
```markdown
# E2E Testing

FraiseQL uses end-to-end testing to verify all components work together.

## Quick Start

```bash
# Start test infrastructure
make e2e-setup

# Run all E2E tests
make e2e-all

# Stop and cleanup
make e2e-clean
```

## Individual Language Tests

```bash
make e2e-python       # Python E2E
make e2e-typescript   # TypeScript E2E
make e2e-java         # Java E2E
make e2e-go           # Go E2E
make e2e-php          # PHP E2E
```

## Test Infrastructure

### Docker Services

- PostgreSQL 16 (primary)
- PostgreSQL + pgvector (vector tests)
- MySQL 8.3 (secondary support)
- SQLite (local development)

### Test Coverage

Each language tests:

1. **Schema Authoring**: Type decorators/attributes
2. **JSON Export**: Schema generation
3. **CLI Compilation**: fraiseql-cli compile
4. **Runtime Execution**: fraiseql-server GraphQL queries

### CI/CD Pipeline

GitHub Actions runs:

- All 5 language tests in parallel
- With PostgreSQL and MySQL services
- Caches dependencies for speed
- Generates summary reports

## Writing E2E Tests

See language-specific guides:

- [Python E2E Tests](../fraiseql-python/tests/e2e/README.md)
- [TypeScript E2E Tests](../fraiseql-typescript/tests/e2e/README.md)
- [Java E2E Tests](../fraiseql-java/tests/e2e/README.md)
- [Go E2E Tests](../fraiseql-go/fraiseql/e2e_test.go)
- [PHP E2E Tests](../fraiseql-php/tests/e2e/README.md)

```

---

### Task 4.5: Commit and Push

```bash
cd /home/lionel/code/fraiseql

# Stage all changes
git add .

# Status check
git status

# Commit with comprehensive message
git commit -m "feat: Complete language generators E2E testing infrastructure

## Summary
- All 5 language generators production-ready (Python, TypeScript, Java, Go, PHP)
- Comprehensive E2E testing with Makefile orchestration
- GitHub Actions CI/CD pipeline for automated testing
- Docker infrastructure for test databases
- Full documentation and examples

## Changes
- Created E2E tests for all 5 languages
- Implemented Makefile E2E targets (e2e-setup, e2e-all, e2e-clean)
- Set up GitHub Actions workflow (.github/workflows/e2e-tests.yml)
- Fixed CLI schema format compatibility
- Updated main README with language generator info
- Created comprehensive documentation

## Testing
✅ All 5 languages: 45-82 tests passing
✅ E2E infrastructure: Complete and working
✅ CI/CD pipeline: Automated on GitHub Actions
✅ Documentation: Complete with examples

## Verification
- make e2e-setup → Docker containers running
- make e2e-all → All tests passing
- GitHub Actions → Workflow triggers on push
"

# Push to remote
git push origin feature/phase-1-foundation

# Optional: Create pull request (if desired)
# gh pr create --title "Language Generators: E2E Testing Complete" \
#   --body "All 5 language generators production-ready with comprehensive E2E testing"
```

---

## FINAL VERIFICATION CHECKLIST

Before declaring complete, verify:

```bash
#!/bin/bash
echo "=== FINAL VERIFICATION ==="

# Phase 1: Quick Fixes
echo ""
echo "PHASE 1: Quick Fixes"
echo "  Python tests:"
python -m pytest fraiseql-python/tests/ -q 2>&1 | tail -1

echo "  TypeScript tests:"
cd fraiseql-typescript && npm test 2>&1 | grep -E "passed|failed" | tail -1 && cd -

echo "  Java tests:"
mvn test -f fraiseql-java/pom.xml -q 2>&1 | tail -1

echo "  PHP tests:"
cd fraiseql-php && vendor/bin/phpunit tests/ -q 2>&1 | tail -1 && cd -

echo "  Go tests:"
cd fraiseql-go && go test ./fraiseql/... -q 2>&1 | tail -1 && cd -

# Phase 2: E2E Infrastructure
echo ""
echo "PHASE 2: E2E Infrastructure"
[ -f tests/e2e/python_e2e_test.py ] && echo "  ✅ Python E2E test" || echo "  ❌ Python E2E missing"
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "  ✅ TypeScript E2E test" || echo "  ❌ TypeScript E2E missing"
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "  ✅ Java E2E test" || echo "  ❌ Java E2E missing"
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "  ✅ Go E2E test" || echo "  ❌ Go E2E missing"
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "  ✅ PHP E2E test" || echo "  ❌ PHP E2E missing"

grep -q "e2e-all" Makefile && echo "  ✅ Makefile E2E targets" || echo "  ❌ Makefile missing"
[ -f .github/workflows/e2e-tests.yml ] && echo "  ✅ GitHub Actions" || echo "  ❌ GitHub Actions missing"

# Phase 3: CLI Integration
echo ""
echo "PHASE 3: CLI Integration"
fraiseql-cli compile /tmp/go_schema.json > /dev/null 2>&1 && echo "  ✅ CLI compiles Go schema" || echo "  ❌ CLI compilation failed"

# Phase 4: Documentation
echo ""
echo "PHASE 4: Documentation"
grep -q "Language Generators" README.md && echo "  ✅ README updated" || echo "  ❌ README not updated"
[ -f docs/language-generators.md ] && echo "  ✅ Language generators docs" || echo "  ❌ Docs missing"
[ -f docs/e2e-testing.md ] && echo "  ✅ E2E testing docs" || echo "  ❌ E2E docs missing"

echo ""
echo "=== VERIFICATION COMPLETE ==="
```

---

## IMPLEMENTATION SUMMARY

| Phase | Task | Effort | Timeline | Status |
|-------|------|--------|----------|--------|
| 1 | Python (pip install) | 5 min | Day 1 AM | Ready |
| 1 | TypeScript (config) | 15 min | Day 1 AM | Ready |
| 1 | Java (Maven install) | 10 min | Day 1 AM | Ready |
| 1 | PHP (Composer install) | 5 min | Day 1 AM | Ready |
| 1 | CLI investigation | 2-4 hrs | Day 1 PM | Ready |
| 2 | E2E test files | 4 hrs | Day 2 | Ready |
| 2 | Makefile targets | 2 hrs | Day 2 PM | Ready |
| 2 | GitHub Actions | 3 hrs | Day 3 AM | Ready |
| 3 | CLI fix | 1-2 hrs | Day 3 PM | Ready |
| 4 | Documentation | 2-3 hrs | Day 4-5 | Ready |
| | **TOTAL** | **16-18 hrs** | **3-4 days** | ✅ Ready |

---

## SUCCESS CRITERIA

### ✅ All Tests Passing

- Python: 7/7 tests
- TypeScript: 10/10 tests + 2 examples
- Java: 82/82 tests
- Go: 45/45 tests
- PHP: 40+/40+ tests

### ✅ CLI Integration Working

- All 5 languages compile: `fraiseql-cli compile schema.json`
- Generates: `schema.compiled.json`
- fraiseql-server can load compiled schemas

### ✅ E2E Infrastructure Complete

- `make e2e-setup` starts Docker
- `make e2e-all` runs all 5 languages
- `make e2e-clean` stops and cleanup
- GitHub Actions workflow triggers automatically

### ✅ Documentation Complete

- README.md updated
- Language generators guide created
- E2E testing guide created
- Contributing guide updated

### ✅ Ready for Production

- All tests passing in CI/CD
- No known issues or blockers
- Comprehensive documentation
- Ready for package releases

---

## NEXT PHASE (Week 2): PACKAGE RELEASES

After this plan is complete:

1. **PyPI Release** (Python)

   ```bash
   cd fraiseql-python
   python -m build
   python -m twine upload dist/*
   ```

2. **NPM Release** (TypeScript)

   ```bash
   cd fraiseql-typescript
   npm publish
   ```

3. **Maven Central** (Java)
   - Configure pom.xml with credentials
   - `mvn deploy`

4. **Go Modules** (Already available)
   - Tag release: `git tag v0.1.0`
   - `git push --tags`

5. **Packagist** (PHP)
   - Register on packagist.org
   - Add repository webhook

---

**Document Version**: 1.0
**Created**: January 16, 2026
**Status**: Complete implementation plan, ready to execute
**Next Action**: Start Phase 1 Task 1.1 (Python pip install)
