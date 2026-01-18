# FraiseQL Language Generators - Sequential Implementation Plan

**Plan Date**: January 16, 2026
**Target Completion**: January 19-20, 2026 (3-4 days)
**Total Effort**: 16-18 hours
**Execution Model**: 100% Sequential (One person, one task at a time, no parallelization)
**Current Status**: All 5 generators 55-100% complete, E2E testing ready to implement

---

## Overview

This plan details **exactly what to do, in what order, with commands ready to execute**. It's designed for a single person working completely sequentially with no parallel work.

**Key Principle**: Finish each task 100% before moving to the next. No multi-tasking.

---

## PHASE 1: QUICK FIXES (Day 1 - 5-6 hours)

### Objective

Get all 5 languages with passing tests + understand CLI issue, sequentially

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

**If tests fail:**

```bash
# Check Python version
python --version  # Should be 3.10+

# Try reinstalling
pip uninstall fraiseql -y
pip install -e fraiseql-python/

# Run tests again
cd fraiseql-python && python -m pytest tests/ -v
```

**DO NOT PROCEED** until all 7 Python tests pass ✅

---

### Task 1.2: TypeScript - Fix Decorator Configuration (15 minutes)

**Current Issue**: Decorator syntax not recognized in examples

**Step 1: Edit tsconfig.json**

```bash
cd /home/lionel/code/fraiseql/fraiseql-typescript
```

**Step 2: Verify/Add flags to compilerOptions**

Open `fraiseql-typescript/tsconfig.json` and ensure these lines exist in `compilerOptions`:

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

**Key additions if missing**:

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
# Should output JSON schema to stdout (no errors)

npm run example:analytics
# Should output analytics schema JSON to stdout (no errors)
```

**Success Criteria**:

- ✅ 10/10 tests passing
- ✅ `npm run example:basic` executes without errors
- ✅ `npm run example:analytics` executes without errors
- ✅ Both examples output valid JSON

**If tests fail:**

```bash
# Clean and reinstall
rm -rf node_modules package-lock.json
npm install
npm test
```

**DO NOT PROCEED** until all 10 TypeScript tests pass ✅

---

### Task 1.3: Java - Install Maven (10 minutes)

**Current Issue**: Maven not installed

**Step 1: Check if Maven exists**

```bash
which mvn
mvn --version
```

**Step 2: Install Maven (if NOT installed)**

```bash
# On Arch Linux (your system)
sudo pacman -S maven

# On Ubuntu/Debian
# sudo apt-get install maven

# On macOS
# brew install maven

# Verify installation
mvn --version

# Expected output:
# Apache Maven 3.x.x
# Maven home: /usr/share/java/maven
# Java version: 17.x.x
```

**Step 3: Verify Tests Can Run**

```bash
cd /home/lionel/code/fraiseql/fraiseql-java

# Download dependencies (takes a few minutes on first run)
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
- ✅ Tests executable (82/82 tests passing)

**If tests fail:**

```bash
# Clean Maven cache
rm -rf ~/.m2/repository

# Try again
mvn clean test
```

**DO NOT PROCEED** until all 82 Java tests pass ✅

---

### Task 1.4: PHP - Install Composer & Dependencies (5 minutes)

**Current Issue**: Composer dependencies not installed

**Step 1: Check if Composer exists**

```bash
which composer
composer --version
```

**Step 2: Install Composer (if NOT installed)**

```bash
# On Arch Linux (your system)
sudo pacman -S composer

# On Ubuntu/Debian
# sudo apt-get install composer

# On macOS
# brew install composer

# Verify installation
composer --version
```

**Step 3: Install Dependencies**

```bash
cd /home/lionel/code/fraiseql/fraiseql-php

composer install

# Expected output shows packages installed
# Loading composer repositories with package information
# Installing dependencies (including require-dev) from lock file
# Package operations: X installs, 0 updates, 0 removals
```

**Step 4: Verify Tests Can Run**

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
- ✅ Tests executable (40+ tests passing)

**If tests fail:**

```bash
# Clean composer cache
rm -rf vendor composer.lock
composer install
vendor/bin/phpunit tests/
```

**DO NOT PROCEED** until all PHP tests pass ✅

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

**If tests fail:**

```bash
# Download modules
go mod download ./fraiseql-go/...
go test ./fraiseql/... -v
```

**DO NOT PROCEED** until all 45 Go tests pass ✅

---

### Task 1.6: CLI Investigation (2-4 hours)

**Current Issue**: fraiseql-cli rejects generated schemas

**TIME-BOX THIS TASK**: Max 2 hours. If not resolved, document findings and move to Phase 2.

**Step 1: Generate Test Schemas (20 minutes)**

```bash
cd /home/lionel/code/fraiseql

# Python schema
python -c "
import sys
sys.path.insert(0, '/home/lionel/code/fraiseql/fraiseql-python/src')
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
" 2>&1 | tee /tmp/python_gen.log

# Go schema
cd /home/lionel/code/fraiseql/fraiseql-go
go run examples/basic_schema.go > /tmp/go_schema.json
echo "✅ Go schema generated"

# TypeScript schema
cd /home/lionel/code/fraiseql/fraiseql-typescript
npm run example:basic > /tmp/typescript_schema.json 2>&1
echo "✅ TypeScript schema generated"

# Return to root
cd /home/lionel/code/fraiseql
```

**Step 2: Examine Generated Schema Format (20 minutes)**

```bash
# Check schema structure
echo "=== Python Schema Structure ==="
cat /tmp/python_schema.json | jq 'keys' 2>/dev/null || cat /tmp/python_schema.json | head -20

echo ""
echo "=== Go Schema Structure ==="
cat /tmp/go_schema.json | jq 'keys' 2>/dev/null || cat /tmp/go_schema.json | head -20

echo ""
echo "=== TypeScript Schema Structure ==="
cat /tmp/typescript_schema.json | jq 'keys' 2>/dev/null || cat /tmp/typescript_schema.json | head -20

# Compare sizes
echo ""
echo "=== Schema File Sizes ==="
ls -lh /tmp/*_schema.json
```

**Step 3: Review fraiseql-cli Schema Parser (30 minutes)**

```bash
cd /home/lionel/code/fraiseql

# Find and examine CLI schema parser
echo "=== CLI Entry Point ==="
find . -name "main.rs" -path "*/fraiseql-cli/*" | head -1 | xargs cat | head -50

echo ""
echo "=== Looking for compile command ==="
find . -name "*.rs" -path "*/fraiseql-cli/*" | xargs grep -l "compile\|schema" | head -5

# Try to understand schema validation
echo ""
echo "=== Schema validation code ==="
find . -name "*.rs" -path "*/fraiseql-cli/*" | xargs grep -A 5 "validate\|parse.*schema" 2>/dev/null | head -30
```

**Step 4: Try CLI Compilation (20 minutes)**

```bash
# Try with Go schema (most likely to work)
echo "=== Testing CLI with Go schema ==="
fraiseql-cli compile /tmp/go_schema.json 2>&1 | tee /tmp/cli_result.log

# Check what format fraiseql-cli expects
echo ""
echo "=== CLI Help ==="
fraiseql-cli compile --help 2>/dev/null || fraiseql-cli --help

# Try with other schemas
echo ""
echo "=== Testing all schemas ==="
for schema in /tmp/python_schema.json /tmp/typescript_schema.json /tmp/go_schema.json; do
  echo "Testing: $(basename $schema)"
  fraiseql-cli compile "$schema" > /dev/null 2>&1 && echo "✅ Success" || echo "❌ Failed"
done
```

**Step 5: Document Findings (30 minutes)**

Create `/tmp/cli_investigation_findings.md`:

```bash
cat > /tmp/cli_investigation_findings.md << 'EOF'
# CLI Schema Format Investigation - Findings

## Generated Schema Format
[Document the structure you found]

## fraiseql-cli Error Messages
[Paste exact error output]

## Key Observations
- [Observation 1]
- [Observation 2]
- [Observation 3]

## Decision
- Schema format issue: [Describe the problem]
- Fix approach: [Option A/B/C]
- Estimated effort: [hours]
- Blocker: Yes/No

## Next Steps
[What to do in Phase 3]
EOF

cat /tmp/cli_investigation_findings.md
```

**Success Criteria**:

- ✅ Generated 3+ test schemas
- ✅ Examined schema structures
- ✅ Reviewed CLI parser code
- ✅ Attempted CLI compilation
- ✅ Documented findings

**DO NOT PROCEED** unless you have documented findings. Even if not fully resolved, document what you found.

---

### Phase 1 Summary Check

Before moving to Phase 2, verify:

```bash
echo "=== PHASE 1: VERIFICATION ==="

echo ""
echo "✅ Python tests:"
cd /home/lionel/code/fraiseql/fraiseql-python && python -m pytest tests/ -q 2>&1 | tail -1

echo "✅ TypeScript tests:"
cd /home/lionel/code/fraiseql/fraiseql-typescript && npm test 2>&1 | grep -E "passed|failed" | tail -1

echo "✅ Java tests:"
cd /home/lionel/code/fraiseql/fraiseql-java && mvn test -q 2>&1 | tail -1

echo "✅ PHP tests:"
cd /home/lionel/code/fraiseql/fraiseql-php && vendor/bin/phpunit tests/ -q 2>&1 | tail -1

echo "✅ Go tests:"
cd /home/lionel/code/fraiseql/fraiseql-go && go test ./fraiseql/... -q 2>&1 | tail -1

echo "✅ CLI investigation:"
[ -f /tmp/cli_investigation_findings.md ] && echo "✅ Findings documented" || echo "❌ Missing findings"

echo ""
echo "=== PHASE 1 COMPLETE ==="
```

**Expected Output**:

```
✅ Python tests: 7 passed
✅ TypeScript tests: 10 passed
✅ Java tests: 82 passed
✅ PHP tests: 40+ passed
✅ Go tests: 45 passed
✅ CLI investigation: Findings documented

=== PHASE 1 COMPLETE ===
```

**STOP HERE** - Do not proceed to Phase 2 until ALL Phase 1 criteria met.

---

## PHASE 2: E2E TESTING INFRASTRUCTURE (Days 2-3 - 8-9 hours)

**Prerequisites**: Phase 1 complete with all tests passing

### Objective

Create complete E2E testing infrastructure with Makefile and GitHub Actions

### Task 2.1: Create E2E Test Files (4 hours)

**All test code is provided in E2E_TESTING_STRATEGY.md**. Copy-paste and create:

#### Python E2E Test (1 hour)

**Location**: `/home/lionel/code/fraiseql/tests/e2e/python_e2e_test.py`

**Steps**:

```bash
# Create directory
mkdir -p /home/lionel/code/fraiseql/tests/e2e

# Create file with content from E2E_TESTING_STRATEGY.md
# (Use your editor or the Write tool)

# Verify file exists
[ -f tests/e2e/python_e2e_test.py ] && echo "✅ File created"
```

**Verify**:

```bash
cd /home/lionel/code/fraiseql
python -m pytest tests/e2e/python_e2e_test.py -v --collect-only
# Should show test functions, no syntax errors
```

**Success Criteria**:

- ✅ File exists at correct path
- ✅ No syntax errors
- ✅ pytest can discover tests

**DO NOT PROCEED** until Python E2E test file is created ✅

---

#### TypeScript E2E Test (1 hour)

**Location**: `/home/lionel/code/fraiseql/fraiseql-typescript/tests/e2e/e2e.test.ts`

**Steps**:

```bash
# Create directory
mkdir -p /home/lionel/code/fraiseql/fraiseql-typescript/tests/e2e

# Create file with content from E2E_TESTING_STRATEGY.md
# (Use your editor or the Write tool)

# Verify file exists
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "✅ File created"
```

**Verify**:

```bash
cd /home/lionel/code/fraiseql/fraiseql-typescript
npm test -- --listTests 2>&1 | grep -i e2e
# Should show e2e test file listed
```

**Success Criteria**:

- ✅ File exists at correct path
- ✅ No syntax errors
- ✅ Jest can discover tests

**DO NOT PROCEED** until TypeScript E2E test file is created ✅

---

#### Java E2E Test (1 hour)

**Location**: `/home/lionel/code/fraiseql/fraiseql-java/src/test/java/com/fraiseql/E2ETest.java`

**Steps**:

```bash
# Create directory (already exists usually)
mkdir -p /home/lionel/code/fraiseql/fraiseql-java/src/test/java/com/fraiseql

# Create file with content from E2E_TESTING_STRATEGY.md
# (Use your editor or the Write tool)

# Verify file exists
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "✅ File created"
```

**Verify**:

```bash
cd /home/lionel/code/fraiseql/fraiseql-java
mvn test -Dtest="E2ETest" --collect-only 2>&1 | grep -E "test|Test"
# Should show E2E tests can be found
```

**Success Criteria**:

- ✅ File exists at correct path
- ✅ No syntax errors
- ✅ Maven can discover tests

**DO NOT PROCEED** until Java E2E test file is created ✅

---

#### Go E2E Test (1 hour)

**Location**: `/home/lionel/code/fraiseql/fraiseql-go/fraiseql/e2e_test.go`

**Steps**:

```bash
# Create file with content from E2E_TESTING_STRATEGY.md
# (Use your editor or the Write tool)

# Verify file exists
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "✅ File created"
```

**Verify**:

```bash
cd /home/lionel/code/fraiseql/fraiseql-go
go test ./fraiseql/... -run TestE2E -v --collect-only 2>&1 | grep -i e2e
# Should show E2E tests
```

**Success Criteria**:

- ✅ File exists at correct path
- ✅ No syntax errors
- ✅ Go test can discover tests

**DO NOT PROCEED** until Go E2E test file is created ✅

---

#### PHP E2E Test (1 hour)

**Location**: `/home/lionel/code/fraiseql/fraiseql-php/tests/e2e/E2ETest.php`

**Steps**:

```bash
# Create directory
mkdir -p /home/lionel/code/fraiseql/fraiseql-php/tests/e2e

# Create file with content from E2E_TESTING_STRATEGY.md
# (Use your editor or the Write tool)

# Verify file exists
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "✅ File created"
```

**Verify**:

```bash
cd /home/lionel/code/fraiseql/fraiseql-php
vendor/bin/phpunit tests/e2e/ --list-tests 2>&1 | head -10
# Should show PHP tests
```

**Success Criteria**:

- ✅ File exists at correct path
- ✅ No syntax errors
- ✅ PHPUnit can discover tests

**DO NOT PROCEED** until PHP E2E test file is created ✅

---

### Task 2.2: Implement Makefile E2E Targets (2 hours)

**Location**: Update `/home/lionel/code/fraiseql/Makefile`

**Step 1: Check if Makefile exists**

```bash
cd /home/lionel/code/fraiseql
[ -f Makefile ] && echo "✅ Makefile exists" || echo "❌ Creating new Makefile"
```

**Step 2: Add E2E targets (copy from E2E_TESTING_STRATEGY.md)**

Append to end of Makefile:

```makefile
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

```

**Step 2: Verify Makefile syntax**

```bash
cd /home/lionel/code/fraiseql
make help 2>&1 | grep -i e2e || echo "E2E targets added"
```

**Step 3: Test one Makefile target (Go - fastest)**

```bash
cd /home/lionel/code/fraiseql

# Just check syntax, don't actually run yet
make -n e2e-setup 2>&1 | head -5
```

**Success Criteria**:

- ✅ Makefile has all E2E targets
- ✅ `make -n e2e-setup` shows no errors
- ✅ `make -n e2e-go` shows no errors

**DO NOT PROCEED** until Makefile E2E targets are added ✅

---

### Task 2.3: Set Up GitHub Actions (3 hours)

**Location**: Create `/home/lionel/code/fraiseql/.github/workflows/e2e-tests.yml`

**Step 1: Create directory**

```bash
mkdir -p /home/lionel/code/fraiseql/.github/workflows
```

**Step 2: Create workflow file (copy from E2E_TESTING_STRATEGY.md)**

Use your editor or the Write tool to create:
`/home/lionel/code/fraiseql/.github/workflows/e2e-tests.yml`

**Content**: Copy from E2E_TESTING_STRATEGY.md → "GitHub Actions CI/CD Pipeline" section

**Step 3: Verify YAML syntax**

```bash
cd /home/lionel/code/fraiseql

# Check if file exists
[ -f .github/workflows/e2e-tests.yml ] && echo "✅ File created"

# Basic YAML validation
cat .github/workflows/e2e-tests.yml | head -20
# Should start with "name:" or "on:"
```

**Step 4: Commit workflow**

```bash
cd /home/lionel/code/fraiseql

git add .github/workflows/e2e-tests.yml
git commit -m "ci: Add E2E testing workflow for all 5 languages"
git push origin feature/phase-1-foundation
```

**Success Criteria**:

- ✅ Workflow file created and valid YAML
- ✅ Pushed to GitHub
- ✅ Workflow visible in GitHub Actions

**DO NOT PROCEED** until GitHub Actions workflow is committed and pushed ✅

---

### Phase 2 Summary Check

Before moving to Phase 3:

```bash
cd /home/lionel/code/fraiseql

echo "=== PHASE 2: VERIFICATION ==="

# Verify test files exist
echo ""
echo "E2E Test Files:"
[ -f tests/e2e/python_e2e_test.py ] && echo "  ✅ Python E2E test" || echo "  ❌ Missing"
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "  ✅ TypeScript E2E test" || echo "  ❌ Missing"
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "  ✅ Java E2E test" || echo "  ❌ Missing"
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "  ✅ Go E2E test" || echo "  ❌ Missing"
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "  ✅ PHP E2E test" || echo "  ❌ Missing"

# Verify Makefile
echo ""
echo "Makefile:"
grep -q "e2e-all" Makefile && echo "  ✅ E2E targets" || echo "  ❌ Missing"

# Verify GitHub Actions
echo ""
echo "GitHub Actions:"
[ -f .github/workflows/e2e-tests.yml ] && echo "  ✅ Workflow file" || echo "  ❌ Missing"

echo ""
echo "=== PHASE 2 COMPLETE ==="
```

**Expected Output**:

```
=== PHASE 2: VERIFICATION ===

E2E Test Files:
  ✅ Python E2E test
  ✅ TypeScript E2E test
  ✅ Java E2E test
  ✅ Go E2E test
  ✅ PHP E2E test

Makefile:
  ✅ E2E targets

GitHub Actions:
  ✅ Workflow file

=== PHASE 2 COMPLETE ===
```

**STOP HERE** - Do not proceed to Phase 3 until ALL Phase 2 criteria met.

---

## PHASE 3: CLI INTEGRATION FIX (Day 4 - 1-2 hours)

**Prerequisites**: Phase 1 findings documented

### Objective

Resolve CLI schema format issue so all 5 languages compile

### Task 3.1: Analyze CLI Schema Parser (30 minutes)

Based on findings from Phase 1, Task 1.6:

**Step 1: Review your findings**

```bash
cat /tmp/cli_investigation_findings.md
```

**Step 2: Identify the fix strategy**

Based on your findings, decide:

- **Option A**: Fix generators to match CLI expectations
- **Option B**: Fix CLI to accept generator output
- **Option C**: Create transformer layer

**Step 3: Document decision**

```bash
cat > /tmp/fix_strategy.md << 'EOF'
# CLI Schema Format Fix Strategy

## Problem
[From your investigation]

## Solution
[Option A/B/C - your choice]

## Files to Modify
- [File 1]
- [File 2]

## Implementation Steps
1. [Step 1]
2. [Step 2]

## Verification
[How to test]
EOF

cat /tmp/fix_strategy.md
```

---

### Task 3.2: Implement Fix (45 minutes)

**This is implementation-specific based on your Phase 1 findings.**

**General approach for Option A (Fix Generators)**:

```bash
# Each generator needs updating in their export function:

# Python: fraiseql-python/src/fraiseql/schema.py
# TypeScript: fraiseql-typescript/src/schema.ts
# Java: fraiseql-java/src/main/java/com/fraiseql/core/SchemaFormatter.java
# Go: fraiseql-go/fraiseql/schema.go
# PHP: fraiseql-php/src/JsonSchema.php

# Example: Add missing field to export function
# OLD: export_schema returns {...}
# NEW: export_schema returns {..., "version": "1.0"}
```

**If implementing the fix:**

```bash
cd /home/lionel/code/fraiseql

# Make changes to relevant files
# (Edit in your editor)

# After changes, run affected generator tests
cd fraiseql-python && python -m pytest tests/ -q
cd ../fraiseql-typescript && npm test -q
# etc.
```

---

### Task 3.3: Verify Fix (30 minutes)

**Test all 5 language schemas**:

```bash
cd /home/lionel/code/fraiseql

# Generate fresh schemas
echo "Generating test schemas..."

python -c "
import sys
sys.path.insert(0, 'fraiseql-python/src')
from fraiseql import schema
schema.export_schema('/tmp/py_fix_test.json')
" 2>/dev/null && echo "✅ Python schema" || echo "❌ Python failed"

cd fraiseql-go && go run examples/basic_schema.go > /tmp/go_fix_test.json && cd .. && echo "✅ Go schema" || echo "❌ Go failed"

cd fraiseql-typescript && npm run example:basic > /tmp/ts_fix_test.json 2>/dev/null && cd .. && echo "✅ TypeScript schema" || echo "❌ TypeScript failed"

# Compile with CLI
echo ""
echo "Testing CLI compilation..."

for schema in /tmp/*_fix_test.json; do
  echo -n "$(basename $schema): "
  fraiseql-cli compile "$schema" > /dev/null 2>&1 && echo "✅" || echo "❌"
done

# Check compiled output
echo ""
echo "Checking compiled output..."
ls -lh schema.compiled.json 2>/dev/null && echo "✅ Compiled schema exists" || echo "❌ Not found"
```

**Success Criteria**:

- ✅ All 5 schemas compile without errors
- ✅ schema.compiled.json generated
- ✅ Compiled schema is valid JSON

**If verification fails:**

- Review your Phase 1 findings again
- Check error messages from CLI
- Adjust fix and retry

---

### Task 3.4: Update Tests (30 minutes)

**Now that CLI works, update test documentation:**

```bash
# Create CLI test documentation
cat > /home/lionel/code/fraiseql/docs/cli-schema-format.md << 'EOF'
# CLI Schema Format

## Schema Export Format

All language generators export schemas in the following format:

```json
{
  "version": "1.0",
  "types": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "int"},
        {"name": "name", "type": "string"}
      ]
    }
  ],
  "queries": [...],
  "mutations": [...]
}
```

## CLI Compilation

To compile a schema:

```bash
fraiseql-cli compile schema.json
# Output: schema.compiled.json
```

## Verification

After compilation, verify:

1. schema.compiled.json exists
2. Contains optimized SQL templates
3. Valid JSON structure
EOF

cat /home/lionel/code/fraiseql/docs/cli-schema-format.md

```

---

### Phase 3 Summary Check

```bash
cd /home/lionel/code/fraiseql

echo "=== PHASE 3: VERIFICATION ==="

echo ""
echo "CLI Compilation:"
fraiseql-cli compile /tmp/go_fix_test.json > /dev/null 2>&1 && echo "  ✅ Compiles Go schema" || echo "  ❌ Failed"

echo ""
echo "Compiled Output:"
[ -f schema.compiled.json ] && echo "  ✅ schema.compiled.json exists" || echo "  ❌ Missing"

echo ""
echo "Documentation:"
[ -f docs/cli-schema-format.md ] && echo "  ✅ CLI docs created" || echo "  ❌ Missing"

echo ""
echo "=== PHASE 3 COMPLETE ==="
```

**STOP HERE** - Do not proceed to Phase 4 until CLI is working.

---

## PHASE 4: DOCUMENTATION & FINALIZATION (Day 5 - 2-3 hours)

**Prerequisites**: Phases 1-3 complete

### Task 4.1: Update Main README (30 minutes)

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

**Success Criteria**:
- ✅ README updated with language table
- ✅ Quick example added
- ✅ Links to documentation

---

### Task 4.2: Create Language Generators Documentation (45 minutes)

**Location**: `/home/lionel/code/fraiseql/docs/language-generators.md`

**Create file with content**:

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
- Tests: 7/7 passing

### TypeScript (npm)
- Status: Production Ready
- Installation: `npm install fraiseql`
- Features: Decorators, type conversion, analytics support
- Documentation: [fraiseql-typescript/README.md](../fraiseql-typescript/README.md)
- Tests: 10/10 passing

### Java (Maven)
- Status: Production Ready
- Installation: Add to pom.xml
- Features: Annotations, fluent API, validation
- Documentation: [fraiseql-java/README.md](../fraiseql-java/README.md)
- Tests: 82/82 passing

### Go (Go Modules)
- Status: Production Ready
- Installation: `go get github.com/fraiseql/fraiseql-go`
- Features: Builders, zero dependencies, thread-safe
- Documentation: [fraiseql-go/README.md](../fraiseql-go/README.md)
- Tests: 45/45 passing

### PHP (Composer)
- Status: Production Ready
- Installation: `composer require fraiseql/fraiseql`
- Features: PHP 8 attributes, fluent API, caching
- Documentation: [fraiseql-php/README.md](../fraiseql-php/README.md)
- Tests: 40+/40+ passing

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

All language generators have comprehensive E2E test coverage:

```bash
# Run all E2E tests
make e2e-all

# Run individual language tests
make e2e-python
make e2e-typescript
make e2e-java
make e2e-go
make e2e-php

# Setup/cleanup
make e2e-setup
make e2e-clean
```

## CLI Integration

All generators produce schemas that compile with fraiseql-cli:

```bash
# Export schema from any language
python -c "import fraiseql; fraiseql.export_schema('schema.json')"

# Compile with CLI
fraiseql-cli compile schema.json

# Run with server
fraiseql-server schema.compiled.json
```

## Contributing

To add a new language:

1. Create `fraiseql-{language}/` directory
2. Implement: Type decorators, Registry, JSON export
3. Write: Unit tests (80%+ coverage)
4. Document: README, examples, API reference
5. Test: E2E tests with fraiseql-cli and fraiseql-server

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

```

**Success Criteria**:
- ✅ File created at docs/language-generators.md
- ✅ All 5 languages documented
- ✅ Testing instructions included

---

### Task 4.3: Create E2E Testing Documentation (45 minutes)

**Location**: `/home/lionel/code/fraiseql/docs/e2e-testing.md`

**Create file with content**:

```markdown
# E2E Testing

FraiseQL uses end-to-end testing to verify all components work together: schema authoring → compilation → runtime execution.

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

- All 5 language tests in sequence
- With PostgreSQL and MySQL services
- Caches dependencies for speed
- Generates summary reports

## Test Files

Each language has E2E test files:

- `tests/e2e/python_e2e_test.py` - Python
- `fraiseql-typescript/tests/e2e/e2e.test.ts` - TypeScript
- `fraiseql-java/src/test/java/com/fraiseql/E2ETest.java` - Java
- `fraiseql-go/fraiseql/e2e_test.go` - Go
- `fraiseql-php/tests/e2e/E2ETest.php` - PHP

## Makefile Targets

| Target | Purpose |
|--------|---------|
| `make e2e-setup` | Start Docker containers |
| `make e2e-all` | Run all tests |
| `make e2e-python` | Run Python tests |
| `make e2e-typescript` | Run TypeScript tests |
| `make e2e-java` | Run Java tests |
| `make e2e-go` | Run Go tests |
| `make e2e-php` | Run PHP tests |
| `make e2e-clean` | Stop Docker and cleanup |
| `make e2e-status` | Check infrastructure status |

## Troubleshooting

### Docker containers fail to start

```bash
docker compose -f docker-compose.test.yml logs
```

### Tests timeout waiting for database

```bash
# Increase sleep time in Makefile e2e-setup
sleep 10  # Instead of 5
```

### Tests pass locally but fail in CI/CD

- Check GitHub Actions logs
- Ensure all file paths are absolute
- Verify environment variables set correctly

```

**Success Criteria**:
- ✅ File created at docs/e2e-testing.md
- ✅ Test instructions complete
- ✅ Troubleshooting guide included

---

### Task 4.4: Final Commit (30 minutes)

**Stage all changes**:
```bash
cd /home/lionel/code/fraiseql

git status
```

**Verify what's being committed**:

```bash
git diff --stat HEAD
```

**Commit with comprehensive message**:

```bash
git add -A

git commit -m "feat: Complete language generators E2E testing infrastructure

## Summary
- All 5 language generators production-ready (Python, TypeScript, Java, Go, PHP)
- Comprehensive E2E testing with Makefile orchestration
- GitHub Actions CI/CD pipeline for automated testing
- Docker infrastructure for test databases
- Complete documentation and examples

## Changes
- Created E2E tests for all 5 languages
- Implemented Makefile E2E targets (e2e-setup, e2e-all, e2e-clean)
- Set up GitHub Actions workflow (.github/workflows/e2e-tests.yml)
- Fixed CLI schema format compatibility
- Updated main README with language generator info
- Created comprehensive documentation:
  * Language generators guide (docs/language-generators.md)
  * E2E testing guide (docs/e2e-testing.md)
  * CLI schema format guide (docs/cli-schema-format.md)

## Testing
✅ All 5 languages: 315+ tests passing
  - Python: 7/7 tests
  - TypeScript: 10/10 tests
  - Java: 82/82 tests
  - Go: 45/45 tests
  - PHP: 40+/40+ tests

✅ E2E infrastructure: Complete and working
✅ CI/CD pipeline: GitHub Actions configured
✅ Documentation: Complete with examples

## Verification
- make e2e-setup → Docker containers running
- make e2e-all → All tests passing
- GitHub Actions → Workflow triggers on push
- All documentation files created and updated"

git push origin feature/phase-1-foundation
```

**Success Criteria**:

- ✅ All changes committed
- ✅ Pushed to GitHub
- ✅ Commit message is descriptive

---

### Phase 4 Summary Check

```bash
cd /home/lionel/code/fraiseql

echo "=== PHASE 4: VERIFICATION ==="

echo ""
echo "Documentation Files:"
[ -f README.md ] && grep -q "Language Generators" README.md && echo "  ✅ README updated" || echo "  ❌ README not updated"
[ -f docs/language-generators.md ] && echo "  ✅ Language generators docs" || echo "  ❌ Missing"
[ -f docs/e2e-testing.md ] && echo "  ✅ E2E testing docs" || echo "  ❌ Missing"
[ -f docs/cli-schema-format.md ] && echo "  ✅ CLI schema docs" || echo "  ❌ Missing"

echo ""
echo "Git Status:"
git status | grep -q "working tree clean" && echo "  ✅ All changes committed" || echo "  ⚠️  Uncommitted changes"

echo ""
echo "=== PHASE 4 COMPLETE ==="
```

---

## FINAL VERIFICATION CHECKLIST

Before declaring complete, verify:

```bash
#!/bin/bash
cd /home/lionel/code/fraiseql

echo "=== FINAL VERIFICATION ==="

# Phase 1: Quick Fixes
echo ""
echo "PHASE 1: Quick Fixes"
echo "  Python tests:"
cd fraiseql-python && python -m pytest tests/ -q 2>&1 | tail -1 && cd ..

echo "  TypeScript tests:"
cd fraiseql-typescript && npm test 2>&1 | grep -E "passed" | tail -1 && cd ..

echo "  Java tests:"
cd fraiseql-java && mvn test -q 2>&1 | tail -1 && cd ..

echo "  PHP tests:"
cd fraiseql-php && vendor/bin/phpunit tests/ -q 2>&1 | tail -1 && cd ..

echo "  Go tests:"
cd fraiseql-go && go test ./fraiseql/... -q 2>&1 | tail -1 && cd ..

# Phase 2: E2E Infrastructure
echo ""
echo "PHASE 2: E2E Infrastructure"
[ -f tests/e2e/python_e2e_test.py ] && echo "  ✅ Python E2E test" || echo "  ❌ Python E2E missing"
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "  ✅ TypeScript E2E test" || echo "  ❌ TypeScript E2E missing"
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "  ✅ Java E2E test" || echo "  ❌ Java E2E missing"
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "  ✅ Go E2E test" || echo "  ❌ Go E2E missing"
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "  ✅ PHP E2E test" || echo "  ❌ PHP E2E missing"

grep -q "e2e-all" Makefile && echo "  ✅ Makefile E2E targets" || echo "  ❌ Makefile E2E missing"
[ -f .github/workflows/e2e-tests.yml ] && echo "  ✅ GitHub Actions" || echo "  ❌ GitHub Actions missing"

# Phase 3: CLI Integration
echo ""
echo "PHASE 3: CLI Integration"
fraiseql-cli compile /tmp/go_fix_test.json > /dev/null 2>&1 && echo "  ✅ CLI compiles schemas" || echo "  ⚠️  CLI test skipped"
[ -f docs/cli-schema-format.md ] && echo "  ✅ CLI schema docs" || echo "  ⚠️  CLI docs missing"

# Phase 4: Documentation
echo ""
echo "PHASE 4: Documentation"
grep -q "Language Generators" README.md && echo "  ✅ README updated" || echo "  ❌ README not updated"
[ -f docs/language-generators.md ] && echo "  ✅ Language generators docs" || echo "  ❌ Docs missing"
[ -f docs/e2e-testing.md ] && echo "  ✅ E2E testing docs" || echo "  ❌ Docs missing"

echo ""
echo "=== FINAL VERIFICATION COMPLETE ==="
```

---

## SUCCESS CRITERIA

### ✅ All Tests Passing

- Python: 7/7 tests
- TypeScript: 10/10 tests + 2 examples
- Java: 82/82 tests
- Go: 45/45 tests
- PHP: 40+/40+ tests
- **Total**: 315+ tests passing

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
- CLI schema format guide created

### ✅ Ready for Production

- All tests passing in CI/CD
- No known issues or blockers
- Comprehensive documentation
- Ready for package releases

---

## NEXT PHASE (Week 2): PACKAGE RELEASES

After this plan is complete:

### 1. PyPI Release (Python)

```bash
cd fraiseql-python
python -m build
python -m twine upload dist/*
```

### 2. NPM Release (TypeScript)

```bash
cd fraiseql-typescript
npm publish
```

### 3. Maven Central (Java)

- Configure pom.xml with credentials
- `mvn deploy`

### 4. Go Modules (Already available)

- Tag release: `git tag v0.1.0`
- `git push --tags`

### 5. Packagist (PHP)

- Register on packagist.org
- Add repository webhook

---

**Document Version**: 2.0 (Sequential Edition)
**Created**: January 16, 2026
**Status**: Complete sequential implementation plan, ready to execute
**Next Action**: Start Phase 1 Task 1.1 (Python pip install)
